//! Migration preserves the existing single-server roles and report data.
use richview_app::{
    access::{Access, OrgRole, Principal, Right, Visibility},
    storage::{FsStorage, StoredReport},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::current_dir().unwrap().join("tmp").join(format!(
            "schema2-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join(".access")).unwrap();
        Self(root)
    }
}

#[test]
fn migration_refuses_to_overwrite_a_different_backup() {
    let dir = Dir::new();
    let old = br#"{"users":[],"sessions":[],"acls":{}}"#;
    std::fs::write(dir.0.join(".access/state.json"), old).unwrap();
    std::fs::write(dir.0.join(".access/state.v1.json"), b"different").unwrap();
    let error = match Access::open(&dir.0, Visibility::Private, None) {
        Ok(_) => panic!("must refuse mismatched backup"),
        Err(error) => error,
    };
    assert!(error.contains("backup"));
    assert_eq!(
        std::fs::read(dir.0.join(".access/state.json")).unwrap(),
        old
    );
    assert_eq!(
        std::fs::read(dir.0.join(".access/state.v1.json")).unwrap(),
        b"different"
    );
}

#[test]
fn migration_recovers_an_interrupted_backup_and_ignores_orphan_staging() {
    let dir = Dir::new();
    let old = br#"{"users":[],"sessions":[],"acls":{}}"#;
    let access_dir = dir.0.join(".access");
    std::fs::write(access_dir.join("state.json"), old).unwrap();
    std::fs::write(access_dir.join("state.v1.json"), &old[..10]).unwrap();
    std::fs::write(access_dir.join(".orphaned-stage"), &old[..5]).unwrap();
    let access = Access::open(&dir.0, Visibility::Private, None).unwrap();
    assert_eq!(
        std::fs::read(access_dir.join("state.v1.json")).unwrap(),
        old
    );
    let preserved: Vec<_> = std::fs::read_dir(&access_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("state.v1.partial-")
        })
        .collect();
    assert_eq!(preserved.len(), 1);
    assert_eq!(std::fs::read(&preserved[0]).unwrap(), &old[..10]);
    let migrated: serde_json::Value =
        serde_json::from_slice(&std::fs::read(access_dir.join("state.json")).unwrap()).unwrap();
    assert_eq!(migrated["schema"], 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(access_dir.join("state.v1.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    drop(access);
    Access::open(&dir.0, Visibility::Private, None).unwrap();
}

#[test]
fn migration_preserves_a_genuine_different_json_backup() {
    let dir = Dir::new();
    let original = br#"{"users":[],"sessions":[],"acls":{}}"#;
    let backup = br#"{"users":[1],"sessions":[],"acls":{}}"#;
    std::fs::write(dir.0.join(".access/state.json"), original).unwrap();
    std::fs::write(dir.0.join(".access/state.v1.json"), backup).unwrap();
    assert!(Access::open(&dir.0, Visibility::Private, None).is_err());
    assert_eq!(
        std::fs::read(dir.0.join(".access/state.json")).unwrap(),
        original
    );
    assert_eq!(
        std::fs::read(dir.0.join(".access/state.v1.json")).unwrap(),
        backup
    );
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn report(slug: &str, version: &str) -> StoredReport {
    StoredReport {
        slug: slug.into(),
        version: version.into(),
        source: "# hello".into(),
        report_folder: None,
        html: "<h1>hello</h1>".into(),
        frontmatter: json!({}),
        created_at: "2026-01-01".into(),
        styles: vec![],
        data: BTreeMap::new(),
        files: BTreeMap::new(),
        content_sha256: String::new(),
    }
}

#[test]
fn v1_migration_preserves_roles_grants_tokens_and_report_storage() {
    let dir = Dir::new();
    let token_hash = format!("{:x}", Sha256::digest(b"old-token"));
    let old = json!({
        "users": [
            {"id":"u-owner", "name":"Owner", "email":"owner@example.test", "role":"author", "disabled":false, "created":"2026-01-01", "password":null, "tokens":[{"id":"t-old", "name":"old", "sha256":token_hash, "created":"2026-01-01"}]},
            {"id":"u-reader", "name":"Reader", "email":"reader@example.test", "role":"reader", "disabled":false, "created":"2026-01-01", "password":null, "tokens":[]},
            {"id":"u-admin", "name":"Admin", "email":"admin@example.test", "role":"admin", "disabled":false, "created":"2026-01-01", "password":null, "tokens":[]}
        ],
        "sessions": [],
        "acls": {"secret": {"owner":"u-owner", "visibility":"private", "grants":[{"user":"u-reader", "role":"viewer"}]},
                  "service": {"owner":"service", "visibility":"org", "grants":[]}}
    });
    let before = serde_json::to_vec_pretty(&old).unwrap();
    std::fs::write(dir.0.join(".access/state.json"), &before).unwrap();
    let store = FsStorage::new(&dir.0).unwrap();
    store.put_version(report("secret", "1.0.0")).unwrap();
    store.put_version(report("secret", "2.0.0")).unwrap();
    assert!(store.set_pin("secret", "1.0.0").unwrap());
    store.delete_version("secret", "2.0.0").unwrap();
    store.put_version(report("service", "1.0.0")).unwrap();
    let access = Access::open(&dir.0, Visibility::Private, None).unwrap();
    assert_eq!(
        std::fs::read(dir.0.join(".access/state.v1.json")).unwrap(),
        before
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.0.join(".access/state.v1.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let state: Value =
        serde_json::from_slice(&std::fs::read(dir.0.join(".access/state.json")).unwrap()).unwrap();
    assert_eq!(state["schema"], 2);
    assert_eq!(state["tenancy"], "single");
    assert_eq!(state["organizations"][0]["id"], "default");
    assert_eq!(state["memberships"].as_array().unwrap().len(), 3);
    assert!(
        state["users"][0].get("role").is_none(),
        "roles belong to memberships"
    );
    assert_eq!(state["users"][0]["verified"], "2026-01-01");
    assert_eq!(state["users"][0]["tokens"][0]["org"], "default");
    assert_eq!(state["acls"]["secret"]["org"], "default");
    let owner = Principal::User(access.user("u-owner").unwrap());
    let reader = Principal::User(access.user("u-reader").unwrap());
    let admin = Principal::User(access.user("u-admin").unwrap());
    assert!(access.can(&owner, "secret", Right::Delete));
    assert!(
        matches!(access.authenticate(Some("old-token"), None), Principal::User(user) if user.id == "u-owner" && user.token_org.as_deref() == Some("default"))
    );
    assert!(access.can(&reader, "secret", Right::View));
    assert!(!access.can(&reader, "secret", Right::Edit));
    assert!(access.can(&admin, "secret", Right::Share));
    assert_eq!(access.user("u-owner").unwrap().role, OrgRole::Author);
    assert_eq!(
        store.latest_version("secret").unwrap().as_deref(),
        Some("1.0.0")
    );
    assert!(store.is_deleted("secret", "2.0.0").unwrap());
    assert_eq!(access.acl("service").owner, "service");
    access
        .update_user("u-reader", None, Some(OrgRole::Author), None)
        .unwrap();
    let reopened = Access::open(&dir.0, Visibility::Private, None).unwrap();
    assert!(reopened.can(&reader, "secret", Right::View));
    assert_eq!(
        reopened.role_in("u-reader", "default"),
        Some(OrgRole::Author)
    );
}

#[test]
fn hosted_and_single_data_directories_cannot_be_reinterpreted() {
    let hosted = Dir::new();
    // Dir's test helper creates an empty .access; hosted mode requires an actually empty root.
    std::fs::remove_dir(hosted.0.join(".access")).unwrap();
    assert!(Access::open_hosted(&hosted.0).unwrap().is_hosted());
    assert!(Access::open(&hosted.0, Visibility::Private, None).is_err());
    assert!(Access::open_hosted(&hosted.0).is_ok());
    let single = Dir::new();
    let access = Access::open(&single.0, Visibility::Private, None).unwrap();
    access
        .create_user("Admin", "admin@example.test", OrgRole::Admin, None)
        .unwrap();
    assert!(Access::open_hosted(&single.0).is_err());
    let dirty = Dir::new();
    std::fs::write(dirty.0.join("existing"), b"data").unwrap();
    assert!(Access::open_hosted(&dirty.0).is_err());
}

#[test]
fn a_hosted_store_has_only_one_active_access_owner() {
    let dir = Dir::new();
    std::fs::remove_dir(dir.0.join(".access")).unwrap();
    let first = Access::open_hosted(&dir.0).unwrap();
    let clone = first.clone();
    assert!(Access::open_hosted(&dir.0).is_err());
    drop(first);
    assert!(Access::open_hosted(&dir.0).is_err());
    drop(clone);
    assert!(Access::open_hosted(&dir.0).is_ok());
}

#[test]
fn hosted_storage_scopes_equal_slugs_and_enforces_independent_limits() {
    let dir = Dir::new();
    let store = FsStorage::new(&dir.0)
        .unwrap()
        .with_hosted(true)
        .with_hosted_limits(4096, 1);
    let first = report("aaaa0001/q3", "1.0.0");
    let second = report("bbbb0002/q3", "1.0.0");
    store.put_version(first.clone()).unwrap();
    store.put_version(second).unwrap();
    assert!(dir.0.join("orgs/aaaa0001/q3/1.0.0/source.rmd").exists());
    assert!(dir.0.join("orgs/bbbb0002/q3/1.0.0/source.rmd").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in ["orgs", "orgs/aaaa0001", "orgs/aaaa0001/q3"] {
            assert_eq!(
                std::fs::metadata(dir.0.join(path))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }
    store
        .put_share_card("aaaa0001/q3", "1.0.0", &[0; 4096])
        .unwrap();
    assert!(
        store.share_card("aaaa0001/q3", "1.0.0").unwrap().is_none(),
        "a generated card cannot exceed the organization byte limit"
    );
    assert_eq!(store.list().unwrap().len(), 2);
    assert!(matches!(
        store.put_version(report("aaaa0001/another", "1.0.0")),
        Err(richview_app::storage::PublishError::ReportLimit { .. })
    ));
    assert!(!dir.0.join("orgs/aaaa0001/another").exists());
    assert!(store.delete_version("aaaa0001/q3", "1.0.0").unwrap());
    store
        .put_version(report("aaaa0001/another", "1.0.0"))
        .unwrap();
    assert!(
        matches!(
            store.put_version(report("aaaa0001/q3", "2.0.0")),
            Err(richview_app::storage::PublishError::ReportLimit { .. })
        ),
        "reviving a report counts towards the limit"
    );
    let tiny = FsStorage::new(dir.0.join("tiny"))
        .unwrap()
        .with_hosted(true)
        .with_hosted_limits(1, 50);
    assert!(matches!(
        tiny.put_version(report("aaaa0001/refused", "1.0.0")),
        Err(richview_app::storage::PublishError::OrganizationQuota { .. })
    ));
    assert!(!dir.0.join("tiny/orgs/aaaa0001/refused").exists());
    assert!(store.put_version(report("../escaped", "1.0.0")).is_err());
}

#[test]
fn hosted_report_limit_is_serialized_for_concurrent_creates() {
    let dir = Dir::new();
    let store = FsStorage::new(&dir.0)
        .unwrap()
        .with_hosted(true)
        .with_hosted_limits(1_000_000, 1);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = ["race-a", "race-b"]
        .into_iter()
        .map(|slug| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.put_version(report(&format!("aaaa0001/{slug}"), "1.0.0"))
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(
                r,
                Err(richview_app::storage::PublishError::ReportLimit { .. })
            ))
            .count(),
        1
    );
    assert_eq!(store.list().unwrap().len(), 1);
}

#[test]
fn hosted_byte_limit_is_serialized_and_independent_per_organization() {
    let probe = Dir::new();
    let probe_store = FsStorage::new(&probe.0).unwrap().with_hosted(true);
    probe_store
        .put_version(report("aaaa0001/race-a", "1.0.0"))
        .unwrap();
    let size = std::fs::read_dir(probe.0.join("orgs/aaaa0001/race-a/1.0.0"))
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum::<u64>();
    let dir = Dir::new();
    let store = FsStorage::new(&dir.0)
        .unwrap()
        .with_hosted(true)
        .with_hosted_limits(size * 3 / 2, 50);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = ["race-a", "race-b"]
        .into_iter()
        .map(|slug| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.put_version(report(&format!("aaaa0001/{slug}"), "1.0.0"))
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(
                r,
                Err(richview_app::storage::PublishError::OrganizationQuota { .. })
            ))
            .count(),
        1
    );
    assert!(
        store
            .put_version(report("bbbb0002/race-a", "1.0.0"))
            .is_ok()
    );
}
