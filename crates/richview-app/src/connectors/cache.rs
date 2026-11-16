use super::spec::{
    CacheRecord, ConnectorError, DatasetSnapshot, ProvenanceSource, SQL_METADATA_POLICY_VERSION,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Freshness bounds apply to capture time; `always_refresh` requests materialization again.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CachePolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh_until: Option<String>,
    #[serde(default)]
    pub always_refresh: bool,
}

impl CachePolicy {
    pub(super) fn validate(&self) -> Result<(), ConnectorError> {
        if let Some(key) = &self.key {
            validate_key(key)?;
        }
        if self
            .fresh_until
            .as_deref()
            .is_some_and(|value| parse_rfc3339(value).is_none())
        {
            return Err(rejected(
                "cache freshness must be a valid RFC3339 timestamp with a timezone",
            ));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Alias {
    fingerprint: String,
    #[serde(default)]
    ttl_seconds: Option<u64>,
    #[serde(default)]
    fresh_until: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredSnapshot<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sql_policy: Option<u8>,
    #[serde(flatten)]
    snapshot: T,
}

/// Store snapshots by SHA-256 identity and named references by a hash of the key.
/// The host scopes the root to its report; source bindings further isolate query identities.
pub struct SnapshotStore {
    root: PathBuf,
}

impl SnapshotStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn binding_identity(&self, source: &str) -> Result<String, ConnectorError> {
        let path = self.root.join("binding-key");
        if !path.exists() {
            fs::create_dir_all(&self.root)?;
            let mut key = [0; 32];
            getrandom::fill(&mut key)
                .map_err(|_| rejected("cannot initialize server snapshot identity"))?;
            let mut temporary = tempfile::NamedTempFile::new_in(&self.root)?;
            temporary.write_all(&key)?;
            temporary.as_file().sync_all()?;
            match temporary.persist_noclobber(&path) {
                Ok(_) => {}
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(ConnectorError::Io(error.error)),
            }
        }
        let mut key = Vec::new();
        fs::File::open(path)?.take(33).read_to_end(&mut key)?;
        if key.len() != 32 {
            return Err(rejected(
                "server snapshot identity is invalid; restore its store or clear cached snapshots",
            ));
        }
        // Runtime bindings can contain credentials. This private per-store key keeps their
        // public query fingerprints from becoming an offline credential-guessing oracle.
        let mut hash = Sha256::new();
        hash.update(b"snapshot-binding-v1\0");
        hash.update(key);
        hash.update(source.as_bytes());
        Ok(format!("{:x}", hash.finalize()))
    }

    fn path_for(&self, fingerprint: &str) -> Result<PathBuf, ConnectorError> {
        if !is_fingerprint(fingerprint) {
            return Err(rejected("snapshot identity must be a SHA-256 digest"));
        }
        Ok(self.root.join(format!("{fingerprint}.json")))
    }

    fn key_path(&self, key: &str) -> Result<PathBuf, ConnectorError> {
        validate_key(key)?;
        Ok(self
            .root
            .join(format!("key-{:x}.json", Sha256::digest(key.as_bytes()))))
    }

    pub fn load(&self, fingerprint: &str) -> Result<Option<DatasetSnapshot>, ConnectorError> {
        let Some(stored): Option<StoredSnapshot<DatasetSnapshot>> =
            read_json(&self.path_for(fingerprint)?, crate::MAX_INPUT_BYTES)?
        else {
            return Ok(None);
        };
        if stored.snapshot.provenance.source == ProvenanceSource::Sql
            && stored.sql_policy != Some(SQL_METADATA_POLICY_VERSION)
        {
            return Err(rejected(
                "SQL snapshot predates host-metadata isolation; refresh the query from its source",
            ));
        }
        let snapshot = stored.snapshot;
        if snapshot.rows.len() > super::spec::MAX_ROWS
            || snapshot.provenance.fingerprint != fingerprint
            || snapshot.provenance.row_count != snapshot.rows.len()
            || parse_rfc3339(&snapshot.provenance.fetched_at).is_none()
        {
            return Err(invalid_snapshot());
        }
        Ok(Some(snapshot))
    }

    pub(super) fn load_key(&self, key: &str) -> Result<Option<DatasetSnapshot>, ConnectorError> {
        if let Some(alias) = read_json::<Alias>(&self.key_path(key)?, 1024)? {
            let policy = CachePolicy {
                key: Some(key.to_owned()),
                ttl_seconds: alias.ttl_seconds,
                fresh_until: alias.fresh_until,
                always_refresh: false,
            };
            policy.validate()?;
            return self
                .load(&alias.fingerprint)?
                .map(|snapshot| annotate(snapshot, Some(&policy), key))
                .transpose();
        }
        if is_fingerprint(key) {
            self.load(key)
        } else {
            Ok(None)
        }
    }

    pub(super) fn save_key(
        &self,
        key: &str,
        fingerprint: &str,
        policy: Option<&CachePolicy>,
    ) -> Result<(), ConnectorError> {
        self.path_for(fingerprint)?;
        write_json(
            &self.key_path(key)?,
            &Alias {
                fingerprint: fingerprint.to_owned(),
                ttl_seconds: policy.and_then(|p| p.ttl_seconds),
                fresh_until: policy.and_then(|p| p.fresh_until.clone()),
            },
        )
    }

    pub fn save(
        &self,
        fingerprint: &str,
        snapshot: &DatasetSnapshot,
    ) -> Result<(), ConnectorError> {
        if snapshot.rows.len() > super::spec::MAX_ROWS
            || snapshot.provenance.fingerprint != fingerprint
            || snapshot.provenance.row_count != snapshot.rows.len()
            || parse_rfc3339(&snapshot.provenance.fetched_at).is_none()
        {
            return Err(invalid_snapshot());
        }
        write_json(
            &self.path_for(fingerprint)?,
            &StoredSnapshot {
                sql_policy: (snapshot.provenance.source == ProvenanceSource::Sql)
                    .then_some(SQL_METADATA_POLICY_VERSION),
                snapshot,
            },
        )
    }
}

fn is_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn validate_key(key: &str) -> Result<(), ConnectorError> {
    if key.trim().is_empty() || key.len() > 256 || key.chars().any(char::is_control) {
        Err(rejected(
            "snapshot key must be nonempty text of at most 256 bytes without control characters",
        ))
    } else {
        Ok(())
    }
}

fn rejected(reason: &str) -> ConnectorError {
    ConnectorError::QueryRejected {
        reason: reason.into(),
    }
}
fn invalid_snapshot() -> ConnectorError {
    ConnectorError::Execution {
        connector: "cache".into(),
        message: "stored snapshot is invalid or exceeds its byte limit; refresh its source".into(),
    }
}

fn read_json<T: DeserializeOwned>(path: &Path, limit: usize) -> Result<Option<T>, ConnectorError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid_snapshot());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| invalid_snapshot())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), ConnectorError> {
    struct Bounded<'a> {
        file: &'a mut fs::File,
        written: usize,
    }
    impl Write for Bounded<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > crate::MAX_INPUT_BYTES.saturating_sub(self.written) {
                return Err(std::io::Error::other("snapshot exceeds byte limit"));
            }
            let count = self.file.write(bytes)?;
            self.written += count;
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.file.flush()
        }
    }
    let root = path.parent().ok_or_else(invalid_snapshot)?;
    fs::create_dir_all(root)?;
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(
        Bounded {
            file: temporary.as_file_mut(),
            written: 0,
        },
        value,
    )
    .map_err(|_| invalid_snapshot())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| ConnectorError::Io(error.error))?;
    Ok(())
}

pub(super) fn annotate(
    mut snapshot: DatasetSnapshot,
    policy: Option<&CachePolicy>,
    key: &str,
) -> Result<DatasetSnapshot, ConnectorError> {
    if let Some(policy) = policy {
        policy.validate()?;
    }
    let record = snapshot.cache.get_or_insert_with(|| CacheRecord {
        key: key.to_owned(),
        materialized_at: snapshot.provenance.fetched_at.clone(),
        ttl_seconds: None,
        fresh_until: None,
        binding: None,
        stale: false,
    });
    record.key = key.to_owned();
    record.materialized_at = snapshot.provenance.fetched_at.clone();
    if let Some(policy) = policy {
        record.ttl_seconds = match (record.ttl_seconds, policy.ttl_seconds) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if let Some(deadline) = &policy.fresh_until
            && record
                .fresh_until
                .as_deref()
                .is_none_or(|stored| parse_rfc3339(deadline) < parse_rfc3339(stored))
        {
            record.fresh_until = Some(deadline.clone());
        }
    }
    let stale = is_stale(&snapshot)?;
    snapshot
        .cache
        .as_mut()
        .expect("cache record inserted")
        .stale = stale;
    Ok(snapshot)
}

fn is_stale(snapshot: &DatasetSnapshot) -> Result<bool, ConnectorError> {
    let captured = parse_rfc3339(&snapshot.provenance.fetched_at).ok_or_else(invalid_snapshot)?;
    let now = chrono::Utc::now();
    if captured > now {
        return Ok(true);
    }
    let Some(record) = &snapshot.cache else {
        return Ok(false);
    };
    if record
        .ttl_seconds
        .is_some_and(|ttl| now.signed_duration_since(captured).num_seconds() as u64 >= ttl)
    {
        return Ok(true);
    }
    if let Some(deadline) = &record.fresh_until {
        return Ok(now >= parse_rfc3339(deadline).ok_or_else(invalid_snapshot)?);
    }
    Ok(false)
}

fn parse_rfc3339(input: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(input).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_round_trips_a_known_instant() {
        let unix = parse_rfc3339("2024-01-02T03:04:05Z").unwrap();
        assert_eq!(unix.timestamp(), 1_704_164_645);
        for same_instant in ["2024-01-02T05:04:05+02:00", "2024-01-01T22:04:05-05:00"] {
            assert_eq!(parse_rfc3339(same_instant), Some(unix));
        }
        for invalid in ["2024-13-02T03:04:05Z", "2024-02-31T03:04:05Z", "not-a-time"] {
            assert_eq!(parse_rfc3339(invalid), None);
        }
    }

    #[test]
    fn policy_marks_snapshot_stale_past_ttl() {
        let snapshot = DatasetSnapshot {
            rows: vec![],
            schema: vec![],
            provenance: crate::connectors::spec::Provenance {
                connector: "x".into(),
                source: crate::connectors::spec::ProvenanceSource::Sql,
                fetched_at: "2000-01-01T00:00:00Z".into(),
                fingerprint: "deadbeef".into(),
                row_count: 0,
            },
            cache: None,
        };
        let policy = CachePolicy {
            ttl_seconds: Some(60),
            ..Default::default()
        };
        assert!(
            annotate(snapshot, Some(&policy), "x")
                .unwrap()
                .cache
                .unwrap()
                .stale
        );
    }

    #[test]
    fn stored_snapshots_are_bounded_atomic_private_and_confined_to_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("snapshots");
        let store = SnapshotStore::new(root.clone());
        let fingerprint = "a".repeat(64);
        let mut snapshot = DatasetSnapshot {
            rows: vec![serde_json::json!({"n":7})],
            schema: vec![],
            provenance: crate::connectors::spec::Provenance {
                connector: "local".into(),
                source: crate::connectors::spec::ProvenanceSource::Sql,
                fetched_at: "2024-01-02T03:04:05Z".into(),
                fingerprint: fingerprint.clone(),
                row_count: 1,
            },
            cache: None,
        };
        store.save(&fingerprint, &snapshot).unwrap();
        store.save_key("../outside", &fingerprint, None).unwrap();
        assert_eq!(
            store.load_key("../outside").unwrap().unwrap().rows,
            snapshot.rows
        );
        assert!(!directory.path().join("outside").exists());
        assert!(store.load("../outside").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(root.join(format!("{fingerprint}.json")))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }
        snapshot.rows = vec![serde_json::json!({"n":7}); super::super::spec::MAX_ROWS + 1];
        snapshot.provenance.row_count = snapshot.rows.len();
        assert!(store.save(&fingerprint, &snapshot).is_err());
        snapshot.provenance.row_count = 1;
        snapshot.rows = vec![
            serde_json::json!({"n": "private-sentinel".repeat(crate::MAX_INPUT_BYTES / 16 + 1)}),
        ];
        let error = store.save(&fingerprint, &snapshot).unwrap_err().to_string();
        assert!(!error.contains("private-sentinel"));
        assert_eq!(
            store.load(&fingerprint).unwrap().unwrap().rows,
            [serde_json::json!({"n":7})]
        );
        let path = root.join(format!("{fingerprint}.json"));
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(crate::MAX_INPUT_BYTES as u64 + 1)
            .unwrap();
        assert!(store.load(&fingerprint).is_err());
        fs::write(path, "{\"private-sentinel\":invalid}").unwrap();
        assert!(
            !store
                .load(&fingerprint)
                .unwrap_err()
                .to_string()
                .contains("private-sentinel")
        );
    }

    #[test]
    fn legacy_sql_captures_require_refresh_even_when_the_source_is_offline() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(dir.path().to_path_buf());
        let fingerprint = "b".repeat(64);
        let mut snapshot = DatasetSnapshot {
            rows: vec![serde_json::json!({"n":7})],
            schema: vec![],
            cache: None,
            provenance: super::super::spec::Provenance {
                connector: "offline".into(),
                source: ProvenanceSource::Sql,
                fetched_at: "2024-01-02T03:04:05Z".into(),
                fingerprint: fingerprint.clone(),
                row_count: 1,
            },
        };
        fs::write(
            store.path_for(&fingerprint).unwrap(),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();
        store.save_key("offline", &fingerprint, None).unwrap();
        let error = store.load_key("offline").unwrap_err().to_string();
        assert!(
            error.contains("predates host-metadata isolation"),
            "{error}"
        );
        // Existing non-SQL captures retain their original compatibility contract.
        snapshot.provenance.source = ProvenanceSource::Metricflow;
        fs::write(
            store.path_for(&fingerprint).unwrap(),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();
        assert_eq!(
            store.load_key("offline").unwrap().unwrap().rows,
            snapshot.rows
        );
        snapshot.provenance.source = ProvenanceSource::Sql;
        store.save(&fingerprint, &snapshot).unwrap();
        assert_eq!(
            store.load_key("offline").unwrap().unwrap().rows,
            snapshot.rows
        );
    }

    #[test]
    fn named_snapshot_freshness_cannot_extend_the_saved_lifetime() {
        let directory = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(directory.path().to_path_buf());
        let fingerprint = "b".repeat(64);
        let snapshot = DatasetSnapshot {
            rows: vec![],
            schema: vec![],
            provenance: crate::connectors::spec::Provenance {
                connector: "local".into(),
                source: crate::connectors::spec::ProvenanceSource::Sql,
                fetched_at: "2024-01-02T03:04:05Z".into(),
                fingerprint: fingerprint.clone(),
                row_count: 0,
            },
            cache: None,
        };
        let original = CachePolicy {
            ttl_seconds: Some(60),
            ..Default::default()
        };
        let snapshot = annotate(snapshot, Some(&original), "monthly").unwrap();
        store.save(&fingerprint, &snapshot).unwrap();
        let extended = CachePolicy {
            ttl_seconds: Some(u64::MAX),
            ..Default::default()
        };
        store
            .save_key("monthly", &fingerprint, Some(&extended))
            .unwrap();
        let snapshot = store.load_key("monthly").unwrap().unwrap();
        assert_eq!(snapshot.cache.as_ref().unwrap().ttl_seconds, Some(60));
        assert!(snapshot.cache.as_ref().unwrap().stale);
        assert!(
            CachePolicy {
                fresh_until: Some("2024-02-31T00:00:00Z".into()),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn trusted_binding_identities_are_stable_and_private_to_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let first = SnapshotStore::new(directory.path().join("first"));
        let identity = first
            .binding_identity("trusted-credential-sentinel")
            .unwrap();
        assert_eq!(
            SnapshotStore::new(first.root().to_path_buf())
                .binding_identity("trusted-credential-sentinel")
                .unwrap(),
            identity
        );
        assert_ne!(
            SnapshotStore::new(directory.path().join("second"))
                .binding_identity("trusted-credential-sentinel")
                .unwrap(),
            identity
        );
        assert!(!identity.contains("trusted-credential-sentinel"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(first.root().join("binding-key"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }
    }
}
