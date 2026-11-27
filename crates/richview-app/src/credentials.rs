//! The sign-ins this computer keeps: one per server, in a private file beside the publish choice.
//! A sign-in is a token, and for one made by the browser also the refresh token that renews it
//! when the hour it lasts is nearly over, without asking its person again. A kept token is read
//! only for the server it was kept for, so it is sent nowhere else (FEA-SF-12, INT-79), and
//! nothing here prints or logs one (ARC-23).
use crate::ops::Failure;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, time::Duration};

/// A token with less than this left is renewed before it is used, so that it does not end in the
/// middle of an upload.
const RENEW_BEFORE_SECONDS: u64 = 300;
/// How long a command waits for another command on this computer that is renewing the same sign-in.
const LOCK_WAIT: Duration = Duration::from_secs(40);

/// The file is small; a larger one is not ours.
const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Deserialize, Default)]
struct Store {
    #[serde(default)]
    servers: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    token: String,
    /// Renews the token; only a sign-in made through the browser has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh: Option<String>,
    /// When the token ends, in seconds since 1970; none for a token that never ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires: Option<u64>,
    /// Who the server said the token belongs to: a label for the person who opens this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    /// The fingerprints of the tokens this same sign-in had before this one, newest last. Only a
    /// renewal adds to them; signing in again starts an empty list. A command that was refused
    /// with one of them was refused by this sign-in, and not by another one signed in since.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lineage: Vec<String>,
    kept: String,
}

/// How many earlier tokens of a sign-in are remembered: more than a command can be behind.
const MAX_LINEAGE: usize = 8;

/// A token's fingerprint: the start of its digest, which says which token it was and is no use as
/// one.
fn fingerprint(token: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes())
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The name a server's sign-in is kept under: its origin, so the same server written with or
/// without a trailing slash is one entry and any other host, port or scheme is another.
fn key(server: &str) -> Result<String, Failure> {
    Ok(crate::remote::server_url(server)
        .map_err(Failure::usage)?
        .origin()
        .ascii_serialization())
}

fn file() -> Result<std::path::PathBuf, Failure> {
    Ok(crate::publish_destination::config_dir()?.join("credentials.json"))
}

/// Reads the kept sign-ins. To use one (`strict`), the file must be a plain file only its owner
/// can read; to replace one, an open file is read anyway, because the write closes it again.
fn read(path: &Path, strict: bool) -> Result<Store, Failure> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(error) => {
            return Err(Failure::host(format!(
                "Cannot read {}: {error}",
                path.display()
            )));
        }
    };
    if !meta.file_type().is_file() || meta.len() > MAX_BYTES {
        return Err(Failure::host(format!(
            "{} is not a file of kept sign-ins. Remove it and run richview login again.",
            path.display()
        )));
    }
    #[cfg(unix)]
    if strict {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(Failure::host(format!(
                "{0} can be read by other users of this computer, so its sign-ins are not used. Run `chmod 600 {0}`, or run richview login again.",
                path.display()
            )));
        }
    }
    #[cfg(not(unix))]
    let _ = strict;
    let bytes = std::fs::read(path)
        .map_err(|e| Failure::host(format!("Cannot read {}: {e}", path.display())))?;
    // Only where it failed, never what serde read there: a type error quotes the value, and the
    // value may be a token.
    serde_json::from_slice(&bytes).map_err(|e| {
        Failure::host(format!(
            "{} is not a file of kept sign-ins (line {}, column {}). Remove it and run richview login again.",
            path.display(),
            e.line(),
            e.column()
        ))
    })
}

/// A sign-in as it is kept.
#[derive(Clone)]
pub(crate) struct Kept {
    pub token: String,
    pub refresh: Option<String>,
    pub expires: Option<u64>,
    pub account: Option<String>,
    /// The fingerprints of this sign-in's earlier tokens (`Entry::lineage`).
    pub lineage: Vec<String>,
}

impl Kept {
    /// A token with the refresh token that renews it and the seconds it lasts from now; without
    /// a refresh token it is one that never ends, pasted or made by an older server.
    pub(crate) fn new(
        token: &str,
        refresh: Option<&str>,
        lasts: Option<u64>,
        account: Option<&str>,
    ) -> Self {
        Self {
            token: token.to_owned(),
            refresh: refresh.map(str::to_owned),
            expires: refresh.and(lasts).map(|seconds| unix_now() + seconds),
            account: account.map(str::to_owned),
            lineage: Vec::new(),
        }
    }

    fn ends_soon(&self) -> bool {
        self.expires
            .is_some_and(|ends| ends <= unix_now() + RENEW_BEFORE_SECONDS)
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn kept_grant_in(path: &Path, server: &str) -> Result<Option<Kept>, Failure> {
    let key = key(server)?;
    Ok(read(path, true)?.servers.remove(&key).map(Kept::from))
}

impl From<Entry> for Kept {
    fn from(entry: Entry) -> Self {
        Self {
            token: entry.token,
            refresh: entry.refresh,
            expires: entry.expires,
            account: entry.account,
            lineage: entry.lineage,
        }
    }
}

#[cfg(test)]
fn kept_in(path: &Path, server: &str) -> Result<Option<String>, Failure> {
    Ok(kept_grant_in(path, server)?.map(|kept| kept.token))
}

fn keep_grant_in(path: &Path, server: &str, kept: &Kept) -> Result<(), Failure> {
    let key = key(server)?;
    // A file this cannot read is replaced: the person is signing in again.
    let mut store = read(path, false).unwrap_or_default();
    store.servers.insert(
        key,
        Entry {
            token: kept.token.clone(),
            refresh: kept.refresh.clone(),
            expires: kept.expires,
            account: kept.account.clone(),
            lineage: kept.lineage.clone(),
            kept: crate::time::Instant::now().seconds(),
        },
    );
    crate::publish_destination::save_private(path, &store)
}

#[cfg(test)]
fn keep_in(path: &Path, server: &str, token: &str, account: Option<&str>) -> Result<(), Failure> {
    keep_grant_in(path, server, &Kept::new(token, None, None, account))
}

/// Removes the sign-in kept for `server` and returns it, as it was when it was removed.
fn forget_in(path: &Path, server: &str) -> Result<Option<Kept>, Failure> {
    let key = key(server)?;
    let mut store = read(path, false)?;
    let Some(entry) = store.servers.remove(&key) else {
        return Ok(None);
    };
    if store.servers.is_empty() {
        std::fs::remove_file(path)
            .map_err(|e| Failure::host(format!("Cannot remove {}: {e}", path.display())))?;
    } else {
        crate::publish_destination::save_private(path, &store)?;
    }
    Ok(Some(Kept::from(entry)))
}

/// Keeps a sign-in for `server`, replacing an earlier one to that server. It waits for a command
/// that is renewing a sign-in, so that neither overwrites what the other keeps.
pub(crate) fn keep(server: &str, kept: &Kept) -> Result<(), Failure> {
    let path = file()?;
    keep_grant_locked(&path, server, kept, LOCK_WAIT)
}

fn keep_grant_locked(
    path: &Path,
    server: &str,
    kept: &Kept,
    wait: Duration,
) -> Result<(), Failure> {
    let _lock = lock_blocking_in(path, wait)?;
    keep_grant_in(path, server, kept)
}

/// Keeps a token that never ends, for `server`.
pub(crate) fn keep_token(server: &str, token: &str, account: Option<&str>) -> Result<(), Failure> {
    keep(server, &Kept::new(token, None, None, account))
}

/// The token to send to `server`, renewed first when it is nearly over: `None` when this computer
/// is not signed in there, or its sign-in has ended and was forgotten, so that a terminal can
/// sign its person in again. Several commands at once renew a sign-in once.
pub(crate) async fn usable(server: &str) -> Result<Option<String>, Failure> {
    usable_in(&file()?, server).await
}

async fn usable_in(path: &Path, server: &str) -> Result<Option<String>, Failure> {
    let Some(first) = kept_grant_in(path, server)? else {
        return Ok(None);
    };
    if first.refresh.is_none() || !first.ends_soon() {
        return Ok(Some(first.token));
    }
    let _lock = lock_in(path).await?;
    let Some(kept) = kept_grant_in(path, server)? else {
        return Ok(None);
    };
    // Another command on this computer may have renewed it while this one waited: the token kept
    // now is then this sign-in's next one. A login made meanwhile is not, and may be another
    // account or another organization; what this command is about to send is for the sign-in it
    // started with, so it stops and the person runs it again.
    if kept.token != first.token && !kept.lineage.contains(&fingerprint(&first.token)) {
        return Err(Failure::host(format!(
            "This computer's sign-in to {server} was replaced while this command waited. Run it again to use the new one."
        )));
    }
    let Some(refresh) = kept.refresh.clone().filter(|_| kept.ends_soon()) else {
        return Ok(Some(kept.token));
    };
    match crate::sign_in::renew(server, &refresh).await {
        Ok(next) => Ok(Some(keep_renewed(path, server, kept, next)?)),
        Err(crate::sign_in::Renewal::Ended) => {
            ended(path, server);
            Ok(None)
        }
        Err(crate::sign_in::Renewal::Unreachable(why)) => {
            if kept.expires.is_some_and(|ends| ends > unix_now()) {
                Ok(Some(kept.token))
            } else {
                Err(Failure::host(format!(
                    "Cannot reach {server} to renew this computer's sign-in: {why}"
                )))
            }
        }
    }
}

/// Keeps what a renewal gave, as the same sign-in as `prior`: its account, and its earlier tokens
/// with the one this renewed added. Returns its token. The caller holds the lock.
fn keep_renewed(
    path: &Path,
    server: &str,
    prior: Kept,
    next: crate::sign_in::Renewed,
) -> Result<String, Failure> {
    let mut lineage = prior.lineage;
    lineage.push(fingerprint(&prior.token));
    if lineage.len() > MAX_LINEAGE {
        lineage.drain(..lineage.len() - MAX_LINEAGE);
    }
    keep_grant_in(
        path,
        server,
        &Kept {
            account: next.account.clone().or(prior.account),
            lineage,
            ..next.kept
        },
    )?;
    Ok(next.token)
}

/// The server says this computer's sign-in is over: forget it and say so, so that a terminal asks
/// its person again. The caller holds the lock.
fn ended(path: &Path, server: &str) {
    let _ = forget_in(path, server);
    eprintln!(
        "richview: this computer's sign-in to {} has ended.",
        server.trim_end_matches('/')
    );
}

/// The token to use in place of `refused`, which the server did not accept: renews the sign-in
/// this computer keeps for `server` once, and only when `refused` is that sign-in's own token (a
/// token the person gave is theirs, and is never swapped for another identity). Its time may have
/// run out by the server's clock and not the local one, or the server may have ended it early.
/// `None` when there is nothing to renew or the server cannot be asked.
pub(crate) async fn renew_refused(server: &str, refused: &str) -> Result<Option<String>, Failure> {
    renew_refused_in(&file()?, server, refused).await
}

async fn renew_refused_in(
    path: &Path,
    server: &str,
    refused: &str,
) -> Result<Option<String>, Failure> {
    let Some(first) = kept_grant_in(path, server)? else {
        return Ok(None);
    };
    if first.refresh.is_none() || first.token != refused {
        return Ok(None);
    }
    let _lock = lock_in(path).await?;
    let Some(kept) = kept_grant_in(path, server)? else {
        return Ok(None);
    };
    let Some(refresh) = kept.refresh.clone() else {
        return Ok(None);
    };
    // Another command on this computer renewed it while this one waited: the token kept now is
    // this sign-in's next one. Anything else kept there is another sign-in, made since by another
    // login to this server, possibly as another account or for another organization, and what
    // this command was sending was for the one it started with.
    if kept.token != refused {
        return Ok(kept
            .lineage
            .contains(&fingerprint(refused))
            .then_some(kept.token));
    }
    match crate::sign_in::renew(server, &refresh).await {
        Ok(next) => Ok(Some(keep_renewed(path, server, kept, next)?)),
        Err(crate::sign_in::Renewal::Ended) => {
            ended(path, server);
            Ok(None)
        }
        Err(crate::sign_in::Renewal::Unreachable(_)) => Ok(None),
    }
}

/// A lock on the kept sign-ins, held by the one command that renews one. It is the operating
/// system's, so it ends with the command, whatever ends it.
struct Lock(#[allow(dead_code)] std::fs::File);

fn open_lock(path: &Path) -> Result<(std::fs::File, std::path::PathBuf), Failure> {
    let dir = path
        .parent()
        .ok_or_else(|| Failure::host("Invalid configuration path"))?;
    std::fs::create_dir_all(dir)
        .map_err(|e| Failure::host(format!("Cannot create {}: {e}", dir.display())))?;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let lock = dir.join("credentials.lock");
    let file = options
        .open(&lock)
        .map_err(|e| Failure::host(format!("Cannot open {}: {e}", lock.display())))?;
    Ok((file, lock))
}

fn lock_refused(lock: &Path, error: impl std::fmt::Display) -> Failure {
    Failure::host(format!(
        "Cannot change this computer's sign-in: another richview command is using {}: {error}",
        lock.display()
    ))
}

async fn lock_in(path: &Path) -> Result<Lock, Failure> {
    let (file, lock) = open_lock(path)?;
    let until = std::time::Instant::now() + LOCK_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Lock(file)),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < until => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(lock_refused(&lock, error)),
        }
    }
}

/// `lock_in` for a change that is not async: it waits up to `wait`, since whoever holds the lock
/// is renewing a sign-in over the network and lets go within seconds.
fn lock_blocking_in(path: &Path, wait: Duration) -> Result<Lock, Failure> {
    let (file, lock) = open_lock(path)?;
    let until = std::time::Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Lock(file)),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(lock_refused(&lock, error)),
        }
    }
}

/// Forgets the sign-in kept for `server` and returns it, as it was kept at the moment it was
/// removed: `None` when there was none. Like `keep`, it waits for a command that is renewing a
/// sign-in, and reads and removes under that one lock: a renewal that finished first has replaced
/// the tokens, and it is the replacements that are returned to be revoked; one that finishes after
/// would keep the sign-in again, so it cannot start. A file that cannot be used (others can read
/// it) is still forgotten, and its sign-in cannot be returned.
pub(crate) fn forget(server: &str) -> Result<(bool, Option<Kept>), Failure> {
    let path = file()?;
    forget_locked(&path, server, LOCK_WAIT)
}

fn forget_locked(
    path: &Path,
    server: &str,
    wait: Duration,
) -> Result<(bool, Option<Kept>), Failure> {
    let _lock = lock_blocking_in(path, wait)?;
    let usable = kept_grant_in(path, server).ok().flatten().is_some();
    let removed = forget_in(path, server)?;
    Ok((removed.is_some(), removed.filter(|_| usable)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_token_is_private_and_found_only_for_its_own_server() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("richview/credentials.json");
        assert!(kept_in(&path, "https://app.richview.ai").unwrap().is_none());
        keep_in(
            &path,
            "https://app.richview.ai/",
            "rvt_one",
            Some("Ada <ada@example.test>"),
        )
        .unwrap();
        keep_in(&path, "http://127.0.0.1:4820", "rvt_two", None).unwrap();
        assert_eq!(
            kept_in(&path, "https://app.richview.ai")
                .unwrap()
                .as_deref(),
            Some("rvt_one")
        );
        assert_eq!(
            kept_in(&path, "http://127.0.0.1:4820/").unwrap().as_deref(),
            Some("rvt_two")
        );
        let file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            file["servers"]["https://app.richview.ai"]["account"],
            "Ada <ada@example.test>"
        );
        // Another host, port or scheme is another server: none of them gets a kept token.
        for other in [
            "https://app.richview.ai.evil.test",
            "https://app.richview.ai:8443",
            "https://reports.richview.ai",
            "http://127.0.0.1:4821",
            "http://localhost:4820",
            "https://127.0.0.1:4820",
        ] {
            assert!(kept_in(&path, other).unwrap().is_none(), "{other}");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(path.parent().unwrap()), 0o700);
        }
    }

    #[test]
    fn signing_in_again_replaces_and_forgetting_removes_only_that_server() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("richview/credentials.json");
        keep_in(&path, "https://a.example.test", "old", None).unwrap();
        keep_in(&path, "https://a.example.test", "new", None).unwrap();
        keep_in(&path, "https://b.example.test", "other", None).unwrap();
        assert_eq!(
            kept_in(&path, "https://a.example.test").unwrap().as_deref(),
            Some("new")
        );
        assert!(
            forget_in(&path, "https://a.example.test")
                .unwrap()
                .is_some()
        );
        assert!(
            forget_in(&path, "https://a.example.test")
                .unwrap()
                .is_none()
        );
        assert!(kept_in(&path, "https://a.example.test").unwrap().is_none());
        assert_eq!(
            kept_in(&path, "https://b.example.test").unwrap().as_deref(),
            Some("other")
        );
        // The last sign-in takes the file with it.
        assert!(
            forget_in(&path, "https://b.example.test")
                .unwrap()
                .is_some()
        );
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn a_token_with_time_left_is_used_as_it_is_and_an_unreachable_server_changes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("richview/credentials.json");
        // Nothing listens on this port: asking it anything fails at once.
        let server = "http://127.0.0.1:9";
        let renewable = |token: &str, ends: u64| Kept {
            token: token.to_owned(),
            refresh: Some("rvr_refresh".to_owned()),
            expires: Some(ends),
            account: None,
            lineage: vec![],
        };
        assert!(usable_in(&path, server).await.unwrap().is_none());
        // A token that never ends, and one with more than five minutes left, are not renewed.
        keep_in(&path, server, "rvt_plain", None).unwrap();
        assert_eq!(
            usable_in(&path, server).await.unwrap().as_deref(),
            Some("rvt_plain")
        );
        keep_grant_in(&path, server, &renewable("rvt_hour", unix_now() + 3600)).unwrap();
        assert_eq!(
            usable_in(&path, server).await.unwrap().as_deref(),
            Some("rvt_hour")
        );
        // Nearly over and the server cannot be asked: the token that still works is used, and the
        // sign-in is kept for the next try.
        keep_grant_in(&path, server, &renewable("rvt_soon", unix_now() + 100)).unwrap();
        assert_eq!(
            usable_in(&path, server).await.unwrap().as_deref(),
            Some("rvt_soon")
        );
        assert_eq!(
            kept_grant_in(&path, server)
                .unwrap()
                .unwrap()
                .refresh
                .as_deref(),
            Some("rvr_refresh")
        );
        // Over and the server cannot be asked: it says so in words that carry no secret.
        keep_grant_in(
            &path,
            server,
            &renewable("rvt_over", unix_now().saturating_sub(10)),
        )
        .unwrap();
        let error = usable_in(&path, server).await.err().unwrap();
        assert!(
            error.message().contains("Cannot reach http://127.0.0.1:9")
                && !error.message().contains("rvt_")
                && !error.message().contains("rvr_"),
            "{}",
            error.message()
        );
        assert!(kept_grant_in(&path, server).unwrap().is_some());
    }

    #[tokio::test]
    async fn keeping_and_forgetting_wait_for_a_command_that_renews() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("richview/credentials.json");
        let server = "https://a.example.test";
        let kept = Kept::new("rvt_new", Some("rvr_new"), Some(3600), None);
        keep_in(&path, server, "rvt_old", None).unwrap();
        // While another command holds the lock, neither changes the file.
        let renewing = lock_in(&path).await.unwrap();
        let short = Duration::from_millis(150);
        for refused in [
            keep_grant_locked(&path, server, &kept, short).err(),
            forget_locked(&path, server, short).err(),
        ] {
            let refused = refused.expect("a change went through a held lock");
            assert!(
                refused.message().contains("another richview command"),
                "{}",
                refused.message()
            );
        }
        assert_eq!(kept_in(&path, server).unwrap().as_deref(), Some("rvt_old"));
        // When it lets go they do.
        drop(renewing);
        keep_grant_locked(&path, server, &kept, short).unwrap();
        assert_eq!(kept_in(&path, server).unwrap().as_deref(), Some("rvt_new"));
        assert!(forget_locked(&path, server, short).unwrap().0);
        assert!(kept_in(&path, server).unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_others_can_read_is_not_used_and_its_refusal_shows_no_token() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("richview/credentials.json");
        keep_in(&path, "https://a.example.test", "rvt_secret", None).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let refused = kept_in(&path, "https://a.example.test").err().unwrap();
        assert!(
            refused.message().contains("chmod 600"),
            "{}",
            refused.message()
        );
        assert!(!refused.message().contains("rvt_secret"));
        // Signing in again closes the file.
        keep_in(&path, "https://a.example.test", "rvt_next", None).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            kept_in(&path, "https://a.example.test").unwrap().as_deref(),
            Some("rvt_next")
        );
        // A broken file is refused in words that quote none of it.
        std::fs::write(
            &path,
            "{\"servers\": {\"https://a.example.test\": rvt_secret",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let broken = kept_in(&path, "https://a.example.test").err().unwrap();
        assert!(
            !broken.message().contains("rvt_secret"),
            "{}",
            broken.message()
        );
        // Nor does a well-formed file whose values have the wrong type, where serde would quote
        // the value: a token written by hand where an entry belongs.
        std::fs::write(
            &path,
            "{\"servers\": {\"https://a.example.test\": \"rvt_secret\"}}",
        )
        .unwrap();
        let mistyped = kept_in(&path, "https://a.example.test").err().unwrap();
        assert!(
            !mistyped.message().contains("rvt_secret")
                && mistyped.message().contains("(line 1, column "),
            "{}",
            mistyped.message()
        );
    }

    #[tokio::test]
    async fn a_command_that_waited_to_renew_never_goes_on_with_a_sign_in_made_since() {
        let server = "http://127.0.0.1:9";
        let ending = Kept::new("rvt_first", Some("rvr_refresh"), Some(10), None);
        let later = |token: &str| Kept::new(token, Some("rvr_refresh"), Some(3600), None);
        for (label, change, stops) in [
            (
                "another command renewed the same sign-in",
                Kept {
                    lineage: vec![fingerprint("rvt_first")],
                    ..later("rvt_second")
                },
                false,
            ),
            (
                "a login signed in again, perhaps as another account or for another organization",
                later("rvt_stranger"),
                true,
            ),
        ] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("richview/credentials.json");
            keep_grant_in(&path, server, &ending).unwrap();
            let held = lock_in(&path).await.unwrap();
            let waiting = tokio::spawn({
                let path = path.clone();
                async move { usable_in(&path, server).await }
            });
            tokio::time::sleep(Duration::from_millis(250)).await;
            keep_grant_in(&path, server, &change).unwrap();
            drop(held);
            let got = waiting.await.unwrap();
            if stops {
                assert!(got.is_err(), "{label}: {got:?}");
            } else {
                assert_eq!(got.unwrap().as_deref(), Some("rvt_second"), "{label}");
            }
        }
    }

    #[tokio::test]
    async fn a_command_refused_by_one_sign_in_is_never_given_another_one_signed_in_since() {
        let server = "http://127.0.0.1:9";
        let renewable = |token: &str| Kept::new(token, Some("rvr_refresh"), Some(3600), None);
        // A command refused with `rvt_first` waits for the lock while something else changes the
        // sign-in kept; what it is given afterwards depends on what changed it.
        for (label, change, given) in [
            (
                "another command renewed the same sign-in",
                Kept {
                    lineage: vec![fingerprint("rvt_first")],
                    ..renewable("rvt_second")
                },
                Some("rvt_second"),
            ),
            (
                "a login signed in again, perhaps as another account or for another organization",
                renewable("rvt_stranger"),
                None,
            ),
        ] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("richview/credentials.json");
            keep_grant_in(&path, server, &renewable("rvt_first")).unwrap();
            let held = lock_in(&path).await.unwrap();
            let waiting = tokio::spawn({
                let path = path.clone();
                async move { renew_refused_in(&path, server, "rvt_first").await }
            });
            tokio::time::sleep(Duration::from_millis(250)).await;
            keep_grant_in(&path, server, &change).unwrap();
            drop(held);
            let got = waiting.await.unwrap().unwrap();
            assert_eq!(got.as_deref(), given, "{label}");
        }
    }

    #[test]
    fn a_renewal_remembers_the_tokens_it_replaced_up_to_a_bound() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("richview/credentials.json");
        let server = "https://a.example.test";
        let mut kept = Kept::new("rvt_0", Some("rvr_0"), Some(3600), Some("Ada"));
        keep_grant_in(&path, server, &kept).unwrap();
        for turn in 1..=(MAX_LINEAGE + 3) {
            let next = crate::sign_in::Renewed {
                token: format!("rvt_{turn}"),
                account: None,
                kept: Kept::new(&format!("rvt_{turn}"), Some("rvr_x"), Some(3600), None),
            };
            keep_renewed(&path, server, kept, next).unwrap();
            kept = kept_grant_in(&path, server).unwrap().unwrap();
        }
        // The account is the sign-in's own, the latest token is the last and the oldest are gone.
        assert_eq!(kept.account.as_deref(), Some("Ada"));
        assert_eq!(kept.token, format!("rvt_{}", MAX_LINEAGE + 3));
        assert_eq!(kept.lineage.len(), MAX_LINEAGE);
        assert!(!kept.lineage.contains(&fingerprint("rvt_0")));
        assert!(
            kept.lineage
                .contains(&fingerprint(&format!("rvt_{}", MAX_LINEAGE + 2)))
        );
    }

    #[test]
    fn signing_out_takes_the_sign_in_that_is_kept_when_it_gets_the_lock_not_the_one_it_saw_before()
    {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("richview/credentials.json");
        let server = "https://a.example.test";
        keep_grant_in(
            &path,
            server,
            &Kept::new("rvt_old", Some("rvr_old"), Some(3600), None),
        )
        .unwrap();
        // A renewal holds the lock and replaces the tokens; the sign-out waits for it.
        let renewing = lock_blocking_in(&path, Duration::from_secs(1)).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let out = path.clone();
        let signing_out = std::thread::spawn(move || {
            sender.send(()).unwrap();
            forget_locked(&out, server, Duration::from_secs(10))
        });
        receiver.recv().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        keep_grant_in(
            &path,
            server,
            &Kept::new("rvt_new", Some("rvr_new"), Some(3600), None),
        )
        .unwrap();
        drop(renewing);
        let (was, taken) = signing_out.join().unwrap().unwrap();
        assert!(was);
        let taken = taken.expect("the sign-in kept at the time was not returned");
        // It is the renewal's tokens that are to be revoked, not the ones that were replaced.
        assert_eq!(taken.token, "rvt_new");
        assert_eq!(taken.refresh.as_deref(), Some("rvr_new"));
        assert!(kept_in(&path, server).unwrap().is_none());
    }
}
