//! Content-addressed single and multipart uploads with a resume journal, verification of stored
//! objects against references, and aborting journaled uploads.
use super::store::Store;
use super::transfer::{
    Failure, Payload, Policy, Transfer, hex, http_client, read_limited, unescape, xml_text,
};
use super::{AssetError, Events, IMMUTABLE, io_error};
use base64::Engine;
use bytes::Bytes;
use futures_util::StreamExt;
use md5::{Digest as _, Md5};
use reqwest::{Method, Url, header};
use richview_core::{AssetRef, AssetUse, Visibility};
use rusty_s3::actions::{CreateMultipartUpload, ListParts};
use rusty_s3::{Bucket, Credentials, S3Action};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

const MIB: u64 = 1 << 20;
/// Larger files use multipart uploads.
const SINGLE_PUT_MAX: u64 = 16 * MIB;
const MIN_PART: u64 = 8 * MIB;
/// Parts per upload stay well under S3's limit of 10,000.
const MAX_PARTS: u64 = 9_000;
const SIGNATURE_TTL: Duration = Duration::from_secs(15 * 60);
const COMPLETE_TIMEOUT: Duration = Duration::from_secs(300);

/// Uniform parts: at least 8 MiB, whole MiB, at most 9,000 parts. R2 requires every part but the
/// last to have the same size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Plan {
    bytes: u64,
    part_size: u64,
    parts: u16,
}

impl Plan {
    fn new(bytes: u64) -> Self {
        if bytes <= SINGLE_PUT_MAX {
            return Self {
                bytes,
                part_size: bytes,
                parts: 1,
            };
        }
        let part_size = (bytes.div_ceil(MAX_PARTS).div_ceil(MIB) * MIB).max(MIN_PART);
        Self {
            bytes,
            part_size,
            parts: bytes.div_ceil(part_size) as u16,
        }
    }

    fn range(&self, part: u16) -> (u64, u64) {
        let start = u64::from(part - 1) * self.part_size;
        (start, self.part_size.min(self.bytes - start))
    }
}

/// `<prefix><use>/<first 16 hex of sha256>/<name>`: new content always gets a new key.
fn content_key(prefix: &str, usage: AssetUse, sha256: &str, name: &str) -> String {
    format!("{prefix}{}/{}/{name}", usage.name(), &sha256[..16])
}

/// A readable key segment from a file name: ASCII letters, digits, '.', '_' and '-'.
fn object_name(file: &Path) -> String {
    let raw = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut name = String::new();
    for c in raw.chars() {
        let c = if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            c
        } else {
            '-'
        };
        if !(c == '-' && name.ends_with('-')) {
            name.push(c);
        }
    }
    let name = name.replace("-.", ".");
    let name = name.trim_matches(['-', '.']);
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let stem: String = stem.chars().take(96 - extension.len().min(16)).collect();
    match (stem.is_empty(), extension.is_empty()) {
        (true, _) => "asset".into(),
        (false, true) => stem,
        (false, false) => format!("{stem}.{extension}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    bytes: u64,
    modified_ns: u64,
}

fn stamp(metadata: &std::fs::Metadata) -> Stamp {
    Stamp {
        bytes: metadata.len(),
        modified_ns: metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos() as u64),
    }
}

async fn hash_file(path: &Path) -> Result<(String, Stamp), AssetError> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path).map_err(|e| io_error("Cannot open", &path, e))?;
        let before = stamp(
            &file
                .metadata()
                .map_err(|e| io_error("Cannot stat", &path, e))?,
        );
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; MIB as usize];
        let mut total = 0;
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|e| io_error("Cannot read", &path, e))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            hasher.update(&buffer[..n]);
        }
        if total != before.bytes {
            return Err(AssetError::mismatch(format!(
                "{} changed while it was hashed",
                path.display()
            )));
        }
        Ok((hex(&hasher.finalize()), before))
    })
    .await
    .map_err(|e| AssetError::transfer(format!("hashing stopped: {e}")))?
}

async fn read_range(path: &Path, offset: u64, size: u64) -> Result<Bytes, AssetError> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| io_error("Cannot open", path, e))?;
    file.seek(std::io::SeekFrom::Start(offset))
        .await
        .map_err(|e| io_error("Cannot seek", path, e))?;
    let mut buffer = vec![0; size as usize];
    file.read_exact(&mut buffer)
        .await
        .map_err(|e| io_error("Cannot read", path, e))?;
    Ok(Bytes::from(buffer))
}

fn content_md5(body: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Md5::digest(body))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Part {
    n: u16,
    etag: String,
    size: u64,
}

/// Resume state of one multipart upload. It holds no credentials or signed URLs.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    store: String,
    bucket: String,
    key: String,
    upload_id: String,
    file: PathBuf,
    sha256: String,
    #[serde(rename = "type")]
    media_type: String,
    visibility: Visibility,
    stamp: Stamp,
    part_size: u64,
    parts: Vec<Part>,
}

impl Journal {
    fn path(dir: &Path, store: &str, bucket: &str, key: &str, sha256: &str) -> PathBuf {
        let digest = hex(&Sha256::digest(format!("{store}\n{bucket}\n{key}")));
        dir.join(format!("{sha256}.{}.json", &digest[..16]))
    }

    fn load(path: &Path) -> Result<Option<Self>, AssetError> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|e| {
                AssetError::invalid(format!("Upload journal {}: {e}", path.display()))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_error("Cannot read upload journal", path, e)),
        }
    }

    /// Written after every part; a rename keeps the previous state on interruption.
    fn save(&self, path: &Path) -> Result<(), AssetError> {
        let dir = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir).map_err(|e| io_error("Cannot create", dir, e))?;
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        std::fs::write(&temporary, bytes)
            .and_then(|()| std::fs::rename(&temporary, path))
            .map_err(|e| io_error("Cannot write upload journal", path, e))
    }
}

pub(super) struct Uploader<'a> {
    store: &'a Store,
    transfer: Transfer<'a>,
    journal_dir: PathBuf,
}

impl<'a> Uploader<'a> {
    pub(super) fn new(
        store: &'a Store,
        journal_dir: PathBuf,
        policy: Policy,
        events: Events<'a>,
    ) -> Result<Self, AssetError> {
        let mut urls = vec![store.private.base_url(), store.public.base_url()];
        urls.extend(store.public_base_url.as_ref());
        Ok(Self {
            store,
            transfer: Transfer {
                client: http_client(&urls)?,
                policy,
                events,
                secrets: store.secrets(),
            },
            journal_dir,
        })
    }

    fn credentials(&self) -> Option<&Credentials> {
        Some(&self.store.credentials)
    }

    fn fail(&self, failure: Failure, what: &str) -> AssetError {
        let mut error = failure.into_error(what);
        error.message = self.transfer.redact(&error.message);
        error
    }

    /// Uploads `file` unless the store already holds it and returns the checked reference.
    pub(super) async fn put(
        &self,
        file: &Path,
        visibility: Visibility,
        media_type: Option<&str>,
    ) -> Result<Value, AssetError> {
        let metadata = std::fs::metadata(file).map_err(|e| io_error("Cannot read", file, e))?;
        if !metadata.is_file() {
            return Err(AssetError::invalid(format!(
                "{} is not a file",
                file.display()
            )));
        }
        let extension = file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        let allowed = || {
            AssetUse::ALL
                .iter()
                .flat_map(|u| u.types())
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        };
        let by_extension = richview_core::media_type_for_extension(extension).ok_or_else(|| {
            AssetError::invalid(format!(
                "{} needs the extension of an allowed type: {}",
                file.display(),
                allowed()
            ))
        })?;
        // A declared type may only tell video from audio in the same container (.mp4, .webm).
        let media_type = media_type.unwrap_or(by_extension);
        if media_type.split_once('/').map(|(_, subtype)| subtype)
            != by_extension.split_once('/').map(|(_, subtype)| subtype)
        {
            return Err(AssetError::invalid(format!(
                "Type '{media_type}' does not match the '.{extension}' file, which is {by_extension}"
            )));
        }
        let usage = AssetUse::of_type(media_type).ok_or_else(|| {
            AssetError::invalid(format!(
                "Type '{media_type}' is not allowed; data, image, video and audio types are: {}",
                allowed()
            ))
        })?;
        if metadata.len() == 0 || metadata.len() > self.store.max_bytes {
            return Err(AssetError::invalid(format!(
                "{} has {} bytes; store '{}' accepts 1 to {} bytes",
                file.display(),
                metadata.len(),
                self.store.name,
                self.store.max_bytes
            )));
        }
        let (sha256, stamp) = hash_file(file).await?;
        let prefix = match visibility {
            Visibility::Public => &self.store.public_prefix,
            Visibility::Private => &self.store.private_prefix,
        };
        let key = content_key(prefix, usage, &sha256, &object_name(file));
        let bucket = self.store.bucket(visibility);
        let asset = AssetRef {
            url: self.store.object_url(visibility, &key)?.to_string(),
            bytes: stamp.bytes,
            sha256,
            media_type: media_type.to_owned(),
            visibility,
            duration: None,
            width: None,
            height: None,
        };
        // A reference the renderer would reject is refused before any transfer.
        let problems = asset.validate(usage);
        if !problems.is_empty() {
            return Err(AssetError::invalid(
                problems
                    .iter()
                    .map(|d| format!("{}: {}", d.code, d.message))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        let plan = Plan::new(asset.bytes);
        (self.transfer.events)(json!({
            "event": "start", "file": file, "store": self.store.name, "bucket": bucket.name(),
            "key": key, "bytes": asset.bytes, "sha256": asset.sha256, "type": asset.media_type,
            "visibility": visibility, "parts": plan.parts, "part_size": plan.part_size
        }));
        let journal_path = Journal::path(
            &self.journal_dir,
            &self.store.name,
            bucket.name(),
            &key,
            &asset.sha256,
        );
        let existing = match self
            .transfer
            .head(|| {
                bucket
                    .head_object(self.credentials(), &key)
                    .sign(SIGNATURE_TTL)
            })
            .await
        {
            // S3 answers 403 for a missing object when the credentials may not list the bucket.
            // A real refusal is reported by the upload request, with the store's error document.
            Err(Failure::Status { status: 403, .. }) => None,
            other => other.map_err(|f| self.fail(f, "HEAD"))?,
        };
        let sent_parts = match existing {
            Some(head) if head.problems(&asset, true).is_empty() => {
                if let Some(journal) = Journal::load(&journal_path)? {
                    self.abort_upload(bucket, &journal.key, &journal.upload_id)
                        .await?;
                    let _ = std::fs::remove_file(&journal_path);
                }
                (self.transfer.events)(
                    json!({"event": "exists", "key": key, "sha256": asset.sha256}),
                );
                0
            }
            Some(head) => {
                return Err(AssetError::mismatch(format!(
                    "Another object occupies {key}: {}",
                    head.problems(&asset, true).join("; ")
                )));
            }
            None if plan.parts == 1 => {
                self.put_single(file, bucket, &key, &asset).await?;
                1
            }
            None => {
                self.put_multipart(file, bucket, &key, &asset, stamp, &journal_path)
                    .await?
            }
        };
        if sent_parts > 0 {
            let head = self
                .transfer
                .head(|| bucket.head_object(self.credentials(), &key).sign(SIGNATURE_TTL))
                .await
                .map_err(|f| self.fail(f, "HEAD"))?
                .ok_or_else(|| {
                    AssetError::transfer(format!(
                        "The store has no object at {key} after the upload; rerun to upload it again"
                    ))
                })?;
            let problems = head.problems(&asset, true);
            if !problems.is_empty() {
                return Err(AssetError::mismatch(format!(
                    "The stored object differs: {}",
                    problems.join("; ")
                )));
            }
        }
        let asset_json = serde_json::to_value(&asset).unwrap_or_default();
        (self.transfer.events)(json!({"event": "done", "asset": asset_json}));
        Ok(json!({
            "ok": true, "asset": asset_json, "store": self.store.name, "bucket": bucket.name(),
            "key": key, "uploaded": sent_parts > 0, "parts": plan.parts, "part_size": plan.part_size,
            "sent_parts": sent_parts
        }))
    }

    /// The signed headers that pin type, hash and caching of a new object.
    fn object_headers(asset: &AssetRef) -> Vec<(&'static str, String)> {
        let mut headers = vec![
            ("content-type", asset.media_type.clone()),
            ("x-amz-meta-sha256", asset.sha256.clone()),
        ];
        if asset.visibility == Visibility::Public {
            headers.push(("cache-control", IMMUTABLE.into()));
        }
        headers
    }

    fn progress(&self, asset: &AssetRef, sent: u64, done: usize, parts: u16) {
        (self.transfer.events)(json!({
            "event": "progress", "sent": sent, "total": asset.bytes, "parts_done": done,
            "parts": parts
        }));
    }

    async fn put_single(
        &self,
        file: &Path,
        bucket: &Bucket,
        key: &str,
        asset: &AssetRef,
    ) -> Result<(), AssetError> {
        let body = Bytes::from(
            tokio::fs::read(file)
                .await
                .map_err(|e| io_error("Cannot read", file, e))?,
        );
        if hex(&Sha256::digest(&body)) != asset.sha256 {
            return Err(AssetError::mismatch(format!(
                "{} changed after it was hashed",
                file.display()
            )));
        }
        self.progress(asset, 0, 0, 1);
        let signed = Self::object_headers(asset);
        let md5 = content_md5(&body);
        let mut headers: Vec<(&str, &str)> = signed.iter().map(|(n, v)| (*n, v.as_str())).collect();
        headers.push(("content-md5", &md5));
        let (signed, headers, body) = (&signed, &headers, &body);
        let timeout = self.transfer.policy.timeout_for(asset.bytes);
        self.transfer
            .retrying(json!({"request": "put"}), move |attempt| {
                let mut action = bucket.put_object(self.credentials(), key);
                for (name, value) in signed {
                    action.headers_mut().insert(*name, value.clone());
                }
                let url = action.sign(SIGNATURE_TTL);
                let body = body.clone();
                async move {
                    self.transfer
                        .request(
                            Method::PUT,
                            url,
                            headers,
                            Some(Payload {
                                bytes: body,
                                retry: attempt > 1,
                            }),
                            timeout,
                        )
                        .await
                        .map(drop)
                }
            })
            .await
            .map_err(|f| self.fail(f, "PUT"))?;
        self.progress(asset, asset.bytes, 1, 1);
        Ok(())
    }

    async fn put_multipart(
        &self,
        file: &Path,
        bucket: &Bucket,
        key: &str,
        asset: &AssetRef,
        stamp: Stamp,
        journal_path: &Path,
    ) -> Result<usize, AssetError> {
        let plan = Plan::new(asset.bytes);
        let resumed = match Journal::load(journal_path)? {
            Some(journal)
                if journal.stamp == stamp
                    && journal.part_size == plan.part_size
                    && journal.key == key
                    && journal.bucket == bucket.name() =>
            {
                match self.list_parts(bucket, key, &journal.upload_id).await {
                    Ok(listed) => Some(Journal {
                        // The store's part list is the truth; parts of another size are resent.
                        parts: listed
                            .into_iter()
                            .filter(|p| {
                                (1..=plan.parts).contains(&p.n) && p.size == plan.range(p.n).1
                            })
                            .collect(),
                        ..journal
                    }),
                    Err(Failure::Status { status: 404, .. }) => {
                        (self.transfer.events)(
                            json!({"event": "restart", "reason": "the store no longer has the upload"}),
                        );
                        None
                    }
                    Err(failure) => return Err(self.fail(failure, "ListParts")),
                }
            }
            Some(stale) => {
                (self.transfer.events)(
                    json!({"event": "restart", "reason": "the file or part size changed since the journal was written"}),
                );
                self.abort_upload(bucket, &stale.key, &stale.upload_id)
                    .await?;
                None
            }
            None => None,
        };
        let mut journal = match resumed {
            Some(journal) => journal,
            None => Journal {
                store: self.store.name.clone(),
                bucket: bucket.name().to_owned(),
                key: key.to_owned(),
                upload_id: self.create_multipart(bucket, key, asset).await?,
                file: std::path::absolute(file).unwrap_or_else(|_| file.to_owned()),
                sha256: asset.sha256.clone(),
                media_type: asset.media_type.clone(),
                visibility: asset.visibility,
                stamp,
                part_size: plan.part_size,
                parts: Vec::new(),
            },
        };
        journal.save(journal_path)?;
        let done: BTreeSet<u16> = journal.parts.iter().map(|p| p.n).collect();
        let mut sent: u64 = journal.parts.iter().map(|p| p.size).sum();
        self.progress(asset, sent, done.len(), plan.parts);
        let missing: Vec<u16> = (1..=plan.parts).filter(|n| !done.contains(n)).collect();
        let sent_parts = missing.len();
        let upload_id = journal.upload_id.clone();
        let mut uploads = futures_util::stream::iter(missing)
            .map(|n| self.upload_part(file, bucket, key, &upload_id, plan, n))
            .buffer_unordered(self.transfer.policy.parallel.max(1));
        while let Some(part) = uploads.next().await {
            let part = part?;
            sent += part.size;
            journal.parts.push(part);
            journal.save(journal_path)?;
            self.progress(asset, sent, journal.parts.len(), plan.parts);
        }
        drop(uploads);
        let now = std::fs::metadata(file)
            .map(|m| self::stamp(&m))
            .map_err(|e| io_error("Cannot stat", file, e))?;
        if now != stamp {
            self.abort_upload(bucket, key, &upload_id).await?;
            let _ = std::fs::remove_file(journal_path);
            return Err(AssetError::mismatch(format!(
                "{} changed during the upload; the upload was aborted, rerun to upload the new content",
                file.display()
            )));
        }
        journal.parts.sort_by_key(|p| p.n);
        // The completion request numbers ETags by position.
        if !journal.parts.iter().map(|p| p.n).eq(1..=plan.parts) {
            return Err(AssetError::transfer(format!(
                "The upload of {key} is missing parts; rerun to resume it"
            )));
        }
        self.complete(bucket, key, &upload_id, &journal.parts)
            .await?;
        let _ = std::fs::remove_file(journal_path);
        Ok(sent_parts)
    }

    async fn create_multipart(
        &self,
        bucket: &Bucket,
        key: &str,
        asset: &AssetRef,
    ) -> Result<String, AssetError> {
        let signed = Self::object_headers(asset);
        let headers: Vec<(&str, &str)> = signed.iter().map(|(n, v)| (*n, v.as_str())).collect();
        let (signed, headers) = (&signed, &headers);
        let body = self
            .transfer
            .retrying(json!({"request": "create"}), move |_| {
                let mut action = bucket.create_multipart_upload(self.credentials(), key);
                for (name, value) in signed {
                    action.headers_mut().insert(*name, value.clone());
                }
                let url = action.sign(SIGNATURE_TTL);
                async move {
                    let response = self
                        .transfer
                        .request(
                            Method::POST,
                            url,
                            headers,
                            None,
                            self.transfer.policy.timeout,
                        )
                        .await?;
                    read_limited(response).await
                }
            })
            .await
            .map_err(|f| self.fail(f, "CreateMultipartUpload"))?;
        CreateMultipartUpload::parse_response(&body)
            .map(|response| response.upload_id().to_owned())
            .map_err(|_| {
                AssetError::transfer("The store's CreateMultipartUpload reply has no upload id")
            })
    }

    async fn upload_part(
        &self,
        file: &Path,
        bucket: &Bucket,
        key: &str,
        upload_id: &str,
        plan: Plan,
        n: u16,
    ) -> Result<Part, AssetError> {
        let (offset, size) = plan.range(n);
        let body = read_range(file, offset, size).await?;
        let md5 = content_md5(&body);
        let (body, md5) = (&body, md5.as_str());
        let etag = self
            .transfer
            .retrying(json!({"part": n}), move |attempt| {
                let url = bucket
                    .upload_part(self.credentials(), key, n, upload_id)
                    .sign(SIGNATURE_TTL);
                let body = body.clone();
                async move {
                    let response = self
                        .transfer
                        .request(
                            Method::PUT,
                            url,
                            &[("content-md5", md5)],
                            Some(Payload {
                                bytes: body,
                                retry: attempt > 1,
                            }),
                            self.transfer.policy.timeout_for(size),
                        )
                        .await?;
                    response
                        .headers()
                        .get(header::ETAG)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            Failure::Fatal(AssetError::transfer(format!(
                                "part {n}: the store returned no ETag"
                            )))
                        })
                }
            })
            .await
            .map_err(|f| self.fail(f, &format!("part {n}")))?;
        Ok(Part { n, etag, size })
    }

    async fn list_parts(
        &self,
        bucket: &Bucket,
        key: &str,
        upload_id: &str,
    ) -> Result<Vec<Part>, Failure> {
        let mut parts = Vec::new();
        let mut marker = None;
        loop {
            let body = self
                .transfer
                .retrying(json!({"request": "list-parts"}), move |_| {
                    let mut action = bucket.list_parts(self.credentials(), key, upload_id);
                    if let Some(marker) = marker {
                        action.set_part_number_marker(marker);
                    }
                    let url = action.sign(SIGNATURE_TTL);
                    async move {
                        let response = self
                            .transfer
                            .request(Method::GET, url, &[], None, self.transfer.policy.timeout)
                            .await?;
                        read_limited(response).await
                    }
                })
                .await?;
            let page = ListParts::parse_response(&body).map_err(|_| {
                Failure::Fatal(AssetError::transfer(
                    "The store's ListParts reply is unreadable",
                ))
            })?;
            parts.extend(page.parts.into_iter().map(|p| Part {
                n: p.number,
                etag: p.etag,
                size: p.size,
            }));
            match page.next_part_number_marker {
                Some(next) => marker = Some(next),
                None => return Ok(parts),
            }
        }
    }

    async fn complete(
        &self,
        bucket: &Bucket,
        key: &str,
        upload_id: &str,
        parts: &[Part],
    ) -> Result<(), AssetError> {
        self.transfer
            .retrying(json!({"request": "complete"}), move |_| {
                let action = bucket.complete_multipart_upload(
                    self.credentials(),
                    key,
                    upload_id,
                    parts.iter().map(|p| p.etag.as_str()),
                );
                let url = action.sign(SIGNATURE_TTL);
                let body = Bytes::from(action.body());
                async move {
                    match self
                        .transfer
                        .request(
                            Method::POST,
                            url,
                            &[],
                            Some(Payload {
                                bytes: body,
                                retry: false,
                            }),
                            COMPLETE_TIMEOUT,
                        )
                        .await
                    {
                        // S3 can report a failed completion inside a 200 response.
                        Ok(response) => {
                            let body = read_limited(response).await?;
                            match xml_text(&body, "Code") {
                                Some(code @ ("InternalError" | "SlowDown")) => {
                                    Err(Failure::Retry {
                                        reason: format!("200 {code}"),
                                        after: None,
                                    })
                                }
                                Some(code) => Err(Failure::Status {
                                    status: 200,
                                    code: code.to_owned(),
                                    message: unescape(
                                        xml_text(&body, "Message").unwrap_or_default(),
                                    ),
                                }),
                                None => Ok(()),
                            }
                        }
                        // An earlier attempt may have completed it; the HEAD check decides.
                        Err(Failure::Status {
                            status: 404, code, ..
                        }) if code == "NoSuchUpload" => Ok(()),
                        Err(failure) => Err(failure),
                    }
                }
            })
            .await
            .map_err(|f| self.fail(f, "CompleteMultipartUpload"))
    }

    /// Returns false when the store no longer had the upload.
    async fn abort_upload(
        &self,
        bucket: &Bucket,
        key: &str,
        upload_id: &str,
    ) -> Result<bool, AssetError> {
        self.transfer
            .retrying(json!({"request": "abort"}), move |_| {
                let url = bucket
                    .abort_multipart_upload(self.credentials(), key, upload_id)
                    .sign(SIGNATURE_TTL);
                async move {
                    match self
                        .transfer
                        .request(Method::DELETE, url, &[], None, self.transfer.policy.timeout)
                        .await
                    {
                        Ok(_) => Ok(true),
                        Err(Failure::Status { status: 404, .. }) => Ok(false),
                        Err(failure) => Err(failure),
                    }
                }
            })
            .await
            .map_err(|f| self.fail(f, "AbortMultipartUpload"))
    }
}

/// Checks each reference with HEAD (status, size, type, range support and stored hash metadata)
/// and, with `hash`, streams the object to compare its SHA-256. Private references are read with
/// the store's credentials.
pub(super) async fn verify(
    references: &[(String, Value)],
    store: Option<&Store>,
    hash: bool,
    policy: Policy,
    events: Events<'_>,
) -> Result<(Value, i32), AssetError> {
    let mut checked = Vec::new();
    let mut exit = 0;
    for (label, value) in references {
        let usage = value
            .get("type")
            .and_then(Value::as_str)
            .and_then(AssetUse::of_type)
            .unwrap_or(AssetUse::Data);
        let mut diagnostics = Vec::new();
        let Some(asset) = AssetRef::parse(value, usage, label, &mut diagnostics) else {
            checked.push(json!({"reference": label, "ok": false, "diagnostics": diagnostics}));
            exit = 2;
            continue;
        };
        let url = Url::parse(&asset.url).map_err(|e| AssetError::invalid(e.to_string()))?;
        let mut urls = vec![&url];
        if let Some(store) = store {
            urls.push(store.private.base_url());
        }
        let transfer = Transfer {
            client: http_client(&urls)?,
            policy,
            events,
            secrets: store.map(Store::secrets).unwrap_or_default(),
        };
        let sign = match asset.visibility {
            Visibility::Public => None,
            Visibility::Private => {
                let store = store.ok_or_else(|| {
                    AssetError::invalid(format!(
                        "{label} is private: pass --to <store> so the request can be signed"
                    ))
                })?;
                let key = store.private_key(&asset.url).ok_or_else(|| {
                    AssetError::invalid(format!(
                        "{label} is not an object of store '{}' ({})",
                        store.name,
                        store.private.base_url()
                    ))
                })?;
                Some((store, key))
            }
        };
        let signed = |method: Method| match &sign {
            None => url.clone(),
            Some((store, key)) if method == Method::GET => store
                .private
                .get_object(Some(&store.credentials), key)
                .sign(SIGNATURE_TTL),
            Some((store, key)) => store
                .private
                .head_object(Some(&store.credentials), key)
                .sign(SIGNATURE_TTL),
        };
        let what = format!("verify {label}");
        let head = transfer
            .head(|| signed(Method::HEAD))
            .await
            .map_err(|f| transfer_error(&transfer, f, &what))?;
        let mut problems = match &head {
            Some(head) => head.problems(&asset, false),
            None => vec!["the object does not exist (404)".into()],
        };
        let mut sha256 = Value::Null;
        if hash && head.is_some() {
            let (digest, length) = transfer
                .hash(|| signed(Method::GET), asset.bytes)
                .await
                .map_err(|f| transfer_error(&transfer, f, &what))?;
            if length != asset.bytes {
                problems.push(format!(
                    "the content has {}{length} bytes, not {}",
                    if length > asset.bytes {
                        "more than "
                    } else {
                        ""
                    },
                    asset.bytes
                ));
            } else if digest != asset.sha256 {
                problems.push(format!(
                    "the content's SHA-256 is {digest}, not {}",
                    asset.sha256
                ));
            }
            sha256 = json!(if problems.is_empty() {
                "match"
            } else {
                "mismatch"
            });
        }
        if !problems.is_empty() && exit == 0 {
            exit = 4;
        }
        checked.push(json!({
            "reference": label, "url": asset.url, "visibility": asset.visibility,
            "ok": problems.is_empty(), "head": head, "sha256": sha256, "problems": problems
        }));
    }
    let ok = exit == 0;
    Ok((json!({"ok": ok, "checked": checked}), exit))
}

fn transfer_error(transfer: &Transfer<'_>, failure: Failure, what: &str) -> AssetError {
    let mut error = failure.into_error(what);
    error.message = transfer.redact(&error.message);
    error
}

/// Aborts journaled multipart uploads, optionally only those of one store or of hashes that
/// start with the given prefixes, and removes their journals. `load_store` resolves the store a
/// journal names.
pub(super) async fn abort(
    journal_dir: &Path,
    only_store: Option<&str>,
    hashes: &[String],
    load_store: impl Fn(&str) -> Result<Store, AssetError>,
    events: Events<'_>,
) -> Result<Value, AssetError> {
    if let Some(hash) = hashes
        .iter()
        .find(|h| h.len() < 8 || !h.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(AssetError::invalid(format!(
            "'{hash}' is not a SHA-256 or a prefix of at least 8 hex characters"
        )));
    }
    let entries = match std::fs::read_dir(journal_dir) {
        Ok(entries) => entries.filter_map(Result::ok).map(|e| e.path()).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(io_error("Cannot list", journal_dir, e)),
    };
    let mut paths: Vec<PathBuf> = entries
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    let mut loaded: BTreeMap<String, Store> = BTreeMap::new();
    let mut aborted = Vec::new();
    for path in paths {
        let Some(journal) = Journal::load(&path)? else {
            continue;
        };
        if only_store.is_some_and(|s| s != journal.store)
            || (!hashes.is_empty()
                && !hashes
                    .iter()
                    .any(|h| journal.sha256.starts_with(&h.to_ascii_lowercase())))
        {
            continue;
        }
        if !loaded.contains_key(&journal.store) {
            loaded.insert(journal.store.clone(), load_store(&journal.store)?);
        }
        let store = &loaded[&journal.store];
        let bucket = store.bucket_named(&journal.bucket).ok_or_else(|| {
            AssetError::invalid(format!(
                "Store '{}' no longer names bucket '{}' of journal {}",
                store.name,
                journal.bucket,
                path.display()
            ))
        })?;
        let uploader = Uploader::new(store, journal_dir.to_owned(), Policy::default(), events)?;
        let existed = uploader
            .abort_upload(bucket, &journal.key, &journal.upload_id)
            .await?;
        std::fs::remove_file(&path).map_err(|e| io_error("Cannot remove", &path, e))?;
        aborted.push(json!({
            "store": journal.store, "bucket": journal.bucket, "key": journal.key,
            "sha256": journal.sha256, "parts_done": journal.parts.len(),
            "status": if existed { "aborted" } else { "already gone" }
        }));
    }
    Ok(json!({"ok": true, "aborted": aborted}))
}

#[cfg(test)]
mod tests {
    use super::super::{progress_of, store::StoreProfile};
    use super::*;
    use axum::{
        Router,
        extract::{DefaultBodyLimit, State},
        http::{HeaderMap, StatusCode, Uri},
        response::{IntoResponse, Response},
    };
    use std::sync::{Arc, Mutex};

    const KEY: &str = "GKa1b2c3d4e5f6a7b8c9d0e1f2";
    const SECRET: &str = "5f1c0e9d8b7a6f5e4d3c2b1a0f9e8d7c";
    const NS: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

    struct Upload {
        key: String,
        headers: BTreeMap<String, String>,
        parts: BTreeMap<u16, Vec<u8>>,
    }

    /// An S3 endpoint with the requests RichView sends, a request log and injectable failures.
    #[derive(Default)]
    struct Mock {
        objects: BTreeMap<String, (BTreeMap<String, String>, Vec<u8>)>,
        uploads: BTreeMap<String, Upload>,
        created: usize,
        log: Vec<String>,
        /// The next request with this label fails with this status.
        faults: Vec<(String, u16)>,
    }
    type Shared = Arc<Mutex<Mock>>;

    fn error(status: StatusCode, code: &str) -> Response {
        (
            status,
            format!("<Error><Code>{code}</Code><Message>{code}</Message></Error>"),
        )
            .into_response()
    }

    async fn s3(
        State(mock): State<Shared>,
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        let url = Url::parse(&format!("http://mock{uri}")).unwrap();
        let query: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
        let path = url.path().to_owned();
        let id = query.get("uploadId").cloned().unwrap_or_default();
        let label = match (method.as_str(), query.get("partNumber")) {
            ("PUT", Some(n)) => format!("PART {n}"),
            ("POST", _) if query.contains_key("uploads") => "CREATE".into(),
            ("POST", _) => "COMPLETE".into(),
            ("GET", _) if !id.is_empty() => "LIST".into(),
            ("DELETE", _) => "ABORT".into(),
            (method, _) => method.into(),
        };
        let mut mock = mock.lock().unwrap();
        if let Some(i) = mock.faults.iter().position(|(l, _)| *l == label) {
            let (_, status) = mock.faults.remove(i);
            mock.log.push(format!("{label} {status}"));
            return error(StatusCode::from_u16(status).unwrap(), "SlowDown");
        }
        mock.log.push(label.clone());
        let md5_ok = headers.get("content-md5").and_then(|v| v.to_str().ok())
            == Some(content_md5(&body).as_str());
        let mut stored: BTreeMap<String, String> =
            ["content-type", "x-amz-meta-sha256", "cache-control"]
                .into_iter()
                .filter_map(|n| Some((n.into(), headers.get(n)?.to_str().ok()?.into())))
                .collect();
        stored.insert(
            "signed".into(),
            query
                .get("X-Amz-SignedHeaders")
                .cloned()
                .unwrap_or_default(),
        );
        match label.as_str() {
            "HEAD" | "GET" => {
                let Some((headers, data)) = mock.objects.get(&path) else {
                    return error(StatusCode::NOT_FOUND, "NoSuchKey");
                };
                let mut response = Response::builder()
                    .header("content-length", data.len())
                    .header("accept-ranges", "bytes");
                for (name, value) in headers.iter().filter(|(n, _)| *n != "signed") {
                    response = response.header(name, value);
                }
                let body = if label == "GET" {
                    data.clone()
                } else {
                    Vec::new()
                };
                response.body(axum::body::Body::from(body)).unwrap()
            }
            "PUT" if !md5_ok => error(StatusCode::BAD_REQUEST, "BadDigest"),
            "PUT" => {
                mock.objects.insert(path, (stored, body.to_vec()));
                StatusCode::OK.into_response()
            }
            "CREATE" => {
                mock.created += 1;
                let id = format!("upload-{}", mock.created);
                let key = path;
                let upload = Upload {
                    key,
                    headers: stored,
                    parts: BTreeMap::new(),
                };
                mock.uploads.insert(id.clone(), upload);
                format!("<InitiateMultipartUploadResult xmlns=\"{NS}\"><UploadId>{id}</UploadId></InitiateMultipartUploadResult>").into_response()
            }
            _ if !mock.uploads.contains_key(&id) => error(StatusCode::NOT_FOUND, "NoSuchUpload"),
            "LIST" => {
                let parts: String = mock.uploads[&id].parts.iter().map(|(n, data)| format!("<Part><PartNumber>{n}</PartNumber><LastModified>2026-10-11T00:00:00.000Z</LastModified><ETag>\"etag-{n}\"</ETag><Size>{}</Size></Part>", data.len())).collect();
                format!("<ListPartsResult xmlns=\"{NS}\"><MaxParts>1000</MaxParts><IsTruncated>false</IsTruncated>{parts}</ListPartsResult>").into_response()
            }
            "COMPLETE" => {
                let upload = mock.uploads.remove(&id).unwrap();
                let body = String::from_utf8_lossy(&body);
                assert!((1..=upload.parts.len()).all(|n| body.contains(&format!(
                    "<ETag>&quot;etag-{n}&quot;</ETag><PartNumber>{n}</PartNumber>"
                ))));
                let data = upload.parts.into_values().flatten().collect();
                mock.objects.insert(upload.key, (upload.headers, data));
                format!(
                    "<CompleteMultipartUploadResult xmlns=\"{NS}\"></CompleteMultipartUploadResult>"
                )
                .into_response()
            }
            "ABORT" => {
                mock.uploads.remove(&id);
                StatusCode::NO_CONTENT.into_response()
            }
            _ if !md5_ok => error(StatusCode::BAD_REQUEST, "BadDigest"),
            _ => {
                let n = query["partNumber"].parse().unwrap();
                mock.uploads
                    .get_mut(&id)
                    .unwrap()
                    .parts
                    .insert(n, body.to_vec());
                ([(header::ETAG, format!("\"etag-{n}\""))], "").into_response()
            }
        }
    }

    async fn serve_mock() -> (Shared, String) {
        let mock = Shared::default();
        let app = Router::new()
            .fallback(s3)
            .layer(DefaultBodyLimit::disable())
            .with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (mock, endpoint)
    }

    fn profile(value: Value) -> StoreProfile {
        serde_json::from_value(value).unwrap()
    }

    fn store(endpoint: &str) -> Store {
        let profile = profile(json!({
            "endpoint": endpoint, "region": "garage", "bucket": "assets",
            "public_base_url": format!("{endpoint}/assets")
        }));
        Store::new("local", profile, Credentials::new(KEY, SECRET)).unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("richview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn plans_uniform_whole_mebibyte_parts_below_the_part_limit() {
        assert_eq!(Plan::new(SINGLE_PUT_MAX).parts, 1);
        let video = Plan::new(20_037_160);
        assert_eq!((video.part_size, video.parts), (8 * MIB, 3));
        assert_eq!(
            (1..=3).map(|n| video.range(n)).collect::<Vec<_>>(),
            [(0, 8 * MIB), (8 * MIB, 8 * MIB), (16 * MIB, 3_259_944)]
        );
        for bytes in [SINGLE_PUT_MAX + 1, 5 << 30, 100 << 30, 50_000_000_000_000] {
            let plan = Plan::new(bytes);
            assert!(u64::from(plan.parts) <= MAX_PARTS, "{bytes}");
            assert!(plan.part_size >= MIN_PART && plan.part_size.is_multiple_of(MIB));
            assert_eq!(plan.range(plan.parts).0 + plan.range(plan.parts).1, bytes);
        }
    }

    #[test]
    fn keys_are_content_addressed_and_readable() {
        let sha = "0c6eb955d7b052f79ab511fb0ab43105ed10c31dcf3ff8169b23133583d4aaf4";
        let name = object_name(Path::new("media/My Clip (final).MP4"));
        assert_eq!(
            content_key("public/", AssetUse::Video, sha, &name),
            "public/video/0c6eb955d7b052f7/My-Clip-final.MP4"
        );
        assert_eq!(object_name(Path::new("../../.hidden")), "hidden");
        assert_eq!(object_name(Path::new("Výkon kůň.csv")), "V-kon-k.csv");
        assert_eq!(object_name(Path::new("———")), "asset");
    }

    #[tokio::test]
    async fn uploads_resume_from_listed_parts_retry_verify_and_abort() {
        let (mock, endpoint) = serve_mock().await;
        let store = store(&endpoint);
        let dir = scratch("asset-upload");
        let journals = dir.join("journal");
        let events = Mutex::new(Vec::new());
        let record = |event: Value| events.lock().unwrap().push(event);
        let policy = Policy {
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(4),
            parallel: 1,
            body_hold: Duration::from_millis(1),
            ..Policy::default()
        };
        let uploader = Uploader::new(&store, journals.clone(), policy, &record).unwrap();
        let log = || std::mem::take(&mut mock.lock().unwrap().log);

        // A small dataset goes up in one PUT whose signature covers its type and hash. A 403
        // answer to the first HEAD (no permission to list the bucket) means "not found".
        let csv = dir.join("readings.csv");
        std::fs::write(&csv, "ts,value\n2026-10-01,20.5\n").unwrap();
        mock.lock().unwrap().faults = vec![("HEAD".into(), 403)];
        let put = uploader.put(&csv, Visibility::Private, None).await.unwrap();
        let dataset: AssetRef = serde_json::from_value(put["asset"].clone()).unwrap();
        assert!(dataset.validate(AssetUse::Data).is_empty());
        let key = format!(
            "/assets/private/data/{}/readings.csv",
            &dataset.sha256[..16]
        );
        assert_eq!(dataset.url, format!("{endpoint}{key}"));
        assert_eq!(log(), ["HEAD 403", "PUT", "HEAD"]);
        let stored = mock.lock().unwrap().objects[&key].0.clone();
        assert_eq!(stored["content-type"], "text/csv");
        assert_eq!(stored["signed"], "content-type;host;x-amz-meta-sha256");
        assert!(!stored.contains_key("cache-control"));
        let again = uploader.put(&csv, Visibility::Private, None).await.unwrap();
        assert_eq!(again["uploaded"], false);
        assert_eq!(log(), ["HEAD"]);
        // A declared type cannot relabel a file; files need an allowed extension.
        let notes = dir.join("notes");
        std::fs::write(&notes, "ts,value\n").unwrap();
        for (file, declared) in [
            (&csv, "image/png"),
            (&csv, "application/json"),
            (&notes, "text/csv"),
        ] {
            let refused = uploader
                .put(file, Visibility::Private, Some(declared))
                .await;
            assert_eq!(refused.unwrap_err().exit, 2, "{declared}");
        }
        assert!(log().is_empty());

        // Part 2 keeps failing: the run stops with part 1 journaled.
        let video = dir.join("clip.mp4");
        let bytes: Vec<u8> = (0..17 * MIB).map(|i| (i % 251) as u8).collect();
        std::fs::write(&video, &bytes).unwrap();
        mock.lock().unwrap().faults = vec![("PART 2".into(), 500); 5];
        let failed = uploader
            .put(&video, Visibility::Public, None)
            .await
            .unwrap_err();
        assert_eq!(failed.exit, 3);
        assert_eq!(log()[..3], ["HEAD", "CREATE", "PART 1"]);
        let journal_path = std::fs::read_dir(&journals)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let journal = Journal::load(&journal_path).unwrap().unwrap();
        assert_eq!(journal.parts.iter().map(|p| p.n).collect::<Vec<_>>(), [1]);

        // The rerun asks the store which parts it has, sends only the others and retries a 503.
        mock.lock().unwrap().faults = vec![("PART 3".into(), 503)];
        events.lock().unwrap().clear();
        let resumed = uploader
            .put(&video, Visibility::Public, None)
            .await
            .unwrap();
        assert_eq!(resumed["sent_parts"], 2);
        assert_eq!(
            log(),
            [
                "HEAD",
                "LIST",
                "PART 2",
                "PART 3 503",
                "PART 3",
                "COMPLETE",
                "HEAD"
            ]
        );
        assert!(!journal_path.exists());
        let key = format!(
            "/assets/public/video/{}/clip.mp4",
            &resumed["asset"]["sha256"].as_str().unwrap()[..16]
        );
        let (stored, data) = mock.lock().unwrap().objects[&key].clone();
        assert!(data == bytes);
        assert_eq!(stored["cache-control"], IMMUTABLE);
        assert_eq!(
            stored["signed"],
            "cache-control;content-type;host;x-amz-meta-sha256"
        );
        let seen = std::mem::take(&mut *events.lock().unwrap());
        let sent: Vec<u64> = seen
            .iter()
            .filter_map(|e| progress_of(e).map(|p| p.0))
            .collect();
        assert_eq!(sent, [8 * MIB, 16 * MIB, 17 * MIB]);
        assert!(seen.iter().any(|e| e["event"] == "retry" && e["part"] == 3));
        let text = format!("{seen:?}{failed:?}");
        assert!(!text.contains(KEY) && !text.contains(SECRET));

        // Verification reads the public object anonymously and the private one through the store.
        let video_ref = resumed["asset"].clone();
        let references = [
            ("video".to_owned(), video_ref.clone()),
            ("csv".to_owned(), put["asset"].clone()),
        ];
        let (report, exit) = verify(&references, Some(&store), true, policy, &|_: Value| {})
            .await
            .unwrap();
        assert_eq!((exit, &report["ok"]), (0, &json!(true)), "{report}");
        let mut tampered = video_ref.clone();
        tampered["sha256"] = json!("f".repeat(64));
        let (report, exit) = verify(
            &[("video".into(), tampered)],
            None,
            true,
            policy,
            &|_: Value| {},
        )
        .await
        .unwrap();
        assert_eq!(exit, 4);
        assert_eq!(report["checked"][0]["sha256"], "mismatch");
        let mut retyped = video_ref;
        retyped["type"] = json!("video/webm");
        assert_eq!(
            verify(
                &[("video".into(), retyped)],
                None,
                false,
                policy,
                &|_: Value| {}
            )
            .await
            .unwrap()
            .1,
            4
        );

        // Abort removes interrupted uploads from the store and their journals.
        let other = dir.join("other.mp4");
        let mut other_bytes = bytes.clone();
        other_bytes[0] ^= 1;
        std::fs::write(&other, other_bytes).unwrap();
        mock.lock().unwrap().faults = vec![("PART 2".into(), 500); 5];
        assert!(
            uploader
                .put(&other, Visibility::Private, None)
                .await
                .is_err()
        );
        assert_eq!(mock.lock().unwrap().uploads.len(), 1);
        let aborted = abort(&journals, None, &[], |_| Ok(store.clone()), &|_: Value| {})
            .await
            .unwrap();
        assert_eq!(aborted["aborted"][0]["status"], "aborted");
        assert!(mock.lock().unwrap().uploads.is_empty());
        assert_eq!(std::fs::read_dir(&journals).unwrap().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
