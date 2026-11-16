//! Cross-organization denials cover the hosted report route table, not a hand-picked subset.
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use richview_app::{
    access::Access,
    server::{
        AppState, build_app, build_report_app,
        routes::{Rule, hosted_specs},
    },
    storage::{FsStorage, StoredReport},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};
use tower::ServiceExt;

struct Fixture {
    _dir: tempfile::TempDir,
    app: axum::Router,
    reports: axum::Router,
}

impl Fixture {
    fn new() -> Self {
        let temp = std::env::current_dir().unwrap().join("tmp");
        fs::create_dir_all(&temp).unwrap();
        let dir = tempfile::Builder::new()
            .prefix("hosted-matrix-")
            .tempdir_in(temp)
            .unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join(".access")).unwrap();
        let hash = |token: &str| format!("{:x}", Sha256::digest(token.as_bytes()));
        let state = json!({"schema":2,"tenancy":"hosted",
            "organizations":[{"id":"aaaa0001","name":"A","created":"now","suspended":false,"limits":{}},{"id":"bbbb0002","name":"B","created":"now","suspended":false,"limits":{}}],
            "memberships":[{"person":"a","org":"aaaa0001","role":"owner","created":"now"},{"person":"b","org":"bbbb0002","role":"owner","created":"now"},{"person":"reader","org":"aaaa0001","role":"reader","created":"now"}],
            "users":[{"id":"a","name":"A","email":"a@example.test","created":"now","verified":"now","tokens":[{"id":"ta","name":"CLI","created":"now","sha256":hash("token-a"),"org":"aaaa0001"}]},
                     {"id":"b","name":"B","email":"b@example.test","created":"now","verified":"now","tokens":[{"id":"tb","name":"CLI","created":"now","sha256":hash("token-b"),"org":"bbbb0002"}]},
                     {"id":"reader","name":"Reader","email":"reader@example.test","created":"now","verified":"now","tokens":[{"id":"tr","name":"CLI","created":"now","sha256":hash("token-reader"),"org":"aaaa0001"}]}],
            "sessions":[{"sha256":hash("session-b"),"csrf":"csrf-b","user":"b","expires":4102444800u64,"scope":""},{"sha256":hash("session-reader"),"csrf":"csrf-reader","user":"reader","expires":4102444800u64,"scope":""}],
            "acls":{"aaaa0001/secret":{"org":"aaaa0001","owner":"a","visibility":"private","grants":[]},"aaaa0001/orgonly":{"org":"aaaa0001","owner":"a","visibility":"org","grants":[]},"aaaa0001/public":{"org":"aaaa0001","owner":"a","visibility":"public","grants":[]}}});
        fs::write(
            root.join(".access/state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let access = Access::open_hosted(&root).unwrap();
        let storage = FsStorage::new(&root).unwrap().with_hosted(true);
        for slug in ["secret", "orgonly", "public"] {
            storage
                .put_version(StoredReport {
                    slug: format!("aaaa0001/{slug}"),
                    version: "1.0.0".into(),
                    source: "# Report".into(),
                    report_folder: None,
                    html: "<h1>Report</h1>".into(),
                    frontmatter: json!({"title":"Report"}),
                    created_at: "2026-11-11T10:00:00Z".into(),
                    styles: vec![],
                    data: BTreeMap::new(),
                    files: BTreeMap::new(),
                    content_sha256: String::new(),
                })
                .unwrap();
        }
        let state = AppState {
            storage,
            access,
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        Self {
            _dir: dir,
            app: build_app(state.clone()),
            reports: build_report_app(state),
        }
    }
}
async fn request(
    router: &axum::Router,
    method: &str,
    path: &str,
    report_side: bool,
    session: Option<(&str, &str)>,
    token: Option<&str>,
) -> (StatusCode, String) {
    let is_form = path.starts_with("/ui/");
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(
            header::HOST,
            if report_side {
                "reports.test"
            } else {
                "app.test"
            },
        )
        .header(
            header::CONTENT_TYPE,
            if is_form {
                "application/x-www-form-urlencoded"
            } else {
                "application/json"
            },
        );
    if let Some((cookie, _)) = session {
        request = request.header(header::COOKIE, format!("rv_session={cookie}"));
    }
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let body = if is_form {
        format!(
            "csrf={}&visibility=public",
            session.map_or("", |(_, csrf)| csrf)
        )
    } else {
        r#"{"version":"1.0.0","visibility":"private","grants":[]}"#.to_owned()
    };
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn foreign_organization_gets_the_same_answer_for_private_and_missing_reports_on_each_route() {
    let fixture = Fixture::new();
    let (session_status, account) = request(
        &fixture.app,
        "GET",
        "/account",
        false,
        Some(("session-b", "csrf-b")),
        None,
    )
    .await;
    assert_eq!(session_status, StatusCode::OK);
    assert!(
        account.contains("b@example.test"),
        "foreign session was not authenticated"
    );
    let (reader_status, reader_account) = request(
        &fixture.app,
        "GET",
        "/account",
        false,
        Some(("session-reader", "csrf-reader")),
        None,
    )
    .await;
    assert_eq!(reader_status, StatusCode::OK);
    assert!(
        reader_account.contains("reader@example.test"),
        "reader session was not authenticated"
    );
    let (token_status, _) = request(
        &fixture.app,
        "GET",
        "/api/reports",
        false,
        None,
        Some("token-b"),
    )
    .await;
    assert_eq!(
        token_status,
        StatusCode::OK,
        "foreign token was not authenticated"
    );
    let mut checked = 0;
    for spec in hosted_specs().into_iter().filter(|s| {
        s.path.contains("{org}")
            && s.path.contains("{slug}")
            && matches!(
                s.rule,
                Rule::View | Rule::Edit | Rule::Manage | Rule::Share | Rule::Delete | Rule::Session
            )
    }) {
        let concrete = |slug: &str| {
            spec.path
                .replace("{org}", "aaaa0001")
                .replace("{slug}", slug)
                .replace("{version}", "1.0.0")
                .replace("{*path}", "file.csv")
        };
        let report_side = spec.path.starts_with("/r/");
        let is_ui = spec.path.starts_with("/ui/") || spec.path.starts_with("/reports/");
        let router = if report_side {
            &fixture.reports
        } else {
            &fixture.app
        };
        for (label, session, token) in [
            ("foreign", Some(("session-b", "csrf-b")), Some("token-b")),
            (
                "reader",
                Some(("session-reader", "csrf-reader")),
                Some("token-reader"),
            ),
            ("anonymous", None, None),
        ] {
            let session = if is_ui { session } else { None };
            let token = if is_ui { None } else { token };
            let existing = request(
                router,
                spec.method,
                &concrete("secret"),
                report_side,
                session,
                token,
            )
            .await;
            let missing = request(
                router,
                spec.method,
                &concrete("missing"),
                report_side,
                session,
                token,
            )
            .await;
            assert_eq!(
                existing, missing,
                "{label} learns report existence at {} {}",
                spec.method, spec.path
            );
            let denied = matches!(existing.0, StatusCode::NOT_FOUND | StatusCode::SEE_OTHER)
                || (label == "anonymous"
                    && matches!(existing.0, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN));
            assert!(
                denied,
                "{label} got {} at {} {}",
                existing.0, spec.method, spec.path
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 15,
        "hosted report routes were not covered: {checked}"
    );
}

#[tokio::test]
async fn role_and_visibility_are_scoped_to_the_report_organization() {
    let fixture = Fixture::new();
    for (slug, token, expected) in [
        ("secret", "token-a", StatusCode::OK),
        ("secret", "token-reader", StatusCode::NOT_FOUND),
        ("secret", "token-b", StatusCode::NOT_FOUND),
        ("orgonly", "token-reader", StatusCode::OK),
        ("orgonly", "token-b", StatusCode::NOT_FOUND),
        ("public", "token-b", StatusCode::OK),
    ] {
        let (status, _) = request(
            &fixture.app,
            "GET",
            &format!("/api/reports/aaaa0001/{slug}"),
            false,
            None,
            Some(token),
        )
        .await;
        assert_eq!(status, expected, "{slug} with {token}");
    }
}

#[tokio::test]
async fn hosted_sign_in_notice_does_not_reveal_whether_an_address_is_throttled() {
    let fixture = Fixture::new();
    async fn attempt(app: &axum::Router, email: &str) -> (StatusCode, String) {
        let body = format!("email={email}&password=wrong-password");
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ui/sign-in")
                    .header(header::HOST, "app.test")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_owned();
        (response.status(), location)
    }
    let mut known = (StatusCode::OK, String::new());
    for _ in 0..6 {
        known = attempt(&fixture.app, "a%40example.test").await;
    }
    let unknown = attempt(&fixture.app, "missing%40example.test").await;
    assert_eq!(known.0, StatusCode::SEE_OTHER);
    assert_eq!(known, unknown);

    let fresh = Fixture::new();
    let mut throttled_unknown = (StatusCode::OK, String::new());
    for _ in 0..6 {
        throttled_unknown = attempt(&fresh.app, "missing%40example.test").await;
    }
    let fresh_known = attempt(&fresh.app, "a%40example.test").await;
    assert_eq!(throttled_unknown, fresh_known);
}

#[tokio::test]
async fn hosted_session_form_accepts_same_origin_opaque_origin_but_not_forgery() {
    let fixture = Fixture::new();
    async fn create_token(app: &axum::Router, site: &str, csrf: &str) -> StatusCode {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ui/tokens")
                    .header(header::HOST, "app.test")
                    .header(header::ORIGIN, "null")
                    .header("sec-fetch-site", site)
                    .header(header::COOKIE, "rv_session=session-b")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("csrf={csrf}&name=Test")))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }
    assert_eq!(
        create_token(&fixture.app, "cross-site", "csrf-b").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        create_token(&fixture.app, "same-origin", "wrong").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        create_token(&fixture.app, "same-origin", "csrf-b").await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn account_usage_is_scoped_to_the_signed_in_organization() {
    let fixture = Fixture::new();
    let (status, account_a) = request(
        &fixture.app,
        "GET",
        "/account",
        false,
        Some(("session-reader", "csrf-reader")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(account_a.contains("3 of 50 reports · &lt;0.1 MiB of 50 MiB"));
    let (status, account_b) = request(
        &fixture.app,
        "GET",
        "/account",
        false,
        Some(("session-b", "csrf-b")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(account_b.contains("0 of 50 reports · 0 MiB of 50 MiB"));
}
