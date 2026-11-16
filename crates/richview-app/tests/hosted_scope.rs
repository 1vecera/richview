//! Hosted organizations do not share private reports, tokens, or org visibility.
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use richview_app::{
    access::Access,
    server::{AppState, build_app, build_report_app},
    storage::{FsStorage, StoredReport},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};
use tower::ServiceExt;

struct Fixture {
    root: PathBuf,
    access: Access,
    app: axum::Router,
    reports: axum::Router,
}
impl Fixture {
    fn new(suspend_second: bool) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::current_dir().unwrap().join("tmp").join(format!(
            "hosted-scope-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join(".access")).unwrap();
        let hash = |token: &str| format!("{:x}", Sha256::digest(token.as_bytes()));
        let state = json!({"schema":2,"tenancy":"hosted",
        "organizations":[{"id":"aaaa0001","name":"A","created":"now","suspended":false,"limits":{}},
                         {"id":"bbbb0002","name":"B","created":"now","suspended":suspend_second,"limits":{}}],
            "memberships":[{"person":"a","org":"aaaa0001","role":"owner","created":"now"},
                           {"person":"a","org":"bbbb0002","role":"reader","created":"now"},
                           {"person":"b","org":"bbbb0002","role":"owner","created":"now"}],
        "users":[{"id":"a","name":"A","email":"a@example.test","created":"now","verified":"now","tokens":[{"id":"ta","name":"CLI","created":"now","sha256":hash("token-a"),"org":"aaaa0001"}]},
                 {"id":"b","name":"B","email":"b@example.test","created":"now","verified":"now","tokens":[{"id":"tb","name":"CLI","created":"now","sha256":hash("token-b"),"org":"bbbb0002"}]}],
        "sessions":[], "acls":{
            "aaaa0001/secret":{"org":"aaaa0001","owner":"a","visibility":"private","grants":[]},
            "aaaa0001/orgonly":{"org":"aaaa0001","owner":"a","visibility":"org","grants":[]},
            "aaaa0001/public":{"org":"aaaa0001","owner":"a","visibility":"public","grants":[]},
            "bbbb0002/secret":{"org":"bbbb0002","owner":"b","visibility":"private","grants":[]},
                "bbbb0002/orgonly":{"org":"bbbb0002","owner":"b","visibility":"org","grants":[]},
            "bbbb0002/public":{"org":"bbbb0002","owner":"b","visibility":"public","grants":[]}
        }});
        std::fs::write(
            root.join(".access/state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let access = Access::open_hosted(&root).unwrap();
        let storage = FsStorage::new(&root).unwrap().with_hosted(true);
        for key in [
            "aaaa0001/secret",
            "aaaa0001/orgonly",
            "aaaa0001/public",
            "bbbb0002/secret",
            "bbbb0002/orgonly",
            "bbbb0002/public",
        ] {
            storage
                .put_version(StoredReport {
                    slug: key.into(),
                    version: "1.0.0".into(),
                    source: format!("# {key}"),
                    report_folder: None,
                    html: format!("<h1>{key}</h1>"),
                    frontmatter: json!({"title":key}),
                    created_at: "now".into(),
                    styles: vec![],
                    data: BTreeMap::new(),
                    files: BTreeMap::new(),
                    content_sha256: String::new(),
                })
                .unwrap();
        }
        let state = AppState {
            storage,
            access: access.clone(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let app = build_app(state.clone());
        let reports = build_report_app(state);
        Self {
            root,
            access,
            app,
            reports,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    token: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "app.test");
    if let Some(token) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = String::from_utf8_lossy(
        &to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .into_owned();
    (status, headers, body)
}

#[tokio::test]
async fn oversized_publish_reports_the_hosted_limit_and_remedy() {
    let host = Fixture::new(false);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let app = host.app.clone();
    let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let source =
        "---\ntitle: Large report\ndescription: Local test\nversion: 1.0.0\n---\n\n# Report\n";
    let options = richview_core::document::RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "fixture".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        data: [("large".into(), "x".repeat(richview_app::MAX_INPUT_BYTES))].into(),
        ..Default::default()
    };
    let error = richview_app::cli::publish(source, &options, &server, "token-a", None, true)
        .await
        .unwrap_err();
    assert!(error.contains("413"), "{error}");
    assert!(error.contains("32 MiB"), "{error}");
    assert!(error.contains("smaller data"), "{error}");
    assert!(error.contains("link large data"), "{error}");
    worker.abort();
}

#[tokio::test]
async fn organization_owner_and_token_cannot_cross_into_another_organization() {
    let host = Fixture::new(false);
    assert_eq!(
        call(&host.app, "GET", "/r/secret", Some("token-a")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&host.app, "GET", "/api/reports/secret", Some("token-a"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &host.app,
            "GET",
            "/api/reports/aaaa0001/missing/access",
            Some("token-a")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let missing_share = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/reports/aaaa0001/missing/access")
                .header("host", "app.test")
                .header("content-type", "application/json")
                .header(header::AUTHORIZATION, "Bearer token-a")
                .body(Body::from(r#"{"visibility":"public","grants":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_share.status(), StatusCode::NOT_FOUND);
    assert!(
        !host.access.has_record("aaaa0001/missing"),
        "sharing a missing slug cannot pre-set its visibility"
    );
    assert_eq!(
        call(
            &host.app,
            "GET",
            "/api/reports/aaaa0001/secret",
            Some("token-a")
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &host.app,
            "GET",
            "/api/reports/aaaa0001/missing/versions",
            Some("token-a")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &host.app,
            "GET",
            "/api/reports/bbbb0002/orgonly",
            Some("token-a")
        )
        .await
        .0,
        StatusCode::NOT_FOUND,
        "a token remains scoped even if its person is a member elsewhere"
    );
    assert_eq!(
        call(
            &host.app,
            "GET",
            "/api/reports/aaaa0001/secret",
            Some("token-b")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &host.app,
            "GET",
            "/api/reports/aaaa0001/orgonly",
            Some("token-b")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &host.app,
            "DELETE",
            "/api/reports/aaaa0001/secret",
            Some("token-b")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &host.app,
            "GET",
            "/api/reports/aaaa0001/public",
            Some("token-b")
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, _, body) = call(&host.app, "GET", "/api/reports", Some("token-b")).await;
    let rows: Vec<Value> = serde_json::from_str(&body).unwrap();
    assert!(rows.iter().all(|row| row["org"] == "bbbb0002"));
    assert!(rows.iter().any(|row| row["slug"] == "secret"));
    let (status, headers, body) = call(&host.reports, "GET", "/r/aaaa0001/public", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["x-robots-tag"], "noindex");
    assert_eq!(headers["cache-control"], "private, no-store");
    assert!(body.contains("aaaa0001/public"));
    assert_eq!(
        call(&host.app, "GET", "/api/governance/users", Some("token-a"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&host.reports, "GET", "/richview_docs.js", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn suspension_denies_public_reports_and_the_organization_token() {
    let host = Fixture::new(true);
    assert_eq!(
        call(
            &host.app,
            "GET",
            "/api/reports/bbbb0002/secret",
            Some("token-b")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&host.reports, "GET", "/r/bbbb0002/public", None)
            .await
            .0,
        StatusCode::SEE_OTHER
    );
    assert!(matches!(
        host.access.authenticate(Some("token-b"), None),
        richview_app::access::Principal::Anonymous
    ));
}

#[tokio::test]
async fn split_origin_handoff_is_bound_to_one_organization_and_report() {
    let host = Fixture::new(false);
    host.access
        .set_password("a", "correct horse battery")
        .unwrap();
    let (session, _) = host
        .access
        .sign_in("a@example.test", "correct horse battery")
        .unwrap();
    let state = AppState {
        storage: FsStorage::new(&host.root).unwrap().with_hosted(true),
        access: host.access.clone(),
        preview: None,
        public_origin: Some("https://app.test".into()),
        report_origin: Some("https://reports.test".into()),
    };
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        header::COOKIE,
        format!("rv_session={session}").parse().unwrap(),
    );
    let caller = richview_app::server::auth::caller(&state, &headers);
    let owned = richview_app::server::reports::owned_report_rows(&state, &caller)
        .await
        .unwrap();
    assert!(!owned.is_empty());
    assert!(owned.iter().all(|row| row["org"] == "aaaa0001"));
    assert!(
        owned
            .iter()
            .any(|row| row["url"] == "https://reports.test/r/aaaa0001/secret")
    );
    let shared = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/reports/bbbb0002/orgonly")
                .header("host", "app.test")
                .header(header::COOKIE, format!("rv_session={session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        shared.status(),
        StatusCode::OK,
        "the same person may use their other membership by session"
    );
    let response = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/r/aaaa0001/secret")
                .header("host", "app.test")
                .header(header::COOKIE, format!("rv_session={session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
    let handoff =
        reqwest::Url::parse(response.headers()[header::LOCATION].to_str().unwrap()).unwrap();
    assert_eq!(handoff.host_str(), Some("reports.test"));
    assert_eq!(handoff.path(), "/r/aaaa0001/secret");
    let report_uri = format!("{}?{}", handoff.path(), handoff.query().unwrap());
    let response = host
        .reports
        .clone()
        .oneshot(
            Request::builder()
                .uri(report_uri)
                .header("host", "reports.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.contains("Path=/r/aaaa0001/secret"));
    let cookie = cookie.split(';').next().unwrap();
    let response = host
        .reports
        .clone()
        .oneshot(
            Request::builder()
                .uri("/r/aaaa0001/secret")
                .header("host", "reports.test")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = host
        .reports
        .clone()
        .oneshot(
            Request::builder()
                .uri("/r/bbbb0002/secret")
                .header("host", "reports.test")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::OK);
}

#[cfg(feature = "embedded-runtime")]
#[tokio::test]
async fn copy_runtime_is_served_only_on_the_hosted_app_origin() {
    let host = Fixture::new(false);
    for path in [
        "/richview_docs.js",
        "/richview_docs_bg.wasm",
        "/bootstrap.js",
    ] {
        assert_eq!(call(&host.app, "GET", path, None).await.0, StatusCode::OK);
        assert_eq!(
            call(&host.reports, "GET", path, None).await.0,
            StatusCode::NOT_FOUND
        );
    }
}

#[cfg(feature = "embedded-runtime")]
#[tokio::test]
async fn publish_uses_the_token_organization_and_allows_the_same_slug_elsewhere() {
    let host = Fixture::new(false);
    async fn publish(app: &axum::Router, token: &str, org: Option<&str>) -> (StatusCode, Value) {
        let mut body = json!({"slug":"q4", "new":true, "source":"---\ntitle: Q4\ndescription: D\nversion: 1.0.0\n---\n\n# Q4\n", "reportFolder":{"root":"t","source":"report.rmd","files":["report.rmd"]}});
        if let Some(org) = org {
            body["org"] = json!(org);
        }
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/reports")
                    .header("host", "app.test")
                    .header("content-type", "application/json")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let body =
            serde_json::from_slice(&to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap())
                .unwrap();
        (status, body)
    }
    assert_eq!(
        publish(&host.app, "token-b", Some("aaaa0001")).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, first) = publish(&host.app, "token-a", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(first.to_string().contains("/r/aaaa0001/q4"));
    for path in ["/r/aaaa0001/q4/download", "/r/aaaa0001/q4/v/1.0.0/download"] {
        let (status, headers, body) = call(&host.reports, "GET", path, Some("token-a")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            headers[header::CONTENT_DISPOSITION]
                .to_str()
                .unwrap()
                .contains("q4-1.0.0.html")
        );
        assert!(body.contains("Q4"));
    }
    let (status, second) = publish(&host.app, "token-b", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(second.to_string().contains("/r/bbbb0002/q4"));
    assert!(host.root.join("orgs/aaaa0001/q4/1.0.0/source.rmd").exists());
    assert!(host.root.join("orgs/bbbb0002/q4/1.0.0/source.rmd").exists());
}

#[test]
fn stale_share_authority_is_rechecked_in_the_acl_write() {
    use richview_app::access::{Grant, Principal, ReportRole, Visibility};
    let fixture = Fixture::new(true);
    // This principal models an owner authorized just before the organization was suspended.
    let stale_owner = Principal::User(fixture.access.user("b").unwrap());
    assert!(
        fixture
            .access
            .set_acl_authorized(
                || stale_owner.clone(),
                "bbbb0002/secret",
                Visibility::Public,
                vec![]
            )
            .is_err()
    );
    assert!(
        fixture
            .access
            .set_visibility_authorized(
                || stale_owner.clone(),
                "bbbb0002/secret",
                Visibility::Public
            )
            .is_err()
    );
    assert!(
        fixture
            .access
            .set_access_authorized(
                || stale_owner.clone(),
                "bbbb0002/secret",
                None,
                Visibility::Public,
                vec![]
            )
            .is_err()
    );
    assert!(
        fixture
            .access
            .set_acl_for_fixture("bbbb0002/secret", Visibility::Public, vec![])
            .is_err()
    );
    assert!(
        fixture
            .access
            .set_access_for_fixture("bbbb0002/secret", None, Visibility::Public, vec![])
            .is_err()
    );
    assert_eq!(
        fixture.access.acl("bbbb0002/secret").visibility,
        Visibility::Private
    );

    let active = Fixture::new(false);
    let reader = Principal::User(active.access.user("a").unwrap());
    assert!(
        active
            .access
            .set_acl_authorized(
                || reader.clone(),
                "bbbb0002/secret",
                Visibility::Public,
                vec![]
            )
            .is_err()
    );
    let owner = Principal::User(active.access.user("b").unwrap());
    let grant = Grant {
        user: "a".into(),
        role: ReportRole::Viewer,
    };
    assert!(
        active
            .access
            .set_acl_authorized(
                || owner.clone(),
                "bbbb0002/secret",
                Visibility::Private,
                vec![grant.clone()]
            )
            .is_ok()
    );
    assert!(
        active
            .access
            .set_visibility_authorized(|| owner.clone(), "bbbb0002/secret", Visibility::Public)
            .is_ok()
    );
    assert_eq!(active.access.acl("bbbb0002/secret").grants, vec![grant]);
}

#[tokio::test]
async fn operator_actions_suspend_delete_pause_and_never_expose_report_content() {
    use richview_app::{
        access::{LinkPurpose, Principal},
        server::hosted::{OperatorAction, operator_action},
    };
    let fixture = Fixture::new(false);
    assert_eq!(
        call(
            &fixture.app,
            "GET",
            "/operator/organizations",
            Some("token-a")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let storage = FsStorage::new(&fixture.root).unwrap().with_hosted(true);
    let state = AppState {
        storage,
        access: fixture.access.clone(),
        preview: None,
        public_origin: Some("https://app.test".into()),
        report_origin: Some("https://reports.test".into()),
    };
    let token = state
        .access
        .issue_link("new@example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    let links = operator_action(&state, OperatorAction::PendingLinks)
        .await
        .unwrap();
    assert!(links.to_string().contains("new@example.test"));
    assert!(!links.to_string().contains(&token));
    let orgs = operator_action(&state, OperatorAction::ListOrgs)
        .await
        .unwrap();
    assert_eq!(orgs["organizations"].as_array().unwrap().len(), 2);
    assert!(!orgs.to_string().contains("<h1>"));
    operator_action(
        &state,
        OperatorAction::SetSuspended {
            org: "bbbb0002".into(),
            suspended: true,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        state.access.authenticate(Some("token-b"), None),
        Principal::Anonymous
    ));
    assert_eq!(
        call(&fixture.reports, "GET", "/r/bbbb0002/public", None)
            .await
            .0,
        StatusCode::SEE_OTHER
    );
    operator_action(
        &state,
        OperatorAction::SetSuspended {
            org: "bbbb0002".into(),
            suspended: false,
        },
    )
    .await
    .unwrap();
    operator_action(&state, OperatorAction::SetSignupsPaused { paused: true })
        .await
        .unwrap();
    assert!(state.access.signups_paused());
    let saved: Value =
        serde_json::from_slice(&std::fs::read(fixture.root.join(".access/state.json")).unwrap())
            .unwrap();
    assert_eq!(saved["signups_paused"], true);
    operator_action(
        &state,
        OperatorAction::DeleteReport {
            org: "bbbb0002".into(),
            slug: "secret".into(),
        },
    )
    .await
    .unwrap();
    assert!(!fixture.root.join("orgs/bbbb0002/secret/1.0.0").exists());
    assert!(fixture.root.join("orgs/bbbb0002/secret/.deleted").exists());
    operator_action(
        &state,
        OperatorAction::DisablePerson {
            person: "b".into(),
            disabled: true,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        state.access.authenticate(Some("token-b"), None),
        Principal::Anonymous
    ));
    let summary = operator_action(&state, OperatorAction::Summary)
        .await
        .unwrap();
    assert_eq!(summary["accounts"], 2);
    assert!(!summary.to_string().contains("<h1>"));
    let log = std::fs::read_to_string(fixture.root.join(".access/audit.log")).unwrap();
    assert!(log.contains("operator.attempt.delete-report"));
    assert!(log.contains("operator.complete.delete-report"));
    assert!(!log.contains(&token));
    for line in log.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        if record["action"]
            .as_str()
            .is_some_and(|action| action.starts_with("operator."))
        {
            assert_eq!(record["actor"], "operator");
        }
    }
}

#[tokio::test]
async fn operator_will_not_mutate_if_the_attempt_cannot_be_audited() {
    use richview_app::server::hosted::{OperatorAction, operator_action};
    let fixture = Fixture::new(false);
    let state = AppState {
        storage: FsStorage::new(&fixture.root).unwrap().with_hosted(true),
        access: fixture.access.clone(),
        preview: None,
        public_origin: None,
        report_origin: None,
    };
    std::fs::create_dir(fixture.root.join(".access/audit.log")).unwrap();
    assert!(
        operator_action(
            &state,
            OperatorAction::SetSuspended {
                org: "bbbb0002".into(),
                suspended: true
            }
        )
        .await
        .is_err()
    );
    assert!(state.access.report_org_active("bbbb0002/secret"));
}

#[tokio::test]
async fn hosted_access_put_bounds_and_deduplicates_grants() {
    use richview_app::access::{Grant, ReportRole, Visibility};
    let host = Fixture::new(false);
    let owner = host.access.authenticate(Some("token-a"), None);
    let grant = Grant {
        user: "b".into(),
        role: ReportRole::Viewer,
    };
    let saved = host
        .access
        .set_access_authorized(
            || owner.clone(),
            "aaaa0001/secret",
            None,
            Visibility::Private,
            vec![grant.clone(); 30_000],
        )
        .unwrap();
    assert_eq!(saved.grants, vec![grant]);
    assert!(
        std::fs::metadata(host.root.join(".access/state.json"))
            .unwrap()
            .len()
            < 20_000
    );
    let grants = vec![json!({"user":"b", "role":"viewer"}); 30_000];
    let response = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/reports/aaaa0001/secret/access")
                .header("host", "app.test")
                .header(header::AUTHORIZATION, "Bearer token-a")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"visibility":"public", "grants":grants}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        host.access.acl("aaaa0001/secret").visibility,
        Visibility::Private
    );
    assert_eq!(host.access.acl("aaaa0001/secret").grants.len(), 1);
    let too_many = (0..33)
        .map(|i| Grant {
            user: format!("user-{i}"),
            role: ReportRole::Viewer,
        })
        .collect();
    assert!(
        host.access
            .set_access_authorized(
                || owner.clone(),
                "aaaa0001/secret",
                None,
                Visibility::Public,
                too_many
            )
            .is_err()
    );
    assert_eq!(
        host.access.acl("aaaa0001/secret").visibility,
        Visibility::Private
    );
    let merged = host
        .access
        .set_access_authorized(
            || owner.clone(),
            "aaaa0001/secret",
            None,
            Visibility::Private,
            vec![
                Grant {
                    user: "b".into(),
                    role: ReportRole::Viewer,
                },
                Grant {
                    user: "b".into(),
                    role: ReportRole::Editor,
                },
            ],
        )
        .unwrap();
    assert_eq!(merged.grants[0].role, ReportRole::Editor);
}

#[tokio::test]
async fn retired_address_limit_refuses_a_new_publish_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".access")).unwrap();
    let token_hash = format!("{:x}", Sha256::digest(b"owner-token"));
    let acls: serde_json::Map<String, Value> = (0..256)
        .map(|i| {
            (
                format!("aaaa0001/old-{i}"),
                json!({"org":"aaaa0001", "owner":"a", "visibility":"private", "grants":[]}),
            )
        })
        .collect();
    let state = json!({"schema":2,"tenancy":"hosted", "organizations":[{"id":"aaaa0001","name":"A","created":"now","limits":{}}],
        "memberships":[{"person":"a","org":"aaaa0001","role":"owner","created":"now"}],
        "users":[{"id":"a","name":"A","email":"a@example.test","created":"now","verified":"now","tokens":[{"id":"t","name":"CLI","created":"now","sha256":token_hash,"org":"aaaa0001"}]}],"sessions":[],"acls":acls});
    std::fs::write(
        dir.path().join(".access/state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    let app = build_app(AppState {
        storage: FsStorage::new(dir.path()).unwrap().with_hosted(true),
        access,
        preview: None,
        public_origin: Some("https://app.test".into()),
        report_origin: Some("https://reports.test".into()),
    });
    let body = json!({"slug":"new-report","new":true,"source":"---\ntitle: New\ndescription: D\nversion: 1.0.0\n---\n\n# New\n"});
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header("host", "app.test")
                .header(header::AUTHORIZATION, "Bearer owner-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!dir.path().join("orgs/aaaa0001/new-report").exists());
}

#[test]
fn opening_hosted_state_compacts_existing_duplicate_grants() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".access")).unwrap();
    let path = dir.path().join(".access/state.json");
    let grants = vec![json!({"user":"b", "role":"viewer"}); 30_000];
    let state = json!({"schema":2,"tenancy":"hosted", "organizations":[{"id":"aaaa0001","name":"A","created":"now","limits":{}}],
        "memberships":[{"person":"a","org":"aaaa0001","role":"owner","created":"now"}],
        "users":[{"id":"a","name":"A","email":"a@example.test","created":"now","verified":"now","tokens":[]}, {"id":"b","name":"B","email":"b@example.test","created":"now","verified":"now","tokens":[]}],
        "sessions":[],"acls":{"aaaa0001/report":{"org":"aaaa0001","owner":"a","visibility":"private","grants":grants}}});
    std::fs::write(&path, serde_json::to_vec(&state).unwrap()).unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    assert_eq!(access.acl("aaaa0001/report").grants.len(), 1);
    assert!(std::fs::metadata(path).unwrap().len() < 10_000);
}

#[tokio::test]
async fn anonymous_missing_private_and_suspended_orgs_have_the_same_handoff() {
    let host = Fixture::new(true);
    for path in [
        "/r/aaaa0001/missing",
        "/r/bbbb0002/missing",
        "/r/zzzz9999/missing",
        "/r/aaaa0001/secret",
        "/r/bbbb0002/public",
    ] {
        let (status, headers, body) = call(&host.reports, "GET", path, None).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{path}");
        assert_eq!(
            headers[header::LOCATION],
            format!("https://app.test{path}?rv=signin")
        );
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
        assert!(body.is_empty());
    }
    for path in [
        "/raw/aaaa0001/missing",
        "/raw/bbbb0002/missing",
        "/raw/zzzz9999/missing",
    ] {
        assert_eq!(
            call(&host.reports, "GET", path, None).await.0,
            StatusCode::NOT_FOUND
        );
    }
}

#[tokio::test]
async fn hosted_link_paths_never_send_referrers_even_on_early_errors() {
    let host = Fixture::new(false);
    for (method, path) in [
        ("GET", "/link/not-a-token"),
        ("POST", "/ui/link/not-a-token"),
    ] {
        let (_, headers, _) = call(&host.app, method, path, None).await;
        assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    }
}

#[tokio::test]
async fn missing_hosted_attachment_does_not_load_the_report_source() {
    let fixture = Fixture::new(false);
    std::fs::remove_file(fixture.root.join("orgs/aaaa0001/public/1.0.0/source.rmd")).unwrap();
    let response = call(
        &fixture.reports,
        "GET",
        "/r/aaaa0001/public/v/1.0.0/files/missing.csv",
        None,
    )
    .await;
    assert_eq!(response.0, StatusCode::NOT_FOUND);
    std::fs::write(
        fixture.root.join("orgs/aaaa0001/public/1.0.0/assets.json"),
        r#"{"data":{"large":"unrelated"},"files":{"hello.txt":"aGVsbG8="}}"#,
    )
    .unwrap();
    let response = call(
        &fixture.reports,
        "GET",
        "/r/aaaa0001/public/v/1.0.0/files/hello.txt",
        None,
    )
    .await;
    assert_eq!(response.0, StatusCode::OK);
    assert_eq!(response.2, "hello");
}

#[tokio::test]
async fn hosted_compute_routes_share_an_hourly_organization_limit_before_body_parsing() {
    let fixture = Fixture::new(false);
    async fn attempt(app: &axum::Router, token: &str, path: &str) -> StatusCode {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }
    let paths = [
        "/api/check",
        "/api/validate",
        "/api/reports/aaaa0001/public/v/9.9.9/regenerate",
    ];
    for i in 0..60 {
        assert_ne!(
            attempt(&fixture.app, "token-a", paths[i % paths.len()]).await,
            StatusCode::TOO_MANY_REQUESTS
        );
    }
    for path in paths {
        assert_eq!(
            attempt(&fixture.app, "token-a", path).await,
            StatusCode::TOO_MANY_REQUESTS
        );
    }
    assert_ne!(
        attempt(&fixture.app, "token-b", "/api/check").await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn hosted_app_errors_offer_a_way_back() {
    let fixture = Fixture::new(false);
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/does-not-exist")
                .header(header::ACCEPT, "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = String::from_utf8(
        to_bytes(response.into_body(), 2_000_000)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(body.contains("sign in or go to your reports"));
}

#[tokio::test]
async fn account_usage_is_scoped_to_an_active_member_and_matches_hosted_limits() {
    use richview_app::{access::Principal, server::hosted::account_usage};
    let fixture = Fixture::new(false);
    let state = AppState {
        storage: FsStorage::new(&fixture.root)
            .unwrap()
            .with_hosted(true)
            .with_hosted_limits(12_345, 7),
        access: fixture.access.clone(),
        preview: None,
        public_origin: None,
        report_origin: None,
    };
    let alice = fixture.access.authenticate(Some("token-a"), None);
    let bob = fixture.access.authenticate(Some("token-b"), None);
    let usage = account_usage(&state, &alice, "aaaa0001").await.unwrap();
    assert_eq!(usage.reports, 3);
    assert!(usage.bytes > 0);
    assert_eq!(usage.byte_limit, 12_345);
    assert_eq!(usage.report_limit, 7);
    assert!(state.storage.delete_report("aaaa0001/public").unwrap());
    assert_eq!(
        account_usage(&state, &alice, "aaaa0001")
            .await
            .unwrap()
            .reports,
        2
    );
    assert!(account_usage(&state, &bob, "aaaa0001").await.is_err());
    assert!(
        account_usage(&state, &Principal::Anonymous, "aaaa0001")
            .await
            .is_err()
    );
    assert!(account_usage(&state, &alice, "bbbb0002").await.is_err());
    let suspended = Fixture::new(true);
    let suspended_state = AppState {
        storage: FsStorage::new(&suspended.root).unwrap().with_hosted(true),
        access: suspended.access.clone(),
        preview: None,
        public_origin: None,
        report_origin: None,
    };
    let suspended_bob = suspended.access.authenticate(Some("token-b"), None);
    assert!(
        account_usage(&suspended_state, &suspended_bob, "bbbb0002")
            .await
            .is_err()
    );
}
