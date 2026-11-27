//! One scoped staff read, attributed to an individual before any bytes leave the host.
use super::AppState;
use crate::{
    access::valid_hosted_person_id,
    storage::{ReportKey, validate_version},
    time::Instant,
};
use axum::{
    extract::{Path as RoutePath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(super) fn directory() -> Option<PathBuf> {
    std::env::var_os("RICHVIEW_STAFF_ACCESS_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Basis {
    OwnerPermission,
    LegalOrder,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "path",
    rename_all = "lowercase",
    deny_unknown_fields
)]
enum Artifact {
    Source,
    Html,
    Data(String),
    File(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    actor: String,
    owner: String,
    case: String,
    evidence: String,
    purpose: String,
    basis: Basis,
    sealed: bool,
    org: String,
    slug: String,
    version: String,
    artifact: Artifact,
    issued: i64,
    expires: i64,
}

fn hex_id(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Grant {
    fn key(&self) -> Option<ReportKey> {
        let valid_duration = self
            .expires
            .checked_sub(self.issued)
            .is_some_and(|seconds| (1..=3600).contains(&seconds));
        if !valid_duration
            || !(valid_hosted_person_id(&self.actor)
                && valid_hosted_person_id(&self.owner)
                && valid_hosted_person_id(&self.case)
                && valid_hosted_person_id(&self.evidence))
            || self.owner == "service"
            || self.purpose.trim().is_empty()
            || self.purpose.len() > 240
            || self.purpose.chars().any(char::is_control)
            || (self.sealed && !matches!(self.basis, Basis::LegalOrder))
            || validate_version(&self.version).is_err()
        {
            return None;
        }
        if let Artifact::Data(path) | Artifact::File(path) = &self.artifact
            && (!richview_core::document::report_relative_path(path)
                || path.len() > 512
                || path.chars().any(char::is_control))
        {
            return None;
        }
        ReportKey::hosted(&self.org, &self.slug).ok()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Outcome {
    Released,
    Refused,
    Unavailable,
    AlreadyUsed,
}

#[derive(Debug, Deserialize, Serialize)]
struct Event {
    grant_id: String,
    actor: String,
    time: String,
    outcome: Outcome,
    grant: Grant,
    /// How many times this refusal was recorded, once it was recorded again: one record is kept
    /// for a grant, a person and an outcome, and a person who asks again and again only raises this.
    #[serde(default, skip_serializing_if = "is_zero")]
    count: u32,
    /// When it was last recorded, with `count`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last: Option<String>,
}

fn is_zero(count: &u32) -> bool {
    *count == 0
}

fn invalid() -> io::Error {
    io::Error::other("Staff access record unavailable")
}

fn load<T: serde::de::DeserializeOwned>(file: &Path) -> io::Result<T> {
    serde_json::from_reader(fs::File::open(file)?).map_err(io::Error::other)
}

// Missing authority directories are an outage. A missing record within a healthy directory
// is the only case in which a revocation or disclosure flag is absent.
fn flagged(root: &Path, kind: &str, id: &str) -> io::Result<bool> {
    let dir = root.join(kind);
    fs::File::open(&dir)?;
    dir.join(id).try_exists()
}

fn identify(root: &Path, token: &str) -> io::Result<Option<String>> {
    if !hex_id(token, 64) {
        return Ok(None);
    }
    let digest = format!("{:x}", Sha256::digest(token.as_bytes()));
    let dir = root.join("identities");
    fs::File::open(&dir)?;
    match fs::read_to_string(dir.join(digest)) {
        Ok(actor) if valid_hosted_person_id(actor.trim()) => Ok(Some(actor.trim().into())),
        Ok(_) => Err(invalid()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Notes on the record of a refusal that it happened again at `time`: the record is replaced whole,
/// so a reader sees either the count before or the count after.
fn count_again(record: &Path, dir: &Path, time: &str) -> io::Result<()> {
    // One at a time, the owner's record and the custodian's alike: each reads the count and
    // replaces it, so two together would write the same number.
    static COUNTING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = COUNTING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut first: Event = load(record)?;
    first.count = first.count.max(1).saturating_add(1);
    // A refusal that was stamped before it was written may be written after a later one: the
    // record keeps the latest time it was asked, whichever of them was written last. The stamps
    // are fixed-width UTC text, which order as the times do.
    let latest = [first.last.as_deref(), Some(first.time.as_str()), Some(time)]
        .into_iter()
        .flatten()
        .max()
        .map(str::to_owned);
    first.last = latest;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer(&mut file, &first).map_err(io::Error::other)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(record).map_err(|e| e.error)?;
    Ok(())
}

fn journal(root: &Path, event: &Event, owner_visible: bool) -> io::Result<()> {
    let events = root.join("events");
    let parent = fs::File::open(&events)?;
    let dir = events.join(if owner_visible {
        event.grant.owner.as_str()
    } else {
        ".custodian"
    });
    match fs::create_dir(&dir) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    parent.sync_all()?;
    let mut file = tempfile::NamedTempFile::new_in(&dir)?;
    serde_json::to_writer(&mut file, event).map_err(io::Error::other)?;
    file.flush()?;
    file.as_file().sync_all()?;
    let refusal = !matches!(event.outcome, Outcome::Released);
    let name = if refusal {
        // A refusal cannot consume a grant or replace a successful release. It is named by the
        // grant, the person who asked and what came of it, so a person who asks again and again
        // leaves one record each of at most three kinds and cannot fill the disk.
        let asked = Sha256::digest(event.actor.as_bytes());
        let by: String = asked.iter().take(8).map(|b| format!("{b:02x}")).collect();
        let outcome = match event.outcome {
            Outcome::Unavailable => "unavailable",
            Outcome::AlreadyUsed => "already-used",
            _ => "refused",
        };
        format!("refusal-{}-{by}-{outcome}.json", event.grant_id)
    } else {
        format!("release-{}.json", event.grant_id)
    };
    let record = dir.join(name);
    match file.persist_noclobber(&record) {
        Ok(_) => {}
        // The same refusal is already on record: it counts this one. A count that cannot be
        // made is not let go: the refusal is then answered as an outage, not as a plain refusal
        // that the owner's history would never show.
        Err(e) if refusal && e.error.kind() == io::ErrorKind::AlreadyExists => {
            count_again(&record, &dir, &event.time)?;
        }
        Err(e) => return Err(e.error),
    }
    fs::File::open(dir)?.sync_all()
}

type Denial = StatusCode;
const HOLD: Denial = StatusCode::SERVICE_UNAVAILABLE;
const DENY: Denial = StatusCode::FORBIDDEN;

fn inspect(root: &Path, state: &AppState, token: &str, id: &str) -> Result<Vec<u8>, Denial> {
    if !state.access.is_hosted() || !hex_id(id, 32) {
        return Err(DENY);
    }
    let actor = identify(root, token).map_err(|_| HOLD)?.ok_or(DENY)?;
    let grant: Grant = load(&root.join("grants").join(id)).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            DENY
        } else {
            HOLD
        }
    })?;
    let key = grant.key().ok_or(HOLD)?;
    let mut event = Event {
        grant_id: id.into(),
        actor,
        time: Instant::now().millis(),
        outcome: Outcome::Refused,
        grant,
        count: 0,
        last: None,
    };
    let allowed = |e: &Event| -> Result<bool, Denial> {
        let now = Instant::now().unix_seconds();
        Ok(e.actor == e.grant.actor
            && e.grant.issued <= now
            && now < e.grant.expires
            && !flagged(root, "revoked", id).map_err(|_| HOLD)?)
    };
    let already = root
        .join("events")
        .join(&event.grant.owner)
        .join(format!("release-{id}.json"));
    if allowed(&event)?
        && !already.try_exists().map_err(|_| HOLD)?
        && state.access.report_org_active(key.as_str())
        && state.access.acl(key.as_str()).owner == event.grant.owner
    {
        let v = event.grant.version.as_str();
        let content = match &event.grant.artifact {
            Artifact::Source | Artifact::Html => state
                .storage
                .read_content(
                    key.as_str(),
                    Some(v),
                    matches!(event.grant.artifact, Artifact::Source),
                )
                .map(|s| s.map(String::into_bytes)),
            Artifact::File(path) => state.storage.read_file(key.as_str(), v, path),
            Artifact::Data(path) => state
                .storage
                .read_data_entry(key.as_str(), v, path)
                .map(|s| s.map(String::into_bytes)),
        };
        if let Ok(Some(bytes)) = content {
            let owner = event.grant.owner.clone();
            // Ownership transfers are serialized with this final authorization and audit commit.
            let authority = fs::File::open(root.join("authority.lock")).map_err(|_| HOLD)?;
            fs2::FileExt::lock_shared(&authority).map_err(|_| HOLD)?;
            let release =
                state
                    .access
                    .while_staff_owner(key.as_str(), &owner, || -> Result<(), Denial> {
                        if state
                            .storage
                            .is_deleted(key.as_str(), &event.grant.version)
                            .map_err(|_| HOLD)?
                            || !allowed(&event)?
                            || identify(root, token).map_err(|_| HOLD)?.as_deref()
                                != Some(event.actor.as_str())
                        {
                            return Err(DENY);
                        }
                        event.time = Instant::now().millis();
                        event.outcome = Outcome::Released;
                        journal(root, &event, true).map_err(|e| {
                            if e.kind() == io::ErrorKind::AlreadyExists {
                                DENY
                            } else {
                                HOLD
                            }
                        })
                    });
            match release {
                Some(Ok(())) => return Ok(bytes),
                Some(Err(StatusCode::SERVICE_UNAVAILABLE)) => return Err(HOLD),
                _ => event.outcome = Outcome::Refused,
            }
        } else {
            event.outcome = Outcome::Unavailable;
        }
    }
    if event.actor == event.grant.actor && already.try_exists().map_err(|_| HOLD)? {
        event.outcome = Outcome::AlreadyUsed;
    }
    state
        .access
        .while_staff_owner(key.as_str(), &event.grant.owner, || {
            journal(root, &event, true)
        })
        .unwrap_or_else(|| journal(root, &event, false))
        .map_err(|_| HOLD)?;
    Err(DENY)
}

#[derive(Clone)]
struct InspectorListener {
    app: AppState,
    directory: PathBuf,
    maintenance_hash: [u8; 32],
}

pub(super) fn router(
    app: AppState,
    maintenance_token: &str,
    root: Option<PathBuf>,
) -> axum::Router {
    let Some(directory) = root else {
        return axum::Router::new();
    };
    axum::Router::new()
        .route("/staff/inspections/{grant}", axum::routing::post(read))
        .with_state(InspectorListener {
            app,
            directory,
            maintenance_hash: Sha256::digest(maintenance_token.as_bytes()).into(),
        })
}

async fn read(
    State(listener): State<InspectorListener>,
    RoutePath(id): RoutePath<String>,
    headers: HeaderMap,
) -> Response {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_owned();
    let result = if super::operator::token_matches(&listener.maintenance_hash, &token) {
        Err(DENY)
    } else {
        tokio::task::spawn_blocking(move || {
            inspect(&listener.directory, &listener.app, &token, &id)
        })
        .await
        .unwrap_or(Err(HOLD))
    };
    let mut response = match result {
        Ok(bytes) => {
            let size = bytes.len();
            let mut response = (
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    (header::CONTENT_DISPOSITION, "attachment"),
                ],
                bytes,
            )
                .into_response();
            response
                .headers_mut()
                .insert(header::CONTENT_LENGTH, size.into());
            response
        }
        Err(status) => status.into_response(),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

pub(super) fn history(root: &Path, owner: &str) -> io::Result<String> {
    if !valid_hosted_person_id(owner) {
        return Err(invalid());
    }
    let events = root.join("events");
    fs::File::open(&events)?;
    let mut rows = Vec::new();
    let dir = events.join(owner);
    if dir.try_exists()? {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let e: Event = load(&path)?;
            if e.grant.owner != owner || !hex_id(&e.grant_id, 32) {
                return Err(invalid());
            }
            if e.grant.sealed && !flagged(root, "disclosed", &e.grant_id)? {
                continue;
            }
            let artifact = match e.grant.artifact {
                Artifact::Source => "source".into(),
                Artifact::Html => "rendered HTML".into(),
                Artifact::Data(p) => format!("data: {p}"),
                Artifact::File(p) => format!("attachment: {p}"),
            };
            let outcome = match e.outcome {
                Outcome::Released => "Released",
                Outcome::Refused => "Refused",
                Outcome::Unavailable => "Artifact unavailable",
                Outcome::AlreadyUsed => "Grant already used",
            };
            let again = match (e.count, &e.last) {
                (count, Some(last)) if count > 1 => format!(" · asked {count} times, last {last}"),
                _ => String::new(),
            };
            let text = format!(
                "{} · {} · {} · case {} · {}/{} v{} · {} · {}{again}",
                e.time,
                outcome,
                e.actor,
                e.grant.case,
                e.grant.org,
                e.grant.slug,
                e.grant.version,
                artifact,
                e.grant.purpose
            );
            rows.push(text);
        }
    }
    rows.sort_unstable_by(|a, b| b.cmp(a));
    let mut page = String::from(
        "<h1>Staff access history</h1><p>This history covers requests through the staff inspection service for reports you owned at the time. Times are UTC.</p>",
    );
    if rows.is_empty() {
        page.push_str("<p>No entries are available to show.</p>");
    } else {
        page.push_str("<ul>");
        for row in rows {
            page.push_str(&format!("<li>{}</li>", super::ui::escape(&row)));
        }
        page.push_str("</ul>");
    }
    page.push_str("<p><a href=\"/account\">Back to account</a></p>");
    Ok(page)
}

#[cfg(test)]
mod tests;
