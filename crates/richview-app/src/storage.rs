use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

/// A validated hosted report address, with a permanent organization and a separate slug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportKey(String);

impl ReportKey {
    pub fn hosted(org: &str, slug: &str) -> Result<Self, String> {
        if org.len() != 8
            || !org
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err("Invalid organization code".into());
        }
        validate_slug(slug)?;
        Ok(Self(format!("{org}/{slug}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ReportKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Why a publication was not stored (TYP-63, TYP-64).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    /// The `{slug, version}` exists with different content; the author bumps the version.
    Conflict {
        stored_sha256: String,
    },
    /// Storing the artifact would exceed the deployment's byte quota; nothing was stored.
    Quota {
        limit: u64,
        used: u64,
        needed: u64,
    },
    /// Storing the artifact would exceed its owner's byte quota.
    AccountQuota {
        limit: u64,
        used: u64,
        needed: u64,
    },
    OrganizationQuota {
        limit: u64,
        used: u64,
        needed: u64,
    },
    ReportLimit {
        limit: u64,
    },
    /// The version was deleted and its number can never be published again.
    Deleted,
    /// The whole report was deleted, so its address is permanently retired.
    ReportDeleted,
    Other(String),
}

impl From<String> for PublishError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict { stored_sha256 } => write!(
                f,
                "This version is already published with different content (stored hash {stored_sha256}). Bump the version in the frontmatter and publish again."
            ),
            Self::Quota {
                limit,
                used,
                needed,
            } => write!(
                f,
                "Storing this report needs {needed} bytes but the server has {} of its {limit}-byte quota left. Nothing was stored; ask the server owner to raise the quota or delete old versions.",
                limit.saturating_sub(*used)
            ),
            Self::AccountQuota {
                limit,
                used,
                needed,
            } => write!(
                f,
                "Storing this report needs {needed} bytes but the account has {} of its {limit}-byte quota left. Nothing was stored; delete old versions or ask the server owner for help.",
                limit.saturating_sub(*used)
            ),
            Self::OrganizationQuota {
                limit,
                used,
                needed,
            } => write!(
                f,
                "This organization has {} of its {limit}-byte limit left; this publication needs {needed} bytes. Nothing was stored.",
                limit.saturating_sub(*used)
            ),
            Self::ReportLimit { limit } => write!(
                f,
                "This organization already has its {limit} live reports. Delete a report before creating another."
            ),
            Self::Deleted => write!(
                f,
                "This version was deleted and its number cannot be published again. Publish under a new version."
            ),
            Self::ReportDeleted => write!(
                f,
                "This report was deleted and its address is retired. Publish under a different address."
            ),
            Self::Other(message) => f.write_str(message),
        }
    }
}

/// The identity of a publication (D-TYP-CONTENT-IDENTITY): the SHA-256 of the `.rmd` source, the
/// ordered styles and every file sorted by report-relative path, each with its byte length. Slug,
/// version, rendered HTML and engine version are not part of it.
pub fn content_sha256(source: &str, styles: &[String], files: &BTreeMap<String, String>) -> String {
    fn feed(hash: &mut Sha256, bytes: &[u8]) {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    let mut hash = Sha256::new();
    feed(&mut hash, source.as_bytes());
    hash.update((styles.len() as u64).to_be_bytes());
    for style in styles {
        feed(&mut hash, style.as_bytes());
    }
    hash.update((files.len() as u64).to_be_bytes());
    for (path, bytes) in files {
        feed(&mut hash, path.as_bytes());
        feed(&mut hash, bytes.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredReport {
    pub slug: String,
    pub version: String,
    pub source: String,
    #[serde(default)]
    pub report_folder: Option<richview_core::document::ReportFolder>,
    pub html: String,
    pub frontmatter: Value,
    /// When this immutable version was first stored.
    pub created_at: String,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub data: BTreeMap<String, String>,
    /// Files the source links to, base64 by report-relative path; served read-only at
    /// `/r/{slug}/v/{version}/files/{path}` (FEA-SF-52).
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub content_sha256: String,
}

/// What a publication's identity covers: its data and styles files, and its linked files kept
/// apart from them. Reports without linked files hash as before.
fn identity_files(
    data: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut all = data.clone();
    for (path, bytes) in files {
        all.insert(format!("\u{1}link:{path}"), bytes.clone());
    }
    all
}

#[derive(Clone)]
pub struct FsStorage {
    dir: Arc<PathBuf>,
    lock: Arc<Mutex<()>>,
    quota: Option<u64>,
    account_quota: Option<u64>,
    hosted: bool,
    org_quota: u64,
    org_reports: u64,
    signup_space: Arc<Mutex<Option<(std::time::Instant, bool)>>>,
}

/// Hosted organization usage measured with the same limits as publication.
#[derive(Debug, PartialEq, Eq)]
pub struct HostedUsage {
    pub bytes: u64,
    pub reports: usize,
    pub byte_limit: u64,
    pub report_limit: u64,
}

impl FsStorage {
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self, String> {
        let dir = dir.into();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dir = fs::canonicalize(dir).map_err(|e| e.to_string())?;
        Ok(Self {
            dir: Arc::new(dir),
            lock: Arc::new(Mutex::new(())),
            quota: None,
            account_quota: None,
            hosted: false,
            org_quota: 50 * 1024 * 1024,
            org_reports: 50,
            signup_space: Arc::default(),
        })
    }

    /// Bound the bytes the store may hold; a publication that would exceed it is refused.
    pub fn with_quota(mut self, quota: Option<u64>) -> Self {
        self.quota = quota;
        self.signup_space = Arc::default();
        self
    }

    pub fn with_hosted(mut self, hosted: bool) -> Self {
        self.hosted = hosted;
        self
    }

    pub fn with_hosted_limits(mut self, bytes: u64, reports: u64) -> Self {
        self.org_quota = bytes;
        self.org_reports = reports;
        self
    }

    pub fn is_hosted(&self) -> bool {
        self.hosted
    }

    pub(crate) fn hosted_usage(&self, org: &str) -> Result<HostedUsage, String> {
        // The same lock protects report writes, so the count and bytes describe one state.
        let _storage = self.lock.lock().map_err(|_| "Storage lock poisoned")?;
        let (bytes, reports) = self.operator_usage(org)?;
        Ok(HostedUsage {
            bytes,
            reports,
            byte_limit: self.org_quota,
            report_limit: self.org_reports,
        })
    }

    /// Return hosted usage without exposing report metadata or content.
    pub(crate) fn operator_usage(&self, org: &str) -> Result<(u64, usize), String> {
        if !self.hosted {
            return Err("Usage requires hosted storage".into());
        }
        ReportKey::hosted(org, "usage")?;
        let orgs = self.dir.join("orgs");
        reject_symlink(&orgs)?;
        let dir = orgs.join(org);
        reject_symlink(&dir)?;
        let bytes = if dir.exists() { dir_bytes(&dir)? } else { 0 };
        let mut reports = 0;
        if dir.exists() {
            for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let slug = entry.file_name().to_string_lossy().to_string();
                if entry.file_type().map_err(|e| e.to_string())?.is_dir()
                    && validate_slug(&slug).is_ok()
                    && !self.list_versions(&format!("{org}/{slug}"))?.is_empty()
                {
                    reports += 1;
                }
            }
        }
        Ok((bytes, reports))
    }

    pub(crate) fn operator_storage_half_full(&self, used: u64) -> bool {
        self.quota
            .is_some_and(|limit| limit > 0 && (used as u128) * 2 >= limit as u128)
    }

    pub(crate) fn operator_total_bytes(&self) -> Result<u64, String> {
        dir_bytes(&self.dir)
    }

    /// Pause new accounts when the configured store reaches 80%; cache brief reads and invalidate
    /// on report writes so anonymous requests cannot trigger an unbounded directory scan.
    pub(crate) fn signup_space_available(&self) -> Result<bool, String> {
        if !self.hosted {
            return Ok(false);
        }
        let Some(quota) = self.quota.filter(|quota| *quota > 0) else {
            return Ok(false);
        };
        let _storage = self.lock.lock().map_err(|_| "Storage lock poisoned")?;
        let mut cache = self
            .signup_space
            .lock()
            .map_err(|_| "Signup capacity lock poisoned")?;
        if let Some((at, available)) = *cache
            && at.elapsed() < std::time::Duration::from_secs(10)
        {
            return Ok(available);
        }
        let used = dir_bytes(&self.dir)?;
        let available = (used as u128) * 5 < (quota as u128) * 4;
        *cache = Some((std::time::Instant::now(), available));
        Ok(available)
    }

    /// Bound report bytes for callers that pass the owner's complete slug set. Do not expose this
    /// as a hosted quota until ownership transfer and the organization model are also enforced.
    pub fn with_account_quota(mut self, quota: Option<u64>) -> Self {
        self.account_quota = quota;
        self
    }

    /// Run `f` holding the in-process lock and the data directory's process lock.
    fn locked<T>(&self, f: impl FnOnce() -> Result<T, PublishError>) -> Result<T, PublishError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Storage lock poisoned".to_owned())?;
        reject_symlink(&self.dir.join(".publish.lock"))?;
        let process_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.dir.join(".publish.lock"))
            .map_err(|e| e.to_string())?;
        fs2::FileExt::lock_exclusive(&process_lock).map_err(|e| e.to_string())?;
        let result = f();
        if self.hosted
            && let Ok(mut cache) = self.signup_space.lock()
        {
            *cache = None;
        }
        result
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A `{slug, version}` is immutable (ARC-21). Identical content is a no-op that returns the
    /// stored artifact; different content is a conflict; a deleted version never returns.
    /// The lock serializes publications; rename commits complete artifacts.
    pub fn put_version(&self, report: StoredReport) -> Result<(StoredReport, bool), PublishError> {
        if self.account_quota.is_some() {
            return Err("An account is required when the account quota is enabled"
                .to_owned()
                .into());
        }
        self.put_version_for_owner(report, &[])
    }

    /// Store a report using its owner's complete set of report slugs for quota accounting.
    /// The server serializes this list, publication, and the ownership claim together.
    pub fn put_version_for_owner(
        &self,
        mut report: StoredReport,
        owned_slugs: &[String],
    ) -> Result<(StoredReport, bool), PublishError> {
        report.version = report.version.trim().to_string();
        self.locked(|| {
            self.validate_key(&report.slug)?;
            validate_version(&report.version)?;
            let slug_dir = self.slug_dir(&report.slug)?;
            if slug_dir.join(".deleted").exists() {
                return Err(PublishError::ReportDeleted);
            }
            report.content_sha256 = content_sha256(
                &report.source,
                &report.styles,
                &identity_files(&report.data, &report.files),
            );
            if let Some(mut existing) = self.read_version(&report.slug, &report.version)? {
                // Recomputed from the stored inputs, so versions stored under the older
                // source-only hash still compare correctly.
                existing.content_sha256 = content_sha256(
                    &existing.source,
                    &existing.styles,
                    &identity_files(&existing.data, &existing.files),
                );
                return if existing.content_sha256 == report.content_sha256 {
                    Ok((existing, false))
                } else {
                    Err(PublishError::Conflict {
                        stored_sha256: existing.content_sha256,
                    })
                };
            }
            if self.is_deleted(&report.slug, &report.version)? {
                return Err(PublishError::Deleted);
            }
            let meta = serde_json::json!({"slug": report.slug, "version": report.version, "frontmatter": report.frontmatter, "createdAt": report.created_at, "contentSha256": report.content_sha256});
            let meta = serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?;
            let assets = serde_json::to_vec(
                &serde_json::json!({"styles": report.styles, "data": report.data,"files": report.files,"reportFolder":report.report_folder}),
            )
            .map_err(|e| e.to_string())?;
            let needed = (report.source.len() + report.html.len() + meta.len() + assets.len()) as u64;
            if let Some(limit) = self.quota {
                let used = dir_bytes(&self.dir)?;
                if used.saturating_add(needed) > limit {
                    return Err(PublishError::Quota {
                        limit,
                        used,
                        needed,
                    });
                }
            }
            if let Some(limit) = self.account_quota {
                let mut used = 0u64;
                for slug in owned_slugs {
                    let path = self.slug_dir(slug)?;
                    if path.exists() {
                        used = used.saturating_add(dir_bytes(&path)?);
                    }
                }
                if used.saturating_add(needed) > limit {
                    return Err(PublishError::AccountQuota { limit, used, needed });
                }
            }
            if self.hosted {
                let org_dir = slug_dir.parent().ok_or_else(|| "Invalid report directory".to_owned())?;
                let used = if org_dir.exists() { dir_bytes(org_dir)? } else { 0 };
                if used.saturating_add(needed) > self.org_quota {
                    return Err(PublishError::OrganizationQuota { limit: self.org_quota, used, needed });
                }
                let new_live_report = self.list_versions(&report.slug)?.is_empty();
                if new_live_report && org_dir.exists() {
                    let mut live = 0u64;
                    for entry in fs::read_dir(org_dir).map_err(|e| e.to_string())? {
                        let entry = entry.map_err(|e| e.to_string())?;
                        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() { continue; }
                        let org = report.slug.split_once('/').unwrap().0;
                        let key = format!("{org}/{}", entry.file_name().to_string_lossy());
                        if self.validate_key(&key).is_ok() && !self.list_versions(&key)?.is_empty() { live += 1; }
                    }
                    if live >= self.org_reports { return Err(PublishError::ReportLimit { limit: self.org_reports }); }
                }
                if new_live_report && self.org_reports == 0 { return Err(PublishError::ReportLimit { limit: 0 }); }
            }
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let staging = slug_dir.join(format!(".staging-{}-{nonce}", std::process::id()));
            // A refused first publication must not reserve the address. Make the directory only
            // after quota checks, and remove our empty directory if any later write fails.
            if self.hosted {
                let parent = slug_dir.parent().ok_or_else(|| "Invalid report directory".to_owned())?;
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let orgs = parent.parent().ok_or_else(|| "Invalid report directory".to_owned())?;
                    fs::set_permissions(orgs, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
                    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
                }
            }
            let created_slug = match fs::create_dir(&slug_dir) {
                Ok(()) => true,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
                Err(e) => return Err(e.to_string().into()),
            };
            let mut created_staging = false;
            let result = (|| {
                #[cfg(unix)]
                if self.hosted {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&slug_dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
                }
                fs::create_dir(&staging).map_err(|e| e.to_string())?;
                created_staging = true;
                write_synced(&staging.join("source.rmd"), report.source.as_bytes())?;
                write_synced(&staging.join("rendered.html"), report.html.as_bytes())?;
                write_synced(&staging.join("meta.json"), &meta)?;
                write_synced(&staging.join("assets.json"), &assets)?;
                fs::rename(&staging, slug_dir.join(&report.version)).map_err(|e| e.to_string())?;
                Ok::<_, String>(())
            })();
            if let Err(error) = result {
                if created_staging && staging.exists() {
                    fs::remove_dir_all(&staging).map_err(|cleanup| {
                        format!("{error}; cannot remove failed staging directory: {cleanup}")
                    })?;
                }
                if created_slug {
                    fs::remove_dir(&slug_dir).map_err(|cleanup| {
                        format!("{error}; cannot remove empty report directory: {cleanup}")
                    })?;
                }
                return Err(error.into());
            }
            Ok((report, true))
        })
    }

    /// Whether a version was deleted; its permalink answers 410 and its number is retired.
    pub fn is_deleted(&self, slug: &str, version: &str) -> Result<bool, String> {
        validate_version(version)?;
        let dir = self.slug_dir(slug)?;
        if dir.join(".deleted").exists() {
            return Ok(true);
        }
        let marker = dir.join(format!("{version}.gone"));
        Ok(fs::symlink_metadata(marker).is_ok())
    }

    /// Hide the whole report first, then remove its files, preserving the retired address.
    /// Retrying after a partial removal completes cleanup without making versions visible again.
    pub fn delete_report(&self, slug: &str) -> Result<bool, String> {
        self.locked(|| {
            let dir = self.slug_dir(slug)?;
            if !dir.is_dir() {
                return Ok(false);
            }
            let marker = dir.join(".deleted");
            reject_symlink(&marker)?;
            if !marker.exists() {
                write_synced(&marker, b"deleted")?;
            }
            for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_name() == ".deleted" {
                    continue;
                }
                let kind = entry.file_type().map_err(|e| e.to_string())?;
                if kind.is_dir() {
                    fs::remove_dir_all(entry.path()).map_err(|e| e.to_string())?;
                } else {
                    fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                }
            }
            Ok(true)
        })
        .map_err(|e| e.to_string())
    }

    /// Remove every report, version and attachment in a deleted hosted organization.
    pub(crate) fn delete_hosted_organization(&self, org: &str) -> Result<(), String> {
        if !self.hosted {
            return Err("Hosted storage required".into());
        }
        ReportKey::hosted(org, "deletion")?;
        self.locked(|| {
            let parent = self.dir.join("orgs");
            reject_symlink(&parent)?;
            let dir = parent.join(org);
            reject_symlink(&dir)?;
            match fs::remove_dir_all(&dir) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string().into()),
            }
        })
        .map_err(|error| error.to_string())
    }

    /// Delete a version and leave a tombstone. `Ok(false)` when it never existed.
    pub fn delete_version(&self, slug: &str, version: &str) -> Result<bool, String> {
        self.locked(|| {
            validate_version(version)?;
            let slug_dir = self.slug_dir(slug)?;
            let path = slug_dir.join(version);
            reject_symlink(&path)?;
            if !path.exists() {
                return Ok(self.is_deleted(slug, version)?);
            }
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs();
            let marker = slug_dir.join(format!("{version}.gone"));
            reject_symlink(&marker)?;
            write_synced(&marker, stamp.to_string().as_bytes())?;
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
            if self.pinned(slug)?.as_deref() == Some(version) {
                fs::remove_file(slug_dir.join("pin.txt")).map_err(|e| e.to_string())?;
            }
            Ok(true)
        })
        .map_err(|e| e.to_string())
    }

    /// The version the owner pinned as `latest`, if any.
    pub fn pinned(&self, slug: &str) -> Result<Option<String>, String> {
        let file = self.slug_dir(slug)?.join("pin.txt");
        reject_symlink(&file)?;
        match fs::read_to_string(file) {
            Ok(v) => Ok(Some(v.trim().to_owned()).filter(|v| validate_version(v).is_ok())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Pin `latest` to an existing version. `Ok(false)` when that version does not exist.
    pub fn set_pin(&self, slug: &str, version: &str) -> Result<bool, String> {
        self.locked(|| {
            validate_version(version)?;
            let slug_dir = self.slug_dir(slug)?;
            let dir = slug_dir.join(version);
            reject_symlink(&dir)?;
            if !dir.is_dir() {
                return Ok(false);
            }
            if self.hosted {
                let org_dir = slug_dir
                    .parent()
                    .ok_or_else(|| "Invalid report directory".to_owned())?;
                if dir_bytes(org_dir)?.saturating_add(version.len() as u64) > self.org_quota
                    || self.quota.is_some_and(|limit| {
                        dir_bytes(&self.dir).map_or(true, |used| {
                            used.saturating_add(version.len() as u64) > limit
                        })
                    })
                {
                    return Err(
                        "The organization byte limit prevents changing the latest pin."
                            .to_owned()
                            .into(),
                    );
                }
            }
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let tmp = slug_dir.join(format!(".pin-{nonce}"));
            write_synced(&tmp, version.as_bytes())?;
            fs::rename(tmp, slug_dir.join("pin.txt")).map_err(|e| e.to_string())?;
            Ok(true)
        })
        .map_err(|e| e.to_string())
    }

    /// Remove the pin so `latest` follows the highest version again.
    pub fn clear_pin(&self, slug: &str) -> Result<(), String> {
        self.locked(|| {
            let file = self.slug_dir(slug)?.join("pin.txt");
            reject_symlink(&file)?;
            match fs::remove_file(file) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string().into()),
            }
        })
        .map_err(|e| e.to_string())
    }

    /// The version `/r/{slug}` serves: the owner's pin, else the highest release by SemVer
    /// precedence, else the highest pre-release (D-TYP-LATEST).
    pub fn latest_version(&self, slug: &str) -> Result<Option<String>, String> {
        if self.slug_dir(slug)?.join(".deleted").exists() {
            return Ok(None);
        }
        if let Some(pinned) = self.pinned(slug)?
            && self.slug_dir(slug)?.join(&pinned).is_dir()
        {
            return Ok(Some(pinned));
        }
        let versions = self.list_versions(slug)?;
        let is_release = |v: &&String| semver::Version::parse(v).is_ok_and(|v| v.pre.is_empty());
        Ok(versions
            .iter()
            .find(is_release)
            .or_else(|| versions.first())
            .cloned())
    }

    fn slug_dir(&self, slug: &str) -> Result<PathBuf, String> {
        self.validate_key(slug)?;
        let path = if self.hosted {
            let (org, slug) = slug.split_once('/').ok_or("Missing report organization")?;
            let orgs = self.dir.join("orgs");
            reject_symlink(&orgs)?;
            let org_dir = orgs.join(org);
            reject_symlink(&org_dir)?;
            org_dir.join(slug)
        } else {
            self.dir.join(slug)
        };
        reject_symlink(&path)?;
        Ok(path)
    }

    pub fn validate_key(&self, key: &str) -> Result<(), String> {
        if !self.hosted {
            return validate_slug(key);
        }
        let (org, slug) = key
            .split_once('/')
            .ok_or("A hosted report needs an organization")?;
        ReportKey::hosted(org, slug).map(|_| ())
    }

    pub fn get(&self, slug: &str, version: Option<&str>) -> Result<Option<StoredReport>, String> {
        let Some((version, _)) = self.report_directory(slug, version)? else {
            return Ok(None);
        };
        self.read_version(slug, &version)
    }

    fn report_directory(
        &self,
        slug: &str,
        version: Option<&str>,
    ) -> Result<Option<(String, PathBuf)>, String> {
        let slug_dir = self.slug_dir(slug)?;
        if slug_dir.join(".deleted").exists() {
            return Ok(None);
        }
        let version = match version {
            Some(v) => v.to_string(),
            None => match self.latest_version(slug)? {
                Some(v) => v,
                None => return Ok(None),
            },
        };
        validate_version(&version)?;
        let path = slug_dir.join(&version);
        reject_symlink(&path)?;
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some((version, path)))
    }

    /// Read just the selected public artifact; an HTML request never loads CSV bundles or source.
    pub fn read_content(
        &self,
        slug: &str,
        version: Option<&str>,
        raw: bool,
    ) -> Result<Option<String>, String> {
        let Some((_, path)) = self.report_directory(slug, version)? else {
            return Ok(None);
        };
        let file = path.join(if raw { "source.rmd" } else { "rendered.html" });
        reject_symlink(&file)?;
        fs::read_to_string(file)
            .map(Some)
            .map_err(|e| e.to_string())
    }

    /// List/report-info only need small metadata, not rendered HTML or resolved source assets.
    pub fn summary(&self, slug: &str, version: Option<&str>) -> Result<Option<Value>, String> {
        Ok(self
            .summary_record(slug, version)?
            .map(|(summary, _)| summary))
    }

    /// Read the latest summary and the stored bytes in its four fixed files.
    pub(crate) fn summary_with_size(
        &self,
        slug: &str,
    ) -> Result<Option<(Value, Option<u64>)>, String> {
        let Some((summary, path)) = self.summary_record(slug, None)? else {
            return Ok(None);
        };
        let mut bytes = Some(0u64);
        for name in ["source.rmd", "rendered.html", "meta.json", "assets.json"] {
            let file = path.join(name);
            match fs::symlink_metadata(&file) {
                Ok(meta) if meta.file_type().is_file() => {
                    bytes = bytes.map(|total| total.saturating_add(meta.len()));
                }
                Err(error)
                    if name == "assets.json" && error.kind() == std::io::ErrorKind::NotFound => {}
                _ => bytes = None,
            }
        }
        Ok(Some((summary, bytes)))
    }

    fn summary_record(
        &self,
        slug: &str,
        version: Option<&str>,
    ) -> Result<Option<(Value, PathBuf)>, String> {
        let Some((version, path)) = self.report_directory(slug, version)? else {
            return Ok(None);
        };
        let file = path.join("meta.json");
        reject_symlink(&file)?;
        let metadata: Value = serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let summary = serde_json::json!({"slug":slug,"version":version,"frontmatter":metadata.get("frontmatter").cloned().unwrap_or_default(),"updatedAt":metadata.get("createdAt").or_else(|| metadata.get("updated_at")).and_then(Value::as_str).unwrap_or_default()});
        Ok(Some((summary, path)))
    }

    fn read_version(&self, slug: &str, version: &str) -> Result<Option<StoredReport>, String> {
        validate_version(version)?;
        let path = self.slug_dir(slug)?.join(version);
        reject_symlink(&path)?;
        if !path.exists() {
            return Ok(None);
        }
        for name in ["source.rmd", "rendered.html", "meta.json", "assets.json"] {
            reject_symlink(&path.join(name))?;
        }
        let source = fs::read_to_string(path.join("source.rmd")).map_err(|e| e.to_string())?;
        let html = fs::read_to_string(path.join("rendered.html")).map_err(|e| e.to_string())?;
        let meta: Value =
            serde_json::from_slice(&fs::read(path.join("meta.json")).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let assets: Value = fs::read(path.join("assets.json"))
            .ok()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default();
        Ok(Some(StoredReport {
            slug: slug.into(),
            version: version.into(),
            source,
            report_folder: serde_json::from_value(
                assets
                    .get("reportFolder")
                    .or_else(|| assets.get("report_folder"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            )
            .map_err(|e| e.to_string())?,
            html,
            frontmatter: meta.get("frontmatter").cloned().unwrap_or_default(),
            // Old immutable versions remain readable without rewriting their files.
            created_at: meta
                .get("createdAt")
                .or_else(|| meta.get("updated_at"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            content_sha256: meta
                .get("contentSha256")
                .or_else(|| meta.get("content_sha256"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            styles: serde_json::from_value(
                assets
                    .get("styles")
                    .cloned()
                    .unwrap_or(serde_json::json!([])),
            )
            .unwrap_or_default(),
            data: serde_json::from_value(
                assets.get("data").cloned().unwrap_or(serde_json::json!({})),
            )
            .unwrap_or_default(),
            files: serde_json::from_value(
                assets
                    .get("files")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
            )
            .unwrap_or_default(),
        }))
    }

    /// The bytes of a file the report links to, or `None` when this version links no such file.
    pub fn read_file(
        &self,
        slug: &str,
        version: &str,
        path: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        use base64::Engine;
        use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, Visitor};

        // The immutable assets file combines files and data; stream past everything except the
        // requested file so a missing attachment cannot allocate a whole report's resources.
        struct FindFile<'a>(&'a str);
        struct Assets<'a>(&'a str);
        impl<'de> DeserializeSeed<'de> for FindFile<'_> {
            type Value = Option<String>;
            fn deserialize<D: serde::Deserializer<'de>>(
                self,
                d: D,
            ) -> Result<Self::Value, D::Error> {
                struct Files<'a>(&'a str);
                impl<'de> Visitor<'de> for Files<'_> {
                    type Value = Option<String>;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("a file map")
                    }
                    fn visit_map<M: MapAccess<'de>>(
                        self,
                        mut map: M,
                    ) -> Result<Self::Value, M::Error> {
                        let mut found = None;
                        while let Some(key) = map.next_key::<String>()? {
                            if key == self.0 {
                                found = Some(map.next_value()?);
                            } else {
                                map.next_value::<IgnoredAny>()?;
                            }
                        }
                        Ok(found)
                    }
                }
                d.deserialize_map(Files(self.0))
            }
        }
        impl<'de> Visitor<'de> for Assets<'_> {
            type Value = Option<String>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("report assets")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut found = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "files" {
                        found = map.next_value_seed(FindFile(self.0))?;
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(found)
            }
        }
        validate_version(version)?;
        let dir = self.slug_dir(slug)?.join(version);
        reject_symlink(&dir)?;
        if !dir.exists() {
            return Ok(None);
        }
        let assets = dir.join("assets.json");
        reject_symlink(&assets)?;
        let Ok(file) = fs::File::open(assets) else {
            return Ok(None);
        };
        let mut deserializer = serde_json::Deserializer::from_reader(std::io::BufReader::new(file));
        let encoded = serde::Deserializer::deserialize_map(&mut deserializer, Assets(path))
            .ok()
            .flatten();
        encoded
            .map(|value| {
                base64::engine::general_purpose::STANDARD
                    .decode(value)
                    .map_err(|e| e.to_string())
            })
            .transpose()
    }

    /// Keeps the runtime a stored report links to, once, under its content id. A version is
    /// permanent, so every runtime it was published against stays served.
    pub fn keep_runtime(&self, id: &str, js: &str, wasm: &[u8]) -> Result<(), String> {
        let dir = self.dir.join("_runtime");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for (name, bytes) in [
            (format!("{id}.js"), js.as_bytes()),
            (format!("{id}_bg.wasm"), wasm),
        ] {
            let path = dir.join(&name);
            reject_symlink(&path)?;
            if !path.exists() {
                let staging = dir.join(format!(".{name}.{}", std::process::id()));
                write_synced(&staging, bytes)?;
                fs::rename(&staging, &path).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    /// A kept runtime file by name (`<id>.js` or `<id>_bg.wasm`); other names are not found.
    pub fn runtime_file(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        let id = name
            .strip_suffix(".js")
            .or_else(|| name.strip_suffix("_bg.wasm"))
            .unwrap_or_default();
        if id.len() != 16
            || !id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Ok(None);
        }
        let path = self.dir.join("_runtime").join(name);
        reject_symlink(&path)?;
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// The share card kept beside a version, if it has been drawn (ARC-69).
    pub fn share_card(&self, slug: &str, version: &str) -> Result<Option<Vec<u8>>, String> {
        let Some((_, dir)) = self.report_directory(slug, Some(version))? else {
            return Ok(None);
        };
        let path = dir.join("share.png");
        reject_symlink(&path)?;
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Keeps the share card beside its version. A card already kept stays: versions are immutable.
    pub fn put_share_card(&self, slug: &str, version: &str, png: &[u8]) -> Result<(), String> {
        if self.hosted {
            return self
                .locked(|| {
                    let slug_dir = self.slug_dir(slug)?;
                    let org_dir = slug_dir
                        .parent()
                        .ok_or_else(|| "Invalid report directory".to_owned())?;
                    if !org_dir.exists() {
                        return Ok(());
                    }
                    let needed = png.len() as u64;
                    if dir_bytes(org_dir)?.saturating_add(needed) > self.org_quota
                        || self.quota.is_some_and(|limit| {
                            dir_bytes(&self.dir)
                                .map_or(true, |used| used.saturating_add(needed) > limit)
                        })
                    {
                        // Rendering still succeeds, but the derived image cannot exceed either quota.
                        return Ok(());
                    }
                    self.put_share_card_unchecked(slug, version, png)
                        .map_err(PublishError::from)
                })
                .map_err(|e| e.to_string());
        }
        // Account-scoped ownership accounting cannot safely infer the owner's other slugs here.
        if self.account_quota.is_some() {
            return Ok(());
        }
        self.put_share_card_unchecked(slug, version, png)
    }

    fn put_share_card_unchecked(
        &self,
        slug: &str,
        version: &str,
        png: &[u8],
    ) -> Result<(), String> {
        let Some((_, dir)) = self.report_directory(slug, Some(version))? else {
            return Ok(());
        };
        let path = dir.join("share.png");
        reject_symlink(&path)?;
        if path.exists() {
            return Ok(());
        }
        // One staging name per call: two first requests must not write the same half-made file.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let staging = dir.join(format!(
            ".share.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&staging);
        write_synced(&staging, png)?;
        // Linking fails when another request got there first, so the first card kept is whole and
        // is never replaced; a rename would overwrite it.
        let linked = fs::hard_link(&staging, &path);
        let _ = fs::remove_file(&staging);
        match linked {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Whether the store has ever held `slug`, a deleted version included.
    pub fn has_slug(&self, slug: &str) -> Result<bool, String> {
        Ok(self.slug_dir(slug)?.exists())
    }

    pub fn list_versions(&self, slug: &str) -> Result<Vec<String>, String> {
        let path = self.slug_dir(slug)?;
        if !path.exists() || path.join(".deleted").exists() {
            return Ok(vec![]);
        }
        let mut versions = Vec::new();
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let version = entry.file_name().to_string_lossy().to_string();
            if entry.file_type().map_err(|e| e.to_string())?.is_dir()
                && validate_version(&version).is_ok()
            {
                versions.push(version);
            }
        }
        versions.sort_by(
            |a, b| match (semver::Version::parse(a), semver::Version::parse(b)) {
                (Ok(a), Ok(b)) => b.cmp(&a),
                _ => b.cmp(a),
            },
        );
        Ok(versions)
    }

    pub fn list(&self) -> Result<Vec<Value>, String> {
        let mut rows = Vec::new();
        if self.hosted {
            let orgs = self.dir.join("orgs");
            reject_symlink(&orgs)?;
            if !orgs.exists() {
                return Ok(rows);
            }
            for org in fs::read_dir(orgs).map_err(|e| e.to_string())? {
                let org = org.map_err(|e| e.to_string())?;
                if !org.file_type().map_err(|e| e.to_string())?.is_dir() {
                    continue;
                }
                let org_id = org.file_name().to_string_lossy().to_string();
                if org_id.len() != 8
                    || !org_id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                {
                    continue;
                }
                for entry in fs::read_dir(org.path()).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let key = format!("{org_id}/{}", entry.file_name().to_string_lossy());
                    if !entry.file_type().map_err(|e| e.to_string())?.is_dir()
                        || self.validate_key(&key).is_err()
                    {
                        continue;
                    }
                    if let Some(mut summary) = self.summary(&key, None)? {
                        summary["org"] = Value::String(org_id.clone());
                        summary["slug"] = Value::String(key.split_once('/').unwrap().1.to_owned());
                        rows.push(summary);
                    }
                }
            }
            rows.sort_by(|a, b| {
                (a["org"].as_str(), a["slug"].as_str())
                    .cmp(&(b["org"].as_str(), b["slug"].as_str()))
            });
            return Ok(rows);
        }
        for entry in fs::read_dir(self.dir.as_ref()).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let slug = entry.file_name().to_string_lossy().to_string();
            if !entry.file_type().map_err(|e| e.to_string())?.is_dir()
                || validate_slug(&slug).is_err()
            {
                continue;
            }
            if let Some(summary) = self.summary(&slug, None)? {
                rows.push(summary);
            }
        }
        rows.sort_by(|a, b| a["slug"].as_str().cmp(&b["slug"].as_str()));
        Ok(rows)
    }
}

pub fn validate_slug(slug: &str) -> Result<(), String> {
    richview_core::slug_problem(slug).map_or(Ok(()), Err)
}

pub fn validate_version(version: &str) -> Result<(), String> {
    richview_core::version_problem(version).map_or(Ok(()), Err)
}

/// Total bytes of every file under `dir`, staging leftovers included.
fn dir_bytes(dir: &Path) -> Result<u64, String> {
    let mut total = 0;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            total += dir_bytes(&entry.path())?;
        } else if kind.is_file() {
            total += entry.metadata().map_err(|e| e.to_string())?.len();
        }
    }
    Ok(total)
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(format!("Storage symlink rejected: {}", path.display()))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod signup_capacity_tests {
    use super::*;

    #[test]
    fn operator_storage_threshold_includes_half_the_quota() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FsStorage::new(dir.path()).unwrap().with_quota(Some(100));
        assert!(!storage.operator_storage_half_full(49));
        assert!(storage.operator_storage_half_full(50));
    }

    #[test]
    fn signup_space_is_rechecked_after_a_report_write_or_delete() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true)
            .with_quota(Some(100_000));
        assert!(storage.signup_space_available().unwrap());
        let report = StoredReport {
            slug: "aaaa0001/report".into(),
            version: "1.0.0".into(),
            source: "x".repeat(81_000),
            report_folder: None,
            html: String::new(),
            frontmatter: serde_json::json!({}),
            created_at: "now".into(),
            styles: vec![],
            data: BTreeMap::new(),
            files: BTreeMap::new(),
            content_sha256: String::new(),
        };
        storage.put_version(report).unwrap();
        assert!(!storage.signup_space_available().unwrap());
        storage.delete_report("aaaa0001/report").unwrap();
        assert!(storage.signup_space_available().unwrap());
    }
}

#[cfg(test)]
mod summary_size_tests {
    use super::*;

    #[test]
    fn latest_summary_counts_only_fixed_version_files() {
        let dir = tempfile::tempdir().unwrap();
        let version = dir.path().join("example/1.0.0");
        fs::create_dir_all(&version).unwrap();
        fs::write(version.join("source.rmd"), b"source").unwrap();
        fs::write(version.join("rendered.html"), b"html").unwrap();
        fs::write(
            version.join("meta.json"),
            br#"{"frontmatter":{"title":"Example"}}"#,
        )
        .unwrap();
        fs::write(version.join("assets.json"), b"{}").unwrap();
        fs::write(version.join("unrelated"), b"do not count").unwrap();
        let store = FsStorage::new(dir.path()).unwrap();
        let (summary, size) = store.summary_with_size("example").unwrap().unwrap();
        assert_eq!(summary["version"], "1.0.0");
        assert_eq!(size, Some(6 + 4 + 35 + 2));
        fs::remove_file(version.join("source.rmd")).unwrap();
        assert_eq!(store.summary_with_size("example").unwrap().unwrap().1, None);
        assert!(store.summary_with_size("missing").unwrap().is_none());
    }
}
