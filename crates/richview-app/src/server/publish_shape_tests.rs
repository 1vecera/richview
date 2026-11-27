//! A crafted publish, check or validate request is answered fast with a finding, and never
//! while it holds the one lock every publisher shares. Each test holds that lock itself, so a
//! request that still needed it would wait for ever, and bounds the time of the work that is
//! left. None of them renders, so none needs the report runtime.
use super::*;
use crate::access::Access;
use crate::storage::FsStorage;
use axum::body::{Body, to_bytes};
use axum::http::Request;
use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use tower::ServiceExt;

const ORG: &str = "aaaa0001";

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Organization A with one owner, who has a CLI token `token-owner`, and an author, Bob, who has
/// `token-bob` and owns nothing here.
fn host() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".access")).unwrap();
    let state = json!({"schema":2,"tenancy":"hosted",
        "organizations":[{"id":ORG,"name":"Acme","created":"2026-01-01T00:00:00.000Z","suspended":false,"limits":{}}],
        "memberships":[{"person":"owner","org":ORG,"role":"owner","created":"2026-01-01T00:01:00.000Z"},
            {"person":"bob","org":ORG,"role":"author","created":"2026-01-01T00:02:00.000Z"}],
        "users":[{"id":"owner","name":"owner","email":"owner@example.test","created":"2026-01-01T00:00:00.000Z",
            "verified":"2026-01-01T00:00:00.000Z",
            "tokens":[{"id":"t-a","name":"CLI","created":"2026-01-01T00:00:00.000Z",
                "sha256":digest("token-owner"),"org":ORG}]},
            {"id":"bob","name":"Bob","email":"bob@example.test","created":"2026-01-01T00:00:00.000Z",
            "verified":"2026-01-01T00:00:00.000Z",
            "tokens":[{"id":"t-b","name":"CLI","created":"2026-01-01T00:00:00.000Z",
                "sha256":digest("token-bob"),"org":ORG}]}],
        "sessions":[{"sha256":digest("session-owner"),"csrf":"csrf-owner","user":"owner",
            "expires":4_102_444_800u64,"scope":""}],
        "acls":{format!("{ORG}/q3"): {"org":ORG,"owner":"owner","visibility":"public","grants":[]},
            // A report that was deleted, or whose publication was interrupted: the record stays.
            format!("{ORG}/kept"): {"org":ORG,"owner":"owner","visibility":"public","grants":[]}}});
    std::fs::write(
        dir.path().join(".access/state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let state = AppState {
        storage: FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true)
            .with_quota(Some(100_000_000)),
        access: Access::open_hosted(dir.path()).unwrap(),
        preview: None,
        public_origin: Some("https://app.test".into()),
        report_origin: Some("https://reports.test".into()),
    };
    (state, dir)
}

/// What the owner's `body` gets from `path`, and how long it took.
async fn post_json(
    state: &AppState,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, String, Duration) {
    post_json_as(state, "token-owner", path, body).await
}

/// The same for the holder of `token`.
async fn post_json_as(
    state: &AppState,
    token: &str,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, String, Duration) {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::HOST, "app.test")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let app = crate::server::build_app(state.clone());
    let started = Instant::now();
    let response = tokio::time::timeout(Duration::from_secs(60), app.oneshot(request))
        .await
        .expect("the request waited for the publication lock")
        .unwrap();
    let status = response.status();
    let text = to_bytes(response.into_body(), 50_000_000).await.unwrap();
    (
        status,
        String::from_utf8_lossy(&text).into_owned(),
        started.elapsed(),
    )
}

/// What the owner's browser session gets from `path`: the editor's way in.
async fn post_as_session(
    state: &AppState,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, String) {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::HOST, "app.test")
        .header(header::ORIGIN, "https://app.test")
        .header(header::COOKIE, "rv_session=session-owner")
        .header("x-richview-csrf", "csrf-owner")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = crate::server::build_app(state.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let text = to_bytes(response.into_body(), 50_000_000).await.unwrap();
    (status, String::from_utf8_lossy(&text).into_owned())
}

/// The same, with the publication lock held by this test the whole time. A request that waited for
/// the lock would not come back.
async fn asked_while_publishing_is_held(
    state: &AppState,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, String, Duration) {
    let held = state.access.publication.clone().lock_owned().await;
    let answer = post_json(state, path, body).await;
    drop(held);
    answer
}

fn source(body: &str) -> String {
    format!("---\ntitle: Q3\ndescription: Open summary\nversion: 1.0.0\n---\n\n# Q3\n\n{body}\n")
}

fn folder(files: Vec<String>) -> serde_json::Value {
    json!({"root": "t", "source": "report.rmd", "files": files})
}

/// A report folder inventory of `count` files.
fn many(count: usize) -> Vec<String> {
    std::iter::once("report.rmd".to_owned())
        .chain((0..count).map(|n| format!("f{n}.csv")))
        .collect()
}

fn refused_as_a_limit(status: StatusCode, text: &str, took: Duration) {
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{text}");
    assert!(text.contains("report.input-limit"), "{text}");
    assert!(
        took < Duration::from_secs(5),
        "refusing took {took:?}, which is work and not a refusal"
    );
}

#[tokio::test]
async fn a_publish_with_a_huge_file_list_or_long_names_is_refused_before_anything_is_walked() {
    let (state, _dir) = host();
    for (label, folder) in [
        ("too many files", folder(many(10_001))),
        (
            "a very long path",
            folder(vec!["report.rmd".into(), "a/".repeat(20_000) + "a.csv"]),
        ),
        (
            "a very long source",
            json!({"root": "t", "source": "/".repeat(100_000), "files": ["report.rmd"]}),
        ),
    ] {
        let (status, text, took) = asked_while_publishing_is_held(
            &state,
            "/api/reports",
            json!({"slug": "q3", "new": true, "source": source("Text."), "reportFolder": folder}),
        )
        .await;
        refused_as_a_limit(status, &text, took);
        assert!(
            !text.contains(&"/".repeat(100)),
            "{label}: the name came back"
        );
    }
    // Nothing was stored, and nothing waited.
    assert!(!state.storage.has_slug(&format!("{ORG}/q3")).unwrap());
}

#[tokio::test]
async fn a_publish_of_a_file_over_its_limit_is_measured_before_the_lock_and_located_in_linear_time()
{
    let (state, _dir) = host();
    // A million-link report would do the same; this is enough for the old quadratic search to
    // run for minutes, and the new one for a moment.
    let links = "[a](a) ".repeat(120_000);
    let big = base64::engine::general_purpose::STANDARD.encode(vec![7u8; 5_000_001]);
    let (status, text, took) = asked_while_publishing_is_held(
        &state,
        "/api/reports",
        json!({"slug": "q3", "new": true, "source": source(&links),
            "files": {"a": big},
            "reportFolder": folder(vec!["report.rmd".into(), "a".into()])}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{text}");
    assert!(text.contains("report.file-limit"), "{text}");
    assert!(
        took < Duration::from_secs(20),
        "locating the file took {took:?}"
    );
    assert!(!state.storage.has_slug(&format!("{ORG}/q3")).unwrap());
}

#[tokio::test]
async fn a_publish_with_a_data_value_that_hides_its_size_in_a_header_is_measured_whole() {
    let (state, _dir) = host();
    // 6 MB of header in front of a one-byte payload: only the payload used to be counted.
    let value = format!("data:{};base64,AA==", "x".repeat(6_000_000));
    let (status, text, _) = asked_while_publishing_is_held(
        &state,
        "/api/reports",
        json!({"slug": "q3", "new": true, "source": source("Text."),
            "data": {"table.csv": value},
            "reportFolder": folder(vec!["report.rmd".into(), "table.csv".into()])}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{text}");
    assert!(text.contains("report.file-limit"), "{text}");
}

#[tokio::test]
async fn check_and_validate_refuse_the_same_requests_before_any_work() {
    let (state, _dir) = host();
    for path in ["/api/check", "/api/validate"] {
        for folder in [
            folder(many(10_001)),
            folder(vec!["report.rmd".into(), "a/".repeat(20_000) + "a.csv"]),
            json!({"root": "t", "source": "/".repeat(100_000), "files": ["report.rmd"]}),
        ] {
            let (status, text, took) = asked_while_publishing_is_held(
                &state,
                path,
                json!({"source": source("Text."), "reportFolder": folder}),
            )
            .await;
            refused_as_a_limit(status, &text, took);
        }
    }
}

#[tokio::test]
async fn a_first_save_never_publishes_into_a_report_that_is_already_there() {
    let (state, _dir) = host();
    let key = format!("{ORG}/q3");
    state
        .storage
        .put_version(StoredReport {
            slug: key.clone(),
            version: "1.0.0".into(),
            source: source("Existing."),
            report_folder: None,
            html: "<html>existing</html>".into(),
            frontmatter: json!({}),
            created_at: "now".into(),
            styles: vec![],
            data: BTreeMap::new(),
            files: BTreeMap::new(),
            content_sha256: String::new(),
        })
        .unwrap();
    assert_eq!(
        state.access.acl(&key).visibility,
        crate::access::Visibility::Public
    );
    // The editor's first save: it makes a report, so an address that is taken is refused, however
    // free the next version number is, and what the report's audience is stays what it was.
    let body = |create_only: bool| {
        json!({"slug": "q3", "org": ORG, "new": true, "createOnly": create_only,
            "source": source("A different report.").replace("1.0.0", "2.0.0"),
            "reportFolder": folder(vec!["report.rmd".into()])})
    };
    let (status, text, _) = post_json(&state, "/api/reports", body(true)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{text}");
    assert!(
        text.contains("publish.slug") && text.contains("already taken"),
        "{text}"
    );
    assert_eq!(state.storage.list_versions(&key).unwrap(), ["1.0.0"]);
    assert_eq!(
        state.access.acl(&key).visibility,
        crate::access::Visibility::Public
    );
    // Anything but a first save is as it was: it is not refused for the address being there.
    let (_, text, _) = post_json(&state, "/api/reports", body(false)).await;
    assert!(!text.contains("already taken"), "{text}");
}

#[tokio::test]
async fn a_first_save_never_takes_an_address_whose_access_record_is_kept() {
    let (state, _dir) = host();
    let key = format!("{ORG}/kept");
    // Nothing is stored at the address, and its record, with its public audience, is.
    assert!(!state.storage.has_slug(&key).unwrap());
    assert_eq!(
        state.access.acl(&key).visibility,
        crate::access::Visibility::Public
    );
    let body = json!({"slug": "kept", "org": ORG, "new": true, "createOnly": true,
        "source": source("A private draft."),
        "reportFolder": folder(vec!["report.rmd".into()])});
    let (status, text, _) = post_json(&state, "/api/reports", body).await;
    assert_eq!(status, StatusCode::CONFLICT, "{text}");
    assert!(
        text.contains("publish.slug") && text.contains("already taken"),
        "{text}"
    );
    assert!(!state.storage.has_slug(&key).unwrap());
    assert_eq!(
        state.access.acl(&key).visibility,
        crate::access::Visibility::Public
    );
}

#[tokio::test]
async fn an_editor_tab_that_does_not_send_create_only_is_held_to_it_by_its_session() {
    let (state, _dir) = host();
    // The tab was opened before the editor sent `createOnly`: it says `new` and nothing more.
    let tab = |slug: &str| {
        json!({"slug": slug, "org": ORG, "new": true, "source": source("A different report.")
                .replace("1.0.0", "2.0.0"),
            "reportFolder": folder(vec!["report.rmd".into()])})
    };
    state
        .storage
        .put_version(StoredReport {
            slug: format!("{ORG}/q3"),
            version: "1.0.0".into(),
            source: source("Existing."),
            report_folder: None,
            html: "<html>existing</html>".into(),
            frontmatter: json!({}),
            created_at: "now".into(),
            styles: vec![],
            data: BTreeMap::new(),
            files: BTreeMap::new(),
            content_sha256: String::new(),
        })
        .unwrap();
    for slug in ["q3", "kept"] {
        let (status, text) = post_as_session(&state, "/api/reports", tab(slug)).await;
        assert_eq!(status, StatusCode::CONFLICT, "{slug}: {text}");
        assert!(text.contains("already taken"), "{slug}: {text}");
    }
    assert_eq!(
        state.storage.list_versions(&format!("{ORG}/q3")).unwrap(),
        ["1.0.0"]
    );
    // The command line says `new` with a token and is not asked to be a first save.
    let (_, text, _) = post_json(&state, "/api/reports", tab("q3")).await;
    assert!(!text.contains("already taken"), "{text}");
}

/// An address kept by a record is nobody's to take: not a command line's `--new`, not an MCP
/// tool's `new`, not a bare token, whoever the token belongs to. Bob is an author of the
/// organization, and Alice's (here the owner's) record is public.
#[tokio::test]
async fn a_token_that_cannot_edit_a_kept_address_never_publishes_there_whichever_way_it_asks() {
    let (state, _dir) = host();
    let key = format!("{ORG}/kept");
    let asks = [
        // `richview publish` revising what it believes exists, and with `--new`.
        ("cli", json!({"new": false})),
        ("cli --new", json!({"new": true})),
        // The MCP tool's `new` is false unless the model passes it.
        ("mcp", json!({"new": false})),
        ("mcp new", json!({"new": true})),
        // A client that says nothing about `new` (it defaults to allowed), and one that says first save.
        ("token", json!({})),
        ("token createOnly", json!({"new": true, "createOnly": true})),
        (
            "cli --new private",
            json!({"new": true, "visibility": "private"}),
        ),
    ];
    for (caller, extra) in asks {
        let mut body = json!({"slug": "kept", "org": ORG, "source": source("Bob's draft."),
            "reportFolder": folder(vec!["report.rmd".into()])});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let (status, text, _) = post_json_as(&state, "token-bob", "/api/reports", body).await;
        assert!(!status.is_success(), "{caller}: {status} {text}");
        assert!(!state.storage.has_slug(&key).unwrap(), "{caller}");
        let acl = state.access.acl(&key);
        assert_eq!(acl.owner, "owner", "{caller}");
        assert_eq!(
            acl.visibility,
            crate::access::Visibility::Public,
            "{caller}"
        );
    }
}

/// The same refusal is the commit's own, decided under the lock every access change takes: a
/// request that got past every earlier look (the record appeared after them) still stores
/// nothing, and the one who may edit the record publishes and keeps its owner and audience.
#[tokio::test]
async fn the_commit_refuses_a_kept_address_to_one_who_cannot_edit_it() {
    let (state, _dir) = host();
    let key = format!("{ORG}/kept");
    let principal = |token: &str| state.access.authenticate(Some(token), None);
    let stored = std::cell::Cell::new(false);
    let bob = state.access.commit_publication(
        || principal("token-bob"),
        &key,
        false,
        None,
        || {
            stored.set(true);
            Ok::<_, ()>(())
        },
    );
    assert!(matches!(bob, Err(crate::access::AccessError::NotFound)));
    assert!(!stored.get());
    let acl = state.access.acl(&key);
    assert_eq!(
        (acl.owner.as_str(), acl.visibility),
        ("owner", crate::access::Visibility::Public)
    );
    // Bob may not make it his own by asking for another audience either.
    let private = state.access.commit_publication(
        || principal("token-bob"),
        &key,
        false,
        Some(crate::access::Visibility::Private),
        || Ok::<_, ()>(()),
    );
    assert!(matches!(private, Err(crate::access::AccessError::NotFound)));
    assert_eq!(
        state.access.acl(&key).visibility,
        crate::access::Visibility::Public
    );
    // The owner of the record publishes, and it keeps the audience it had.
    let owner = state.access.commit_publication(
        || principal("token-owner"),
        &key,
        false,
        None,
        || Ok::<_, ()>(()),
    );
    assert!(matches!(owner, Ok(Ok(()))));
    let acl = state.access.acl(&key);
    assert_eq!(
        (acl.owner.as_str(), acl.visibility),
        ("owner", crate::access::Visibility::Public)
    );
    // A new address is still anyone's who may create reports, and starts private.
    let fresh = format!("{ORG}/brand-new");
    let made = state.access.commit_publication(
        || principal("token-bob"),
        &fresh,
        false,
        None,
        || Ok::<_, ()>(()),
    );
    assert!(matches!(made, Ok(Ok(()))));
    let acl = state.access.acl(&fresh);
    assert_eq!(
        (acl.owner.as_str(), acl.visibility),
        ("bob", crate::access::Visibility::Private)
    );
}
