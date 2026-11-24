//! Bounded, private storage for one-use hosted email links and their rate history.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

// A legacy store may already contain 5,000 anonymous rows. Keep those links and still reserve
// half the expanded bound for existing-account mail, without invalidating a live link.
const MAX_LINKS: usize = 10_000;
const ANONYMOUS_LINKS_LIMIT: usize = MAX_LINKS / 2;
const HOUR: u64 = 3_600;
const DAY: u64 = 24 * HOUR;
const CLEANUP_INTERVAL: u64 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkPurpose {
    Signup,
    Reset,
    Invite,
}

impl LinkPurpose {
    /// How long a mail link works. Sign-up, invitation and share mails may be read hours after
    /// they are sent, so their links last a day; a password reset keeps one hour.
    fn lifetime(self) -> u64 {
        match self {
            Self::Signup | Self::Invite => DAY,
            Self::Reset => HOUR,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Link {
    pub sha256: String,
    pub purpose: LinkPurpose,
    pub email: String,
    pub created: u64,
    pub expires: u64,
    #[serde(default)]
    pub used: bool,
    /// The invitation an `invite` link accepts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invitation: Option<String>,
}

pub(crate) struct Links {
    path: PathBuf,
    records: Arc<Mutex<Vec<Link>>>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn digest(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
fn valid_token(token: &str) -> bool {
    token.len() == 43
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn remove_staging(dir: &Path, legacy: bool) -> Result<(), String> {
    let mut removed = false;
    for item in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = item.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".links-") && !(legacy && name.starts_with(".tmp")) {
            continue;
        }
        if !fs::symlink_metadata(entry.path())
            .map_err(|e| e.to_string())?
            .file_type()
            .is_file()
        {
            return Err("Invalid link staging file".into());
        }
        fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        removed = true;
    }
    if removed {
        crate::access::sync_directory(dir)?;
    }
    Ok(())
}

impl Links {
    /// Remove all link and rate-history records for a deleted account.
    pub(crate) fn purge_address(&self, email: &str) -> Result<(), String> {
        let mut records = self.records.lock().map_err(|_| "Links lock poisoned")?;
        let mut next = records.clone();
        next.retain(|record| !record.email.eq_ignore_ascii_case(email));
        if next.len() != records.len() {
            self.save(&next)?;
            *records = next;
        }
        Ok(())
    }
    pub fn open(dir: &Path) -> Result<Self, String> {
        // Access holds the exclusive hosted lock before opening this store.
        remove_staging(dir, true)?;
        let path = dir.join("links.json");
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if !meta.file_type().is_file() {
                return Err("The links file must be a regular file".into());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o077 != 0 {
                    return Err("The links file must be private to the server user".into());
                }
            }
        }
        let records: Vec<Link> = match fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("Invalid links file: {e}"))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.to_string()),
        };
        if records.len() > MAX_LINKS {
            return Err("Link file exceeds its limit".into());
        }
        let links = Self {
            path,
            records: Arc::new(Mutex::new(records)),
        };
        links.cleanup(now())?;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            links.start_cleanup(&runtime, std::time::Duration::from_secs(CLEANUP_INTERVAL));
        }
        Ok(links)
    }

    fn start_cleanup(&self, runtime: &tokio::runtime::Handle, interval: std::time::Duration) {
        // Weak ownership lets a closed store and its timer go away together.
        let records = Arc::downgrade(&self.records);
        let path = self.path.clone();
        runtime.spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let Some(records) = records.upgrade() else {
                    break;
                };
                let links = Self {
                    path: path.clone(),
                    records,
                };
                if links.cleanup(now()).is_err() {
                    eprintln!("richview: hosted link cleanup failed");
                }
            }
        });
    }

    // Keep the 24-hour address rate history, then remove the expired link record.
    fn cleanup(&self, at: u64) -> Result<(), String> {
        let mut records = self.records.lock().map_err(|_| "Links lock poisoned")?;
        remove_staging(self.path.parent().ok_or("Missing link directory")?, false)?;
        let mut next = records.clone();
        next.retain(|r| r.created > at.saturating_sub(DAY) || (!r.used && r.expires > at));
        if next.len() != records.len() {
            self.save(&next)?;
            *records = next;
        }
        Ok(())
    }

    fn save(&self, records: &[Link]) -> Result<(), String> {
        let parent = self.path.parent().ok_or("Missing link directory")?;
        let mut file = tempfile::Builder::new()
            .prefix(".links-")
            .tempfile_in(parent)
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        serde_json::to_writer(&mut file, records).map_err(|e| e.to_string())?;
        file.flush()
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist(&self.path).map_err(|e| e.error.to_string())?;
        crate::access::sync_directory(parent)?;
        Ok(())
    }

    pub fn inspect(&self, token: &str) -> Option<LinkPurpose> {
        if !valid_token(token) {
            return None;
        }
        let wanted = digest(token);
        let records = self.records.lock().ok()?;
        records
            .iter()
            .find(|link| link.sha256 == wanted && !link.used && link.expires > now())
            .map(|link| link.purpose)
    }

    pub fn pending(&self) -> Result<Vec<serde_json::Value>, String> {
        let records = self.records.lock().map_err(|_| "Links lock poisoned")?;
        let at = now();
        Ok(records.iter().filter(|link| !link.used && link.expires > at).map(|link| {
            serde_json::json!({"email":link.email, "purpose":link.purpose, "created":link.created, "expires":link.expires})
        }).collect())
    }

    /// The live link behind a token, without spending it.
    pub fn peek(&self, token: &str) -> Option<Link> {
        if !valid_token(token) {
            return None;
        }
        let wanted = digest(token);
        let records = self.records.lock().ok()?;
        records
            .iter()
            .find(|link| link.sha256 == wanted && !link.used && link.expires > now())
            .cloned()
    }

    pub fn issue(&self, email: &str, purpose: LinkPurpose) -> Result<Option<String>, String> {
        self.issue_for(email, purpose, None, purpose == LinkPurpose::Signup)
    }

    /// Reserve only the anonymous share of history for a public signup or unknown reset.
    pub fn issue_anonymous(
        &self,
        email: &str,
        purpose: LinkPurpose,
    ) -> Result<Option<String>, String> {
        self.issue_for(email, purpose, None, true)
    }

    /// An invitation link counts against the address's mail limits like any other link.
    pub fn issue_invite(&self, email: &str, invitation: &str) -> Result<Option<String>, String> {
        self.issue_for(
            email,
            LinkPurpose::Invite,
            Some(invitation.to_owned()),
            false,
        )
    }

    fn issue_for(
        &self,
        email: &str,
        purpose: LinkPurpose,
        invitation: Option<String>,
        anonymous: bool,
    ) -> Result<Option<String>, String> {
        let Some(email) = crate::email::normalize(email) else {
            return Ok(None);
        };
        let mut records = self.records.lock().map_err(|_| "Links lock poisoned")?;
        let at = now();
        let mut next = records.clone();
        next.retain(|r| r.created > at.saturating_sub(DAY) || (!r.used && r.expires > at));
        let last_day = next
            .iter()
            .filter(|r| r.email == email && r.created > at.saturating_sub(DAY))
            .count();
        let last_hour = next
            .iter()
            .filter(|r| r.email == email && r.created > at.saturating_sub(HOUR))
            .count();
        let history_limit = if anonymous {
            ANONYMOUS_LINKS_LIMIT
        } else {
            MAX_LINKS
        };
        if next.len() >= history_limit || last_day >= 10 || last_hour >= 3 {
            return Ok(None);
        }
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
        let token =
            base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes);
        next.push(Link {
            sha256: digest(&token),
            purpose,
            email,
            created: at,
            // An invitation itself stays open for a week and can be sent again.
            expires: at + purpose.lifetime(),
            used: false,
            invitation,
        });
        self.save(&next)?;
        *records = next;
        Ok(Some(token))
    }

    /// Spend the link before changing account state; a failed account write needs a fresh link.
    pub fn take(&self, token: &str) -> Result<Option<Link>, String> {
        if !valid_token(token) {
            return Ok(None);
        }
        let wanted = digest(token);
        let mut records = self.records.lock().map_err(|_| "Links lock poisoned")?;
        let mut next = records.clone();
        let Some(link) = next
            .iter_mut()
            .find(|r| r.sha256 == wanted && !r.used && r.expires > now())
        else {
            return Ok(None);
        };
        link.used = true;
        let spent = link.clone();
        self.save(&next)?;
        *records = next;
        Ok(Some(spent))
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;

    #[test]
    fn hosted_hotfix_operator_runbook_matches_links_and_callback_redaction() {
        let runbook = include_str!("../../../docs/hosted-operator.md");
        for (purpose, label) in [
            (LinkPurpose::Signup, "Sign-up"),
            (LinkPurpose::Invite, "Invitation"),
            (LinkPurpose::Reset, "Password reset"),
        ] {
            let hours = purpose.lifetime() / HOUR;
            let unit = if hours == 1 { "hour" } else { "hours" };
            assert!(
                runbook.contains(&format!("| {label} | {hours} {unit} |")),
                "{label}"
            );
        }
        let proxy = runbook
            .lines()
            .find(|line| line.contains("X-Real-IP"))
            .unwrap();
        assert!(proxy.contains("query strings on `/sign-in/{provider}/callback`"));
    }

    #[test]
    fn startup_removes_abandoned_legacy_and_link_staging() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".tmplegacy"), b"private address").unwrap();
        fs::write(dir.path().join(".links-abandoned"), b"private address").unwrap();
        fs::write(dir.path().join("unrelated"), b"keep").unwrap();
        let _links = Links::open(dir.path()).unwrap();
        assert!(!dir.path().join(".tmplegacy").exists());
        assert!(!dir.path().join(".links-abandoned").exists());
        assert!(dir.path().join("unrelated").exists());
    }

    #[tokio::test]
    async fn timer_removes_history_without_another_request() {
        let dir = tempfile::tempdir().unwrap();
        let links = Links::open(dir.path()).unwrap();
        let at = now();
        {
            let mut records = links.records.lock().unwrap();
            records.push(Link {
                sha256: "old".into(),
                purpose: LinkPurpose::Signup,
                email: "person@example.test".into(),
                created: at - DAY - 1,
                expires: at - 1,
                used: false,
                invitation: None,
            });
            links.save(&records).unwrap();
        }
        links.start_cleanup(
            &tokio::runtime::Handle::current(),
            std::time::Duration::from_millis(10),
        );
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !links.records.lock().unwrap().is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let stored: Vec<Link> =
            serde_json::from_slice(&fs::read(dir.path().join("links.json")).unwrap()).unwrap();
        assert!(stored.is_empty());
    }

    #[test]
    fn a_reset_link_lasts_an_hour_and_the_mails_read_later_last_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let links = Links::open(dir.path()).unwrap();
        links
            .issue("new@example.test", LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        links
            .issue("known@example.test", LinkPurpose::Reset)
            .unwrap()
            .unwrap();
        links
            .issue_invite("guest@example.test", "invitation-1")
            .unwrap()
            .unwrap();
        let lifetimes: Vec<(LinkPurpose, u64)> = links
            .records
            .lock()
            .unwrap()
            .iter()
            .map(|link| (link.purpose, link.expires - link.created))
            .collect();
        assert_eq!(
            lifetimes,
            [
                (LinkPurpose::Signup, DAY),
                (LinkPurpose::Reset, HOUR),
                (LinkPurpose::Invite, DAY)
            ]
        );
    }

    #[test]
    fn startup_and_sweep_remove_old_records_but_keep_rate_history() {
        let dir = tempfile::tempdir().unwrap();
        let at = now();
        let old = Link {
            sha256: "old".into(),
            purpose: LinkPurpose::Signup,
            email: "person@example.test".into(),
            created: at - DAY - 1,
            expires: at - DAY + HOUR,
            used: false,
            invitation: None,
        };
        let recent = Link {
            sha256: "recent".into(),
            created: at - HOUR - 1,
            expires: at - 1,
            ..old.clone()
        };
        let path = dir.path().join("links.json");
        fs::write(&path, serde_json::to_vec(&vec![old, recent]).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let links = Links::open(dir.path()).unwrap();
        let stored: Vec<Link> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].sha256, "recent");
        links.cleanup(at + DAY).unwrap();
        let stored: Vec<Link> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(stored.is_empty());
    }
}
