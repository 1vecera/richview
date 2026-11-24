//! The sign-ins this computer keeps: one token per server, in a private file beside the publish
//! choice. A kept token is read only for the server it was kept for, so it is sent nowhere else
//! (FEA-SF-12, INT-79), and nothing here prints or logs one (ARC-23).
use crate::ops::Failure;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

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
    /// Who the server said the token belongs to: a label for the person who opens this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    kept: String,
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

fn kept_in(path: &Path, server: &str) -> Result<Option<String>, Failure> {
    let key = key(server)?;
    Ok(read(path, true)?
        .servers
        .remove(&key)
        .map(|entry| entry.token))
}

fn keep_in(path: &Path, server: &str, token: &str, account: Option<&str>) -> Result<(), Failure> {
    let key = key(server)?;
    // A file this cannot read is replaced: the person is signing in again.
    let mut store = read(path, false).unwrap_or_default();
    store.servers.insert(
        key,
        Entry {
            token: token.to_owned(),
            account: account.map(str::to_owned),
            kept: crate::time::Instant::now().seconds(),
        },
    );
    crate::publish_destination::save_private(path, &store)
}

fn forget_in(path: &Path, server: &str) -> Result<bool, Failure> {
    let key = key(server)?;
    let mut store = read(path, false)?;
    if store.servers.remove(&key).is_none() {
        return Ok(false);
    }
    if store.servers.is_empty() {
        std::fs::remove_file(path)
            .map_err(|e| Failure::host(format!("Cannot remove {}: {e}", path.display())))?;
    } else {
        crate::publish_destination::save_private(path, &store)?;
    }
    Ok(true)
}

/// The token this computer keeps for exactly `server`, if it is signed in there.
pub(crate) fn kept(server: &str) -> Result<Option<String>, Failure> {
    kept_in(&file()?, server)
}

/// Keeps `token` for `server`, replacing an earlier sign-in to that server.
pub(crate) fn keep(server: &str, token: &str, account: Option<&str>) -> Result<(), Failure> {
    keep_in(&file()?, server, token, account)
}

/// Forgets the sign-in kept for `server`; says whether there was one.
pub(crate) fn forget(server: &str) -> Result<bool, Failure> {
    forget_in(&file()?, server)
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
        assert!(forget_in(&path, "https://a.example.test").unwrap());
        assert!(!forget_in(&path, "https://a.example.test").unwrap());
        assert!(kept_in(&path, "https://a.example.test").unwrap().is_none());
        assert_eq!(
            kept_in(&path, "https://b.example.test").unwrap().as_deref(),
            Some("other")
        );
        // The last sign-in takes the file with it.
        assert!(forget_in(&path, "https://b.example.test").unwrap());
        assert!(!path.exists());
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
}
