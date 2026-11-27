use super::*;
use crate::{
    access::Access,
    storage::{FsStorage, StoredReport},
};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::json;
use tower::ServiceExt;

const ID: &str = "1234567890abcdef1234567890abcdef";
const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const MAINT: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const KEY: &str = "aaaa0001/synthetic";

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    app: AppState,
    grant: Grant,
}
impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        fs::create_dir_all(data.join(".access")).unwrap();
        let mut seed = json!({"schema":2,"tenancy":"hosted","organizations":[{"id":"aaaa0001","name":"Example","created":"now","limits":{}}],
            "users":[{"id":"owner","name":"Synthetic owner","role":"owner","tokens":[],"email":"owner@example.test","created":"now","verified":"now"}],
            "memberships":[{"person":"owner","org":"aaaa0001","role":"owner","created":"now"}],
            "sessions":[],"acls":{(KEY):{"org":"aaaa0001","owner":"owner","visibility":"private","grants":[]}}});
        let mut other = seed["users"][0].clone();
        other["id"] = json!("other-owner");
        other["email"] = json!("other@example.test");
        seed["users"].as_array_mut().unwrap().push(other);
        for id in ["owner", "other-owner"] {
            let hash = format!("{:x}", Sha256::digest(format!("session-{id}").as_bytes()));
            seed["sessions"].as_array_mut().unwrap().push(json!({"sha256":hash,"csrf":"fixture-csrf","user":id,"expires":4_102_444_800u64,"scope":""}));
        }
        fs::write(data.join(".access/state.json"), seed.to_string()).unwrap();
        let access = Access::open_hosted(&data).unwrap();
        let storage = FsStorage::new(&data).unwrap().with_hosted(true);
        storage
            .put_version(StoredReport {
                slug: KEY.into(),
                version: "1.0.0".into(),
                source: "PRIVATE SOURCE".into(),
                html: "PRIVATE HTML".into(),
                frontmatter: json!({"title":"Synthetic"}),
                created_at: "2026-11-24T00:00:00Z".into(),
                report_folder: None,
                styles: vec![],
                data: [("rows.csv".into(), "PRIVATE DATA".into())].into(),
                files: [("sample.txt".into(), "ZmlsZQ==".into())].into(),
                content_sha256: String::new(),
            })
            .unwrap();
        let app = AppState {
            access,
            storage,
            preview: None,
            public_origin: None,
            report_origin: None,
        };
        let root = tmp.path().join("journal");
        for name in ["identities", "grants", "revoked", "disclosed", "events"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        fs::write(root.join("authority.lock"), "").unwrap();
        for (token, actor) in [(TOKEN, "staff-a"), (OTHER, "staff-b"), (MAINT, "staff-a")] {
            fs::write(
                root.join("identities")
                    .join(format!("{:x}", Sha256::digest(token.as_bytes()))),
                actor,
            )
            .unwrap();
        }
        let now = Instant::now().unix_seconds();
        let grant = Grant {
            actor: "staff-a".into(),
            owner: "owner".into(),
            case: "case-1".into(),
            evidence: "permission-1".into(),
            purpose: "Investigate <example> with its owner".into(),
            basis: Basis::OwnerPermission,
            sealed: false,
            org: "aaaa0001".into(),
            slug: "synthetic".into(),
            version: "1.0.0".into(),
            artifact: Artifact::Source,
            issued: now - 5,
            expires: now + 120,
        };
        let f = Self {
            _tmp: tmp,
            root,
            app,
            grant,
        };
        f.write_grant(ID);
        f
    }
    fn write_grant(&self, id: &str) {
        fs::write(
            self.root.join("grants").join(id),
            serde_json::to_vec(&self.grant).unwrap(),
        )
        .unwrap();
    }
    fn read(&self, id: &str) -> Result<Vec<u8>, Denial> {
        inspect(&self.root, &self.app, TOKEN, id)
    }
}

#[tokio::test]
async fn concurrent_redemption_and_restart_leave_one_durable_release() {
    let f = Fixture::new();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let run = || {
            barrier.wait();
            f.read(ID)
        };
        let a = scope.spawn(run);
        let b = scope.spawn(run);
        [a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(
        results
            .iter()
            .filter(|r| r.as_deref() == Ok(&b"PRIVATE SOURCE"[..]))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(StatusCode::FORBIDDEN)))
            .count(),
        1
    );
    let release = f
        .root
        .join("events/owner")
        .join(format!("release-{ID}.json"));
    let stamped = fs::read(&release).unwrap();
    let event: Event = serde_json::from_slice(&stamped).unwrap();
    assert!(matches!(event.outcome, Outcome::Released));
    assert_eq!(event.actor, "staff-a");
    assert_eq!(event.grant.case, "case-1");
    assert!(
        !String::from_utf8(stamped.clone())
            .unwrap()
            .contains("PRIVATE SOURCE")
    );
    // A restart: the stopped server lets go of its data directory before the next one opens it.
    let Fixture {
        _tmp, root, app, ..
    } = f;
    let data = app.storage.dir().to_path_buf();
    drop(app);
    let restarted = AppState {
        access: Access::reopen_hosted(&data).await,
        storage: FsStorage::new(&data).unwrap().with_hosted(true),
        preview: None,
        public_origin: None,
        report_origin: None,
    };
    assert_eq!(inspect(&root, &restarted, TOKEN, ID), Err(DENY));
    assert_eq!(fs::read(release).unwrap(), stamped);
}

#[test]
fn each_grant_returns_only_the_named_version_and_artifact() {
    let mut f = Fixture::new();
    for (i, (artifact, expected)) in [
        (Artifact::Html, "PRIVATE HTML"),
        (Artifact::Data("rows.csv".into()), "PRIVATE DATA"),
        (Artifact::File("sample.txt".into()), "file"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("{i:032x}");
        f.grant.artifact = artifact;
        f.write_grant(&id);
        assert_eq!(f.read(&id).unwrap(), expected.as_bytes());
    }
    f.grant.version = "2.0.0".into();
    f.write_grant(ID);
    assert_eq!(f.read(ID), Err(DENY));
    f.grant.artifact = Artifact::File("../elsewhere".into());
    assert!(f.grant.key().is_none());
}

#[test]
fn wrong_operator_owner_expiry_and_revocation_cannot_release_content() {
    let mut f = Fixture::new();
    assert_eq!(inspect(&f.root, &f.app, OTHER, ID), Err(DENY));
    f.grant.owner = "former-owner".into();
    f.write_grant(ID);
    assert_eq!(f.read(ID), Err(DENY));
    assert!(!history(&f.root, "former-owner").unwrap().contains("case-1"));
    f.grant.owner = "owner".into();
    f.grant.issued -= 200;
    f.grant.expires -= 200;
    f.write_grant(ID);
    assert_eq!(f.read(ID), Err(DENY));
    f.grant.issued += 200;
    f.grant.expires += 200;
    f.write_grant(ID);
    fs::write(f.root.join("revoked").join(ID), "").unwrap();
    assert_eq!(f.read(ID), Err(DENY));
    assert!(
        !f.root
            .join(format!("events/owner/release-{ID}.json"))
            .exists()
    );
}

#[test]
fn a_grant_without_a_recorded_reason_releases_nothing_and_writes_no_release() {
    let mut f = Fixture::new();
    for purpose in ["", "   ", "\t\n", "has a\u{7}bell", &"x".repeat(241)] {
        f.grant.purpose = purpose.into();
        f.write_grant(ID);
        assert!(f.grant.key().is_none(), "{purpose:?}");
        assert!(f.read(ID).is_err(), "{purpose:?}");
        assert!(
            !f.root
                .join(format!("events/owner/release-{ID}.json"))
                .exists(),
            "{purpose:?}"
        );
    }
    f.grant.purpose = "Check the column headings the owner reported.".into();
    f.write_grant(ID);
    assert_eq!(f.read(ID).unwrap().as_slice(), b"PRIVATE SOURCE");
}

#[test]
fn a_person_who_asks_again_and_again_leaves_one_refusal_record_and_not_one_for_each_ask() {
    let mut f = Fixture::new();
    f.grant.version = "9.9.9".into();
    f.write_grant(ID);
    for _ in 0..40 {
        assert_eq!(f.read(ID), Err(DENY));
    }
    let records = fs::read_dir(f.root.join("events/owner"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("refusal-"))
        .count();
    assert_eq!(records, 1);
    // The one record says how often it was asked for and when last, and the owner's history shows
    // it once.
    let shown = history(&f.root, "owner").unwrap();
    assert_eq!(shown.matches("<li>").count(), 1, "{shown}");
    assert!(shown.contains("asked 40 times, last "), "{shown}");
}

#[test]
fn refusals_that_arrive_together_are_all_counted_in_the_record_of_either_kind() {
    let f = Fixture::new();
    for owner_visible in [true, false] {
        let refused = || Event {
            grant_id: ID.into(),
            actor: "staff-b".into(),
            time: Instant::now().millis(),
            outcome: Outcome::Refused,
            grant: f.grant.clone(),
            count: 0,
            last: None,
        };
        journal(&f.root, &refused(), owner_visible).unwrap();
        let barrier = std::sync::Barrier::new(16);
        std::thread::scope(|scope| {
            for _ in 0..16 {
                scope.spawn(|| {
                    barrier.wait();
                    journal(&f.root, &refused(), owner_visible).unwrap();
                });
            }
        });
        let dir = f
            .root
            .join("events")
            .join(if owner_visible { "owner" } else { ".custodian" });
        let records: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("refusal-"))
            .collect();
        assert_eq!(records.len(), 1);
        let kept: Event = load(&records[0].path()).unwrap();
        assert_eq!(kept.count, 17, "owner_visible: {owner_visible}");
    }
}

#[test]
fn a_refusal_written_late_never_moves_the_time_it_was_last_asked_backward() {
    let f = Fixture::new();
    let refused = |time: &str| Event {
        grant_id: ID.into(),
        actor: "staff-b".into(),
        time: time.into(),
        outcome: Outcome::Refused,
        grant: f.grant.clone(),
        count: 0,
        last: None,
    };
    let at = |second: u32| format!("2026-11-26T10:00:{second:02}.000Z");
    journal(&f.root, &refused(&at(1)), true).unwrap();
    // The third ask was stamped before the second, and written after it.
    journal(&f.root, &refused(&at(30)), true).unwrap();
    journal(&f.root, &refused(&at(20)), true).unwrap();
    let record = fs::read_dir(f.root.join("events/owner"))
        .unwrap()
        .flatten()
        .find(|entry| entry.file_name().to_string_lossy().starts_with("refusal-"))
        .unwrap()
        .path();
    let kept: Event = load(&record).unwrap();
    assert_eq!(kept.count, 3);
    assert_eq!(kept.last.as_deref(), Some(at(30).as_str()));
}

#[test]
fn a_count_that_cannot_be_made_is_an_outage_and_not_a_refusal_that_leaves_no_trace() {
    let f = Fixture::new();
    let refused = || Event {
        grant_id: ID.into(),
        actor: "staff-b".into(),
        time: Instant::now().millis(),
        outcome: Outcome::Refused,
        grant: f.grant.clone(),
        count: 0,
        last: None,
    };
    journal(&f.root, &refused(), true).unwrap();
    let record = fs::read_dir(f.root.join("events/owner"))
        .unwrap()
        .flatten()
        .find(|entry| entry.file_name().to_string_lossy().starts_with("refusal-"))
        .unwrap()
        .path();
    // The record on file cannot be read back.
    fs::write(&record, "not a record").unwrap();
    assert!(journal(&f.root, &refused(), true).is_err());
}

#[test]
fn journal_outage_denies_and_owner_history_does_not_claim_zero() {
    let f = Fixture::new();
    fs::rename(f.root.join("events"), f.root.join("offline")).unwrap();
    assert_eq!(f.read(ID), Err(HOLD));
    assert!(history(&f.root, "owner").is_err());
    fs::rename(f.root.join("offline"), f.root.join("events")).unwrap();
    assert_eq!(f.read(ID).unwrap().as_slice(), b"PRIVATE SOURCE");
}

#[test]
fn history_is_scoped_to_the_owner_and_honors_a_disclosure_hold() {
    let mut f = Fixture::new();
    f.grant.sealed = true;
    assert!(f.grant.key().is_none());
    f.grant.basis = Basis::LegalOrder;
    f.write_grant(ID);
    f.read(ID).unwrap();
    assert!(!history(&f.root, "owner").unwrap().contains("case-1"));
    fs::write(f.root.join("disclosed").join(ID), "confirmed").unwrap();
    let view = history(&f.root, "owner").unwrap();
    assert!(
        view.contains("case-1") && view.contains("staff-a") && view.contains("&lt;example&gt;")
    );
    assert!(!view.contains("permission-1"));
    assert!(!history(&f.root, "other-owner").unwrap().contains("case-1"));
}

#[tokio::test]
async fn the_loopback_read_refuses_shared_maintenance_credentials_and_never_caches_bytes() {
    let f = Fixture::new();
    let app = router(f.app.clone(), MAINT, Some(f.root.clone()));
    for (token, status) in [(MAINT, DENY), (TOKEN, StatusCode::OK), (TOKEN, DENY)] {
        let req = Request::post(format!("/staff/inspections/{ID}"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
        if status == StatusCode::OK {
            assert_eq!(bytes.as_ref(), b"PRIVATE SOURCE");
        } else {
            assert!(bytes.is_empty());
        }
    }
}

#[tokio::test]
async fn account_history_uses_the_session_even_if_a_different_owner_is_requested() {
    let f = Fixture::new();
    f.read(ID).unwrap();
    for (reader, shows_entry) in [("owner", true), ("other-owner", false), ("missing", false)] {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("rv_session=session-{reader}").parse().unwrap(),
        );
        let query = serde_json::from_value(json!({"view":"staff-access","owner":"owner"})).unwrap();
        let response =
            super::super::ui::account_response(f.app.clone(), headers, query, Some(f.root.clone()))
                .await;
        if reader == "missing" {
            assert_ne!(response.status(), StatusCode::OK);
        } else {
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "private, no-store"
            );
        }
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&body).contains("case-1"),
            shows_entry
        );
    }
}

/// The owner's account page without the feature, with it, and its history after one release.
/// Set `STAFF_ACCESS_WALK_DIR` to keep the three pages for a local walk or a visual review.
#[tokio::test]
async fn the_account_shows_the_history_link_only_once_the_feature_is_on() {
    let f = Fixture::new();
    f.read(ID).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let out = std::env::var_os("STAFF_ACCESS_WALK_DIR")
        .map_or_else(|| scratch.path().into(), PathBuf::from);
    fs::create_dir_all(&out).unwrap();
    for (filename, view, enabled, shows_link, shows_entry) in [
        ("before.html", "", false, false, false),
        ("account.html", "", true, true, false),
        ("history.html", "staff-access", true, false, true),
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, "rv_session=session-owner".parse().unwrap());
        let query = serde_json::from_value(json!({"view":view})).unwrap();
        let response = super::super::ui::account_response(
            f.app.clone(),
            headers,
            query,
            enabled.then(|| f.root.clone()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "{filename}");
        let page = to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&page);
        assert_eq!(
            text.contains("Staff access history"),
            shows_link || shows_entry,
            "{filename}"
        );
        assert_eq!(text.contains("case-1"), shows_entry, "{filename}");
        fs::write(out.join(filename), page).unwrap();
    }
}
