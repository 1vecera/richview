//! Access control on every route of the report host (FEA-SF-18, FEA-SF-19, FEA-SF-64): the route
//! table is the single source, so these tests walk it instead of a hand-kept list.
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use richview_app::access::{Access, OrgRole, Visibility};
use richview_app::server::{
    AppState, build_app,
    routes::{Rule, specs},
};
use richview_app::storage::FsStorage;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use tower::ServiceExt;

struct Host {
    root: PathBuf,
    app: axum::Router,
    admin: String,
    author: String,
    reader: String,
    other: String,
}

impl Host {
    fn new() -> Self {
        Self::build(true)
    }

    /// People with tokens and no passwords: where no one signs in, hashing would only cost time.
    fn bare() -> Self {
        Self::build(false)
    }

    fn build(passwords: bool) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::current_dir().unwrap().join("tmp").join(format!(
            "access-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let storage = FsStorage::new(root.join("store")).unwrap();
        let access = Access::open(storage.dir(), Visibility::Private, None).unwrap();
        let token = |name: &str, role: OrgRole| {
            let user = access
                .create_user(
                    name,
                    &format!("{name}@example.com"),
                    role,
                    passwords.then_some("correct horse battery"),
                )
                .unwrap();
            access.create_token(&user.id, "test").unwrap().1
        };
        let admin = token("admin", OrgRole::Admin);
        let author = token("author", OrgRole::Author);
        let reader = token("reader", OrgRole::Reader);
        let other = token("other", OrgRole::Author);
        let app = build_app(AppState {
            storage,
            access,
            preview: None,
            public_origin: None,
            report_origin: None,
        });
        Self {
            root,
            app,
            admin,
            author,
            reader,
            other,
        }
    }

    /// The same store behind a server with a trusted public origin.
    fn rebuild_with_origin(&mut self, origin: &str) {
        let storage = FsStorage::new(self.root.join("store")).unwrap();
        let access = Access::open(storage.dir(), Visibility::Private, None).unwrap();
        self.app = build_app(AppState {
            storage,
            access,
            preview: None,
            public_origin: Some(origin.into()),
            report_origin: None,
        });
    }

    fn rebuild_with_quota(&mut self, quota: Option<u64>) {
        let storage = FsStorage::new(self.root.join("store"))
            .unwrap()
            .with_quota(quota);
        let access = Access::open(storage.dir(), Visibility::Private, None).unwrap();
        self.app = build_app(AppState {
            storage,
            access,
            preview: None,
            public_origin: None,
            report_origin: None,
        });
    }

    fn rebuild_with_account_quota(&mut self, quota: u64) {
        let storage = FsStorage::new(self.root.join("store"))
            .unwrap()
            .with_account_quota(Some(quota));
        let access = Access::open(storage.dir(), Visibility::Private, None).unwrap();
        self.app = build_app(AppState {
            storage,
            access,
            preview: None,
            public_origin: None,
            report_origin: None,
        });
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Value,
    ) -> (StatusCode, String) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header(
                "content-type",
                if path.starts_with("/ui/") {
                    "application/x-www-form-urlencoded"
                } else {
                    "application/json"
                },
            )
            .header("host", "127.0.0.1:4711");
        if let Some(token) = token {
            req = req.header("authorization", format!("Bearer {token}"));
        }
        let res = self
            .app
            .clone()
            .oneshot(req.body(Body::from(if path.starts_with("/ui/") {
                "csrf=x&visibility=org&name=n&email=e%40x.io&role=reader&password=passwordpassword&action=disable".to_owned()
            } else {
                body.to_string()
            })).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 32 * 1024 * 1024).await.unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn publish(&self, token: &str, slug: &str) -> StatusCode {
        self.call(
            "POST",
            "/api/reports",
            Some(token),
            json!({
                "slug": slug,
                "new": true,
                "source": "---\ntitle: Secret\ndescription: D\nversion: 1.0.0\n---\n\n# Secret\n",
                "reportFolder": {"root": "t", "source": "report.rmd", "files": ["report.rmd"]},
            }),
        )
        .await
        .0
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn concrete(path: &str) -> String {
    path.replace("{slug}", "secret")
        .replace("{version}", "1.0.0")
        .replace("{id}", "nobody")
        .replace("{token}", "nothing")
        .replace("{name}", "x")
        .replace("{*path}", "a.csv")
}

async fn account_quota_serializes_concurrent_publications_and_is_independent() {
    let probe = Host::bare();
    assert_eq!(
        probe.publish(&probe.author, "quota-one").await,
        StatusCode::OK
    );
    let version = probe.root.join("store/quota-one/1.0.0");
    let bytes = ["source.rmd", "rendered.html", "meta.json", "assets.json"]
        .iter()
        .map(|file| fs::metadata(version.join(file)).unwrap().len())
        .sum::<u64>();
    let mut host = Host::bare();
    host.rebuild_with_account_quota(bytes * 3 / 2);
    let (first, second) = tokio::join!(
        host.publish(&host.author, "quota-one"),
        host.publish(&host.author, "quota-two"),
    );
    assert_eq!(
        usize::from(first == StatusCode::OK) + usize::from(second == StatusCode::OK),
        1
    );
    let refused = if first == StatusCode::OK {
        "quota-two"
    } else {
        "quota-one"
    };
    assert!(!host.root.join("store").join(refused).exists());
    assert_eq!(host.publish(&host.other, refused).await, StatusCode::OK);
    assert_eq!(
        host.publish(&host.other, refused).await,
        StatusCode::OK,
        "identical retry is free"
    );
}

async fn whole_report_delete_is_restricted_and_removes_all_versions() {
    let host = Host::bare();
    assert_eq!(host.publish(&host.author, "owned").await, StatusCode::OK);
    assert_eq!(
        host.call("DELETE", "/api/reports/owned", Some(&host.other), json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        host.call("GET", "/api/reports/owned", Some(&host.author), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        host.call(
            "DELETE",
            "/api/reports/owned",
            Some(&host.author),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    let storage = FsStorage::new(host.root.join("store")).unwrap();
    assert!(storage.list_versions("owned").unwrap().is_empty());
    assert!(storage.is_deleted("owned", "1.0.0").unwrap());
    assert!(!host.root.join("store/owned/1.0.0/source.rmd").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn concurrent_authors_cannot_both_create_the_same_report() {
    let host = Host::bare();
    for n in 0..8 {
        let slug = format!("race-{n}");
        let body = |version: &str| {
            json!({
                "slug": slug,
                "source": format!("---\ntitle: Race\ndescription: D\nversion: {version}\n---\n\n# Race\n"),
                "reportFolder": {"root": "t", "source": "report.rmd", "files": ["report.rmd"]},
            })
        };
        let (first, second) = tokio::join!(
            host.call("POST", "/api/reports", Some(&host.author), body("1.0.0")),
            host.call("POST", "/api/reports", Some(&host.other), body("2.0.0")),
        );
        assert_eq!(
            usize::from(first.0 == StatusCode::OK) + usize::from(second.0 == StatusCode::OK),
            1,
            "exactly one author may create {slug}: {first:?}, {second:?}"
        );
        let (winner, loser, winning_version) = if first.0 == StatusCode::OK {
            (&host.author, &host.other, "1.0.0")
        } else {
            (&host.other, &host.author, "2.0.0")
        };
        let storage = FsStorage::new(host.root.join("store")).unwrap();
        assert_eq!(storage.list_versions(&slug).unwrap(), vec![winning_version]);
        let stored = storage.get(&slug, Some(winning_version)).unwrap().unwrap();
        assert_eq!(
            stored.source,
            body(winning_version)["source"].as_str().unwrap()
        );
        let path = format!("/api/reports/{slug}/versions");
        assert_eq!(
            host.call("GET", &path, Some(winner), json!({})).await.0,
            StatusCode::OK
        );
        assert_eq!(
            host.call("GET", &path, Some(loser), json!({})).await.0,
            StatusCode::NOT_FOUND
        );
    }
    account_quota_serializes_concurrent_publications_and_is_independent().await;
}

#[tokio::test]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn quota_refusal_does_not_reserve_the_slug_for_another_author() {
    let mut host = Host::bare();
    let runtime = richview_app::runtime::require().unwrap();
    let wasm = richview_app::runtime::wasm_bytes().unwrap();
    let quota = (runtime.js.len() + wasm.len() + 2 * 1024 * 1024) as u64;
    host.rebuild_with_quota(Some(quota));
    let source = format!(
        "---\ntitle: Large\ndescription: D\nversion: 1.0.0\n---\n\n{}\n",
        "x".repeat(3 * 1024 * 1024)
    );
    assert_eq!(
        host.call(
            "POST",
            "/api/reports",
            Some(&host.author),
            json!({
                "slug": "quota-refused", "new": true, "source": source,
                "reportFolder": {"root": "t", "source": "report.rmd", "files": ["report.rmd"]}
            })
        )
        .await
        .0,
        StatusCode::INSUFFICIENT_STORAGE
    );
    assert!(!host.root.join("store/quota-refused").exists());
    assert_eq!(
        host.publish(&host.other, "quota-refused").await,
        StatusCode::OK
    );
    assert_eq!(
        host.call(
            "GET",
            "/api/reports/quota-refused/versions",
            Some(&host.other),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    whole_report_delete_is_restricted_and_removes_all_versions().await;
}

#[tokio::test]
async fn anonymous_listing_returns_empty_without_reading_the_store() {
    let host = Host::bare();
    fs::rename(host.root.join("store"), host.root.join("store-unavailable")).unwrap();
    let (status, body) = host.call("GET", "/api/reports", None, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), json!([]));
}

#[tokio::test]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn every_route_refuses_a_stranger_and_an_anonymous_caller_without_revealing_the_report() {
    let host = Host::new();
    assert_eq!(host.publish(&host.author, "secret").await, StatusCode::OK);
    for spec in specs() {
        if matches!(spec.rule, Rule::Public | Rule::Preview | Rule::SignIn) {
            continue;
        }
        let path = concrete(spec.path);
        let reader_view = matches!(spec.rule, Rule::View | Rule::Shared);
        // Anonymous: reads of a private report and writes alike never succeed.
        let (anon, body) = host.call(spec.method, &path, None, json!({})).await;
        assert!(
            matches!(
                anon,
                StatusCode::UNAUTHORIZED
                    | StatusCode::NOT_FOUND
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::SEE_OTHER
            ) || (spec.rule == Rule::Shared && anon == StatusCode::OK),
            "{} {} answered anonymous callers with {anon}: {body}",
            spec.method,
            spec.path
        );
        if spec.rule == Rule::Shared {
            assert!(
                !body.contains("secret"),
                "{} lists a private report",
                spec.path
            );
        }
        // A signed-in stranger cannot see the report: the answer is 404, not 403.
        if matches!(
            spec.rule,
            Rule::View | Rule::Edit | Rule::Manage | Rule::Share | Rule::Delete
        ) && spec.path.contains("{slug}")
            && !spec.path.starts_with("/ui/")
        {
            let (status, _) = host
                .call(spec.method, &path, Some(&host.other), json!({}))
                .await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "{} {}",
                spec.method,
                spec.path
            );
        }
        // Governance is for admins.
        if spec.rule == Rule::Admin {
            let (status, _) = host
                .call(spec.method, &path, Some(&host.author), json!({}))
                .await;
            assert!(
                matches!(status, StatusCode::FORBIDDEN | StatusCode::SEE_OTHER),
                "{} {} let an author in: {status}",
                spec.method,
                spec.path
            );
        }
        let _ = reader_view;
    }
}

#[tokio::test]
async fn the_router_serves_exactly_the_route_table() {
    let host = Host::new();
    for spec in specs() {
        if spec.rule == Rule::Preview {
            let (status, _) = host
                .call(spec.method, &concrete(spec.path), None, json!({}))
                .await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "{} is mounted outside serve",
                spec.path
            );
            continue;
        }
        let other = if spec.method == "PATCH" {
            "TRACE"
        } else {
            "PATCH"
        };
        let (status, _) = host
            .call(other, &concrete(spec.path), Some(&host.admin), json!({}))
            .await;
        let listed = specs()
            .iter()
            .any(|s| s.path == spec.path && s.method == other);
        if !listed {
            assert_eq!(
                status,
                StatusCode::METHOD_NOT_ALLOWED,
                "{other} {}",
                spec.path
            );
        }
    }
}

#[tokio::test]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn sharing_widens_what_a_reader_sees_and_never_what_a_reader_may_change() {
    let host = Host::new();
    assert_eq!(host.publish(&host.author, "secret").await, StatusCode::OK);
    let (status, _) = host
        .call("GET", "/r/secret", Some(&host.reader), json!({}))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = host
        .call("GET", "/api/reports", Some(&host.reader), json!({}))
        .await;
    assert!(!list.contains("secret"));

    let (status, _) = host
        .call(
            "PUT",
            "/api/reports/secret/access",
            Some(&host.reader),
            json!({"visibility": "public"}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a stranger cannot even learn the report exists"
    );

    let (status, _) = host
        .call(
            "PUT",
            "/api/reports/secret/access",
            Some(&host.author),
            json!({"visibility": "org"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = host
        .call("GET", "/r/secret", Some(&host.reader), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = host
        .call(
            "DELETE",
            "/api/reports/secret/v/1.0.0",
            Some(&host.reader),
            json!({}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a reader who can see it may not delete it"
    );
    let (status, _) = host
        .call(
            "DELETE",
            "/api/reports/secret/v/1.0.0",
            Some(&host.other),
            json!({}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "org visibility grants view only"
    );
    assert_eq!(
        host.publish(&host.reader, "mine").await,
        StatusCode::FORBIDDEN
    );

    // Public means anonymous callers read it, but still cannot write.
    host.call(
        "PUT",
        "/api/reports/secret/access",
        Some(&host.author),
        json!({"visibility": "public"}),
    )
    .await;
    let (status, _) = host.call("GET", "/r/secret", None, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = host
        .call("DELETE", "/api/reports/secret/v/1.0.0", None, json!({}))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn hostile_input_reaches_no_report_and_no_secret_in_a_url() {
    let host = Host::new();
    assert_eq!(host.publish(&host.author, "secret").await, StatusCode::OK);
    for path in [
        "/r/..%2F..%2Fetc%2Fpasswd",
        "/r/secret/v/..%2F1.0.0/files/x",
        "/r/secret/v/1.0.0/files/..%2F..%2F.access%2Fstate.json",
        "/raw/.access",
        "/r/%2Eaccess",
    ] {
        let (status, body) = host.call("GET", path, Some(&host.admin), json!({})).await;
        assert!(
            matches!(status, StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST),
            "{path} answered {status}"
        );
        assert!(!body.contains("argon2"), "{path} leaked access state");
    }
    // A sign-in cannot be sent on to a path that climbs out of the report area, however the dots
    // are written: a browser resolves the escaped form to the same address.
    for next in [
        "/r/%2e%2e/api/reports",
        "/r/q4/%2e%2e/%2e%2e/ui/users",
        "/r/q4/%2E%2e/x",
        "/r/a%2f..%2f..%2fapi",
        "/r//evil.test",
        "/r/a%5c..%5cb",
    ] {
        let (_, headers) = sign_in_with_next(&host, next).await;
        let to = headers["location"].to_str().unwrap();
        assert_eq!(to, "/", "{next} was followed to {to}");
    }
    // A report address with a query and a file name is followed as written.
    let (_, headers) = sign_in_with_next(&host, "/r/q4/files/a%20b.csv?region=eu&year=2026").await;
    assert_eq!(
        headers["location"].to_str().unwrap(),
        "/r/q4/files/a%20b.csv?region=eu&year=2026"
    );
    // A failed attempt keeps the whole destination, query included, for the next try.
    let res = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ui/sign-in")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("host", "127.0.0.1:4711")
                .body(Body::from(
                    "email=author%40example.com&password=wrong+wrong+wrong&next=%2Fr%2Fq4%3Fregion%3Deu%26year%3D2026",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let back = res.headers()["location"].to_str().unwrap().to_owned();
    assert!(
        back.starts_with("/?next=%2Fr%2Fq4%3Fregion%3Deu%26year%3D2026")
            || back.starts_with("/?next=/r/q4%3Fregion%3Deu%26year%3D2026"),
        "{back}"
    );
    // A token in the query string is not a credential.
    let uri = format!("/api/reports?token={}", host.admin);
    let (_, body) = host.call("GET", &uri, None, json!({})).await;
    assert!(
        !body.contains("secret"),
        "a token in the URL authenticated the caller"
    );
}

/// A good form sign-in that asks to return to `next`; the status and headers of the answer.
async fn sign_in_with_next(host: &Host, next: &str) -> (StatusCode, axum::http::HeaderMap) {
    let form = format!(
        "email=author%40example.com&password=correct+horse+battery&next={}",
        next.replace('%', "%25").replace('&', "%26")
    );
    let res = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ui/sign-in")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("host", "127.0.0.1:4711")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    (res.status(), res.headers().clone())
}

#[tokio::test]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn a_cookie_session_needs_the_csrf_header_and_the_same_origin() {
    let host = Host::new();
    let (status, body) = {
        let res = host
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header("content-type", "application/json")
                    .header("host", "127.0.0.1:4711")
                    .body(Body::from(
                        json!({"email": "author@example.com", "password": "correct horse battery"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = res.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        let csrf = serde_json::from_slice::<Value>(&bytes).unwrap()["csrf"]
            .as_str()
            .unwrap()
            .to_owned();
        let publish = |csrf: Option<&str>, origin: &str| {
            let mut req = Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header("content-type", "application/json")
                .header("host", "127.0.0.1:4711")
                .header("origin", origin)
                .header("cookie", cookie.clone());
            if let Some(csrf) = csrf {
                req = req.header("x-richview-csrf", csrf);
            }
            req.body(Body::from(
                json!({"slug":"c","new":true,"source":"---\ntitle: C\ndescription: D\nversion: 1.0.0\n---\n\n# C\n","reportFolder":{"root":"t","source":"report.rmd","files":["report.rmd"]}}).to_string(),
            ))
            .unwrap()
        };
        let no_csrf = host
            .app
            .clone()
            .oneshot(publish(None, "http://127.0.0.1:4711"))
            .await
            .unwrap();
        let cross = host
            .app
            .clone()
            .oneshot(publish(Some(&csrf), "http://evil.example"))
            .await
            .unwrap();
        assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);
        assert_eq!(cross.status(), StatusCode::FORBIDDEN);
        let ok = host
            .app
            .clone()
            .oneshot(publish(Some(&csrf), "http://127.0.0.1:4711"))
            .await
            .unwrap();
        (ok.status(), String::new())
    };
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn browser_error_pages_use_the_signed_out_chrome_without_changing_api_errors() {
    let host = Host::new();
    let browser = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/missing")
                .header("accept", "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(browser.status(), StatusCode::NOT_FOUND);
    assert!(
        browser.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    let body = String::from_utf8(
        to_bytes(browser.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    // The signed-out chrome: the mark home above the card, the legal links below.
    assert!(
        body.contains(
            "<header class=\"auth-head\"><a class=\"auth-brand\" href=\"https://richview.ai/\""
        ) && body.contains("<footer class=\"auth-foot\">")
            && body.contains("Not found")
    );
    let wrong_method = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/health")
                .header("accept", "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert!(
        wrong_method.headers()["allow"]
            .to_str()
            .unwrap()
            .contains("GET")
    );
    let challenge = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/governance")
                .header("accept", "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(challenge.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(challenge.headers()["www-authenticate"], "Bearer");
    let api = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/governance/users")
                .header("accept", "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
    assert!(
        api.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
}

#[tokio::test]
async fn cowork_site_navigation_points_to_the_main_website() {
    let host = Host::new();
    let (_, body) = host.call("GET", "/", None, json!({})).await;
    assert!(body.contains("href=\"https://richview.ai/\" aria-label=\"RichView home\""));
    for (href, label) in [
        ("https://richview.ai/docs/", "Docs"),
        ("https://richview.ai/docs/charts", "Gallery"),
        (
            "https://richview.ai/showcase/openai-vs-anthropic/",
            "Flagship report",
        ),
        ("https://richview.ai/docs/charts", "Chart gallery"),
        ("https://richview.ai/impressum", "Impressum"),
        ("https://richview.ai/privacy", "Privacy"),
        ("https://richview.ai/licenses", "Font licences"),
    ] {
        assert!(
            body.contains(&format!("href=\"{href}\">{label}</a>")),
            "missing destination for {label}"
        );
    }
    let header = body
        .split("<header class=\"nav\">")
        .nth(1)
        .unwrap()
        .split("</header>")
        .next()
        .unwrap();
    let footer = body
        .split("<footer class=\"foot\">")
        .nth(1)
        .unwrap()
        .split("</footer>")
        .next()
        .unwrap();
    assert!(!header.contains("href=\"/"));
    assert!(!footer.contains("href=\"/"));
}

#[tokio::test]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn the_pages_show_only_what_the_person_may_see() {
    let host = Host::new();
    assert_eq!(host.publish(&host.author, "secret").await, StatusCode::OK);
    let (status, body) = host.call("GET", "/", None, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("action=\"/ui/sign-in\"") && !body.contains("secret"));
    let (status, _) = host.call("GET", "/governance", None, json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = host
        .call("GET", "/governance", Some(&host.reader), json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = host
        .call("GET", "/governance", Some(&host.admin), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("author@example.com"));
}

#[tokio::test]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn the_share_card_follows_the_report_visibility_and_only_public_pages_name_it() {
    let mut host = Host::new();
    host.rebuild_with_origin("https://reports.example");
    assert_eq!(host.publish(&host.author, "secret").await, StatusCode::OK);
    let (status, _) = host
        .call("GET", "/r/secret/share.png", None, json!({}))
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a private card is as hidden as the report"
    );
    let (status, page) = host
        .call("GET", "/r/secret", Some(&host.author), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!page.contains("og:image"));
    host.call(
        "PUT",
        "/api/reports/secret/access",
        Some(&host.author),
        json!({"visibility": "public"}),
    )
    .await;
    let (_, page) = host.call("GET", "/r/secret", None, json!({})).await;
    assert!(page.contains("content=\"https://reports.example/r/secret/v/1.0.0/share.png\""));
    assert!(page.contains("summary_large_image"));
    assert!(
        page.contains("og:image:alt\" content=\"Secret"),
        "the preview image has alt text"
    );
    let (status, png) = host
        .call("GET", "/r/secret/v/1.0.0/share.png", None, json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(png.contains("PNG"), "not a PNG");
    company_logo_share_card_visibility_and_style().await;
}

async fn company_logo_share_card_visibility_and_style() {
    let host = Host::new();
    for (slug, style) in [
        (
            "plain",
            "preset: air\ncolors: {brand: '#0B6E69'}\ncompany: {name: Example Co}\n",
        ),
        (
            "company",
            "preset: air\ncolors: {brand: '#0B6E69'}\ncompany: {name: Example Co, logo: logo.png}\n",
        ),
        (
            "company-sleek",
            "preset: sleek\ncolors: {brand: '#0B6E69'}\ncompany: {name: Example Co, logo: logo.png}\n",
        ),
    ] {
        let (status, body) = host.call(
            "POST",
            "/api/reports",
            Some(&host.author),
            json!({
                "slug": slug, "new": true,
                "source": "---\ntitle: Secret\ndescription: D\nversion: 1.0.0\n---\n\n# Secret\n",
                "styles": [style],
                "data": {"logo.png": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGMQERH5D8IMMAYAJtwE7SIJj3sAAAAASUVORK5CYII="},
                "reportFolder": {"root": "t", "source": "report.rmd", "files": ["report.rmd", "logo.png"]},
            }),
        ).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (status, _) = host
        .call("GET", "/r/company/share.png", None, json!({}))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let png = |slug: &str| {
        let request = Request::builder()
            .uri(format!("/r/{slug}/share.png"))
            .header("authorization", format!("Bearer {}", host.author))
            .header("host", "127.0.0.1:4711")
            .body(Body::empty())
            .unwrap();
        host.app.clone().oneshot(request)
    };
    let plain = png("plain").await.unwrap();
    let branded = png("company").await.unwrap();
    let sleek = png("company-sleek").await.unwrap();
    assert_eq!(plain.status(), StatusCode::OK);
    assert_eq!(branded.status(), StatusCode::OK);
    assert_eq!(sleek.status(), StatusCode::OK);
    let plain = to_bytes(plain.into_body(), 32 * 1024 * 1024).await.unwrap();
    let branded = to_bytes(branded.into_body(), 32 * 1024 * 1024)
        .await
        .unwrap();
    let sleek = to_bytes(sleek.into_body(), 32 * 1024 * 1024).await.unwrap();
    assert!(
        plain != branded,
        "the logo must appear in the stored report's share image"
    );
    assert!(
        branded != sleek,
        "the share card follows the company base style"
    );
    let (status, _) = host
        .call(
            "PUT",
            "/api/reports/company/access",
            Some(&host.author),
            json!({"visibility":"public"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let public = host
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/r/company/share.png")
                .header("host", "127.0.0.1:4711")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(public.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(public.into_body(), 32 * 1024 * 1024)
            .await
            .unwrap(),
        branded
    );
}

#[tokio::test]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn a_font_in_published_data_must_be_a_woff2_file_and_nothing_else() {
    let host = Host::new();
    let publish = |font: &str| {
        json!({
            "slug": "fonts", "new": true,
            "source": "---\ntitle: F\ndescription: D\nversion: 1.0.0\n---\n\n# F\n",
            "reportFolder": {"root": "t", "source": "report.rmd", "files": ["report.rmd"]},
            "data": {"brand.woff2": font},
        })
    };
    for hostile in [
        "data:font/woff2;base64,AAAA)}</style><script>alert(1)</script>",
        "data:font/woff2;base64,AAAAAAAA",
        "data:font/ttf;base64,d09GMg==",
    ] {
        let (status, _) = host
            .call("POST", "/api/reports", Some(&host.author), publish(hostile))
            .await;
        assert!(status.is_client_error(), "{hostile} was accepted: {status}");
    }
    let (status, body) = host
        .call(
            "POST",
            "/api/reports",
            Some(&host.author),
            publish("data:font/woff2;base64,d09GMg=="),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

mod split {
    use super::*;
    use richview_app::server::build_report_app;

    struct Pair {
        host: Host,
        app: axum::Router,
        reports: axum::Router,
    }

    fn pair() -> Pair {
        let mut host = Host::new();
        let state = |host: &Host| {
            let storage = FsStorage::new(host.root.join("store")).unwrap();
            AppState {
                access: Access::open(storage.dir(), Visibility::Private, None).unwrap(),
                storage,
                preview: None,
                public_origin: Some("http://app.test".into()),
                report_origin: Some("http://reports.test".into()),
            }
        };
        // One state behind both listeners, as the server runs them.
        let shared = state(&host);
        host.app = build_app(shared.clone());
        Pair {
            app: build_app(shared.clone()),
            reports: build_report_app(shared),
            host,
        }
    }

    async fn get(
        router: &axum::Router,
        site: &str,
        path: &str,
        cookie: Option<&str>,
    ) -> (StatusCode, axum::http::HeaderMap, String) {
        let mut req = Request::builder()
            .method("GET")
            .uri(path)
            .header("host", site);
        if let Some(cookie) = cookie {
            req = req.header("cookie", cookie);
        }
        let res = router
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (status, headers) = (res.status(), res.headers().clone());
        let bytes = to_bytes(res.into_body(), 1 << 24).await.unwrap();
        (
            status,
            headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    async fn sign_in(router: &axum::Router) -> String {
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header("content-type", "application/json")
                    .header("host", "app.test")
                    .body(Body::from(
                        json!({"email": "reader@example.com", "password": "correct horse battery"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        res.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn report_pages_live_on_their_own_origin_and_the_app_cookie_never_reaches_them() {
        let p = pair();
        assert_eq!(
            p.host.publish(&p.host.author, "secret").await,
            StatusCode::OK
        );
        // Share with the reader's organisation so the signed-in reader may view it.
        let (status, _) = p
            .host
            .call(
                "PUT",
                "/api/reports/secret/access",
                Some(&p.host.author),
                json!({"visibility": "org"}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);

        // The app origin hands report paths over; it serves none itself.
        let (status, headers, _) = get(&p.app, "app.test", "/r/secret", None).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(headers["location"], "http://reports.test/r/secret");

        // The report origin serves nothing but reports.
        for path in [
            "/api/reports",
            "/governance",
            "/",
            "/api/session",
            "/ui/sign-in",
        ] {
            let (status, _, _) = get(&p.reports, "reports.test", path, None).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path} on the report origin");
        }

        // A signed-in reader is handed a ticket, which opens that one report once.
        let session = sign_in(&p.app).await;
        let (_, headers, _) = get(&p.app, "app.test", "/r/secret", Some(&session)).await;
        let target = headers["location"].to_str().unwrap().to_owned();
        let ticket_path = target
            .strip_prefix("http://reports.test")
            .unwrap()
            .to_owned();
        assert!(ticket_path.contains("?grant="));

        // The app cookie alone does not open a private report on the report origin.
        let (status, _, _) = get(&p.reports, "reports.test", "/r/secret", Some(&session)).await;
        assert_eq!(
            status,
            StatusCode::SEE_OTHER,
            "sent to the app to sign in or be handed over"
        );

        // A ticket for another report is spent without opening anything.
        let other = ticket_path.replace("/r/secret", "/r/other");
        let (_, headers, _) = get(&p.reports, "reports.test", &other, None).await;
        assert!(headers.get("set-cookie").is_none());
        let (_, headers, _) = get(&p.reports, "reports.test", &ticket_path, None).await;
        assert!(
            headers.get("set-cookie").is_none(),
            "the ticket was already spent"
        );

        // A fresh ticket becomes a cookie scoped to the report's path.
        let (_, headers, _) = get(&p.app, "app.test", "/r/secret", Some(&session)).await;
        let ticket_path = headers["location"]
            .to_str()
            .unwrap()
            .strip_prefix("http://reports.test")
            .unwrap()
            .to_owned();
        let (status, headers, _) = get(&p.reports, "reports.test", &ticket_path, None).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(headers["location"], "/r/secret");
        let cookie = headers["set-cookie"].to_str().unwrap().to_owned();
        assert!(cookie.contains("HttpOnly") && cookie.contains("Path=/r/secret"));
        let pair = cookie.split(';').next().unwrap();
        let (status, _, page) = get(&p.reports, "reports.test", "/r/secret", Some(pair)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(page.contains("Secret"));

        // That cookie opens nothing else, and the app origin ignores it.
        let (status, _, _) = get(&p.reports, "reports.test", "/r/other", Some(pair)).await;
        assert_eq!(
            status,
            StatusCode::SEE_OTHER,
            "no session there: back to the app"
        );
        let (status, _, body) = get(&p.app, "app.test", "/api/session", Some(pair)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!body.contains("reader@example.com"));
    }
}

#[tokio::test]
#[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
async fn the_authors_local_path_is_not_stored() {
    let host = Host::new();
    let (status, _) = host
        .call(
            "POST",
            "/api/reports",
            Some(&host.author),
            json!({
                "slug": "paths", "new": true,
                "source": "---\ntitle: P\ndescription: D\nversion: 1.0.0\n---\n\n# P\n",
                "reportFolder": {"root": "/srv/someone/private/folder", "source": "report.rmd", "files": ["report.rmd"]},
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, bundle) = host
        .call(
            "GET",
            "/api/reports/paths/v/1.0.0",
            Some(&host.author),
            json!({}),
        )
        .await;
    assert!(!bundle.contains("/srv/someone"), "{bundle}");
    let stored = std::fs::read_dir(host.root.join("store")).unwrap().count();
    assert!(stored > 0);
    fn scan(dir: &std::path::Path) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan(&path);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                assert!(
                    !text.contains("/srv/someone"),
                    "{} keeps the local path",
                    path.display()
                );
            }
        }
    }
    scan(&host.root.join("store"));
}

#[tokio::test]
async fn validate_and_check_answer_the_cli_envelopes_over_http_and_store_nothing() {
    let host = Host::new();
    let folder = json!({"root": "r", "source": "report.rmd", "files": ["report.rmd"]});
    let good = "---\ntitle: Good\ndescription: D\nversion: 1.0.0\n---\n\n# Good\n";
    let (status, body) = host
        .call(
            "POST",
            "/api/validate",
            Some(&host.author),
            json!({"source": good, "reportFolder": folder, "publish": true}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    assert!(
        value["ok"].is_boolean() && value["diagnostics"].is_array() && value["blocks"].is_number()
    );

    let (status, body) = host
        .call(
            "POST",
            "/api/validate",
            Some(&host.author),
            json!({"source": "```chart\ntype: invalid\n```", "reportFolder": folder}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["ok"], false);
    assert!(!value["diagnostics"].as_array().unwrap().is_empty());

    let (status, body) = host
        .call(
            "POST",
            "/api/check",
            Some(&host.author),
            json!({"source": good, "reportFolder": folder}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    assert!(value["charts"].is_array());

    assert_eq!(
        host.call("POST", "/api/validate", None, json!({"source": good}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        host.call(
            "POST",
            "/api/validate",
            Some(&host.reader),
            json!({"source": good})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, list) = host
        .call("GET", "/api/reports", Some(&host.admin), json!({}))
        .await;
    assert_eq!(list, "[]", "validate and check store nothing");
}

#[tokio::test]
async fn http_validate_and_check_report_missing_and_escaping_files_once() {
    let host = Host::new();
    let folder = json!({"root": "r", "source": "report.rmd", "files": ["report.rmd"]});
    for (data, code) in [
        ("data/absent.csv", "report.asset.missing"),
        ("../outside.csv", "report.asset.escape"),
    ] {
        let source = format!("```table\ndata: {{csv: {data}}}\n```\n");
        for path in ["/api/validate", "/api/check"] {
            let (status, body) = host
                .call(
                    "POST",
                    path,
                    Some(&host.author),
                    json!({"source": source, "reportFolder": folder}),
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{path} {data}: {body}");
            let value: Value = serde_json::from_str(&body).unwrap();
            let hits = value["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|d| d["code"] == code)
                .count();
            assert_eq!(hits, 1, "{path} {data}: {body}");
        }
    }
}

#[tokio::test]
async fn a_folder_copied_from_a_mac_is_one_report_over_http_and_its_sidecars_are_no_files() {
    // An inventory some other client built still lists what a Mac leaves behind: an AppleDouble
    // sidecar `._<name>` beside each file, and a `__MACOSX` folder of them.
    let host = Host::new();
    let folder = json!({"root": "q3", "source": "report.rmd", "files": [
        "report.rmd", "._report.rmd", "data/sales.csv", "data/._sales.csv", "__MACOSX/._report.rmd"
    ]});
    let good = "---\ntitle: Good\ndescription: D\nversion: 1.0.0\n---\n\n# Good\n";
    let naming = "---\ntitle: Good\ndescription: D\nversion: 1.0.0\n---\n\n```table\ndata: {csv: data/._sales.csv}\n```\n";
    for path in ["/api/validate", "/api/check"] {
        let (status, body) = host
            .call(
                "POST",
                path,
                Some(&host.author),
                json!({"source": good, "reportFolder": folder}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        let value: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["ok"], true, "{path}: {value}");
        // A report that names a sidecar finds no file there, whatever the inventory lists.
        let (status, body) = host
            .call(
                "POST",
                path,
                Some(&host.author),
                json!({"source": naming, "reportFolder": folder}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        let value: Value = serde_json::from_str(&body).unwrap();
        let hits = value["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| {
                d["code"] == "report.asset.missing"
                    && d["message"].as_str().is_some_and(|m| m.contains("sidecar"))
            })
            .count();
        assert_eq!(hits, 1, "{path}: {body}");
    }
}

mod split_hardening {
    use super::*;
    use richview_app::server::build_report_app;

    struct Pair {
        host: Host,
        app: axum::Router,
        reports: axum::Router,
    }

    fn pair() -> Pair {
        let mut host = Host::new();
        let storage = FsStorage::new(host.root.join("store")).unwrap();
        let shared = AppState {
            access: Access::open(storage.dir(), Visibility::Private, None).unwrap(),
            storage,
            preview: None,
            public_origin: Some("http://app.test".into()),
            report_origin: Some("http://reports.test".into()),
        };
        host.app = build_app(shared.clone());
        Pair {
            app: build_app(shared.clone()),
            reports: build_report_app(shared),
            host,
        }
    }

    async fn send(
        router: &axum::Router,
        method: &str,
        site: &str,
        path: &str,
        extra: &[(&str, &str)],
    ) -> (StatusCode, axum::http::HeaderMap, String) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("host", site);
        for (name, value) in extra {
            req = req.header(*name, *value);
        }
        let res = router
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (status, headers) = (res.status(), res.headers().clone());
        let bytes = to_bytes(res.into_body(), 1 << 24).await.unwrap();
        (
            status,
            headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    async fn login(router: &axum::Router, email: &str) -> (String, String) {
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header("content-type", "application/json")
                    .header("host", "app.test")
                    .body(Body::from(
                        json!({"email": email, "password": "correct horse battery"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let cookie = res.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
        let csrf = serde_json::from_slice::<Value>(&bytes).unwrap()["csrf"]
            .as_str()
            .unwrap()
            .to_owned();
        (cookie, csrf)
    }

    async fn shared_report(p: &Pair) {
        assert_eq!(
            p.host.publish(&p.host.author, "secret").await,
            StatusCode::OK
        );
        p.host
            .call(
                "PUT",
                "/api/reports/secret/access",
                Some(&p.host.author),
                json!({"visibility": "org"}),
            )
            .await;
    }

    fn ticket_of(headers: &axum::http::HeaderMap) -> Option<String> {
        headers
            .get("location")?
            .to_str()
            .unwrap()
            .split_once("grant=")
            .map(|(_, t)| t.to_owned())
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn signing_out_ends_the_read_sessions_it_handed_over() {
        let p = pair();
        shared_report(&p).await;
        let (cookie, csrf) = login(&p.app, "reader@example.com").await;
        let (_, headers, _) = send(
            &p.app,
            "GET",
            "app.test",
            "/r/secret",
            &[("cookie", &cookie)],
        )
        .await;
        let path = headers["location"]
            .to_str()
            .unwrap()
            .strip_prefix("http://reports.test")
            .unwrap()
            .to_owned();
        let (_, headers, _) = send(&p.reports, "GET", "reports.test", &path, &[]).await;
        let read = headers["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let (status, _, _) = send(
            &p.reports,
            "GET",
            "reports.test",
            "/r/secret",
            &[("cookie", &read)],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, _) = send(
            &p.app,
            "DELETE",
            "app.test",
            "/api/session",
            &[("cookie", &cookie), ("x-richview-csrf", &csrf)],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, _) = send(
            &p.reports,
            "GET",
            "reports.test",
            "/r/secret",
            &[("cookie", &read)],
        )
        .await;
        assert_ne!(
            status,
            StatusCode::OK,
            "the read session outlived the sign-in it came from"
        );
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn a_ticket_goes_only_to_the_persons_own_navigation_for_a_report_they_may_view() {
        let p = pair();
        shared_report(&p).await;
        assert_eq!(
            p.host.publish(&p.host.author, "hidden").await,
            StatusCode::OK
        );
        let (cookie, _) = login(&p.app, "reader@example.com").await;
        let get = |path: &'static str, extra: Vec<(&'static str, &'static str)>| {
            let (app, cookie) = (p.app.clone(), cookie.clone());
            async move {
                let mut headers = vec![("cookie", cookie.as_str())];
                headers.extend(extra);
                let owned: Vec<(String, String)> = headers
                    .iter()
                    .map(|(a, b)| (a.to_string(), b.to_string()))
                    .collect();
                let refs: Vec<(&str, &str)> = owned
                    .iter()
                    .map(|(a, b)| (a.as_str(), b.as_str()))
                    .collect();
                send(&app, "GET", "app.test", path, &refs).await
            }
        };
        let (_, h, _) = get(
            "/r/secret",
            vec![
                ("sec-fetch-site", "same-origin"),
                ("sec-fetch-dest", "document"),
            ],
        )
        .await;
        assert!(ticket_of(&h).is_some());
        let (_, h, _) = get(
            "/r/secret",
            vec![("sec-fetch-site", "same-site"), ("sec-fetch-dest", "empty")],
        )
        .await;
        assert!(
            ticket_of(&h).is_none(),
            "a script on a report page must not earn a ticket"
        );
        let (_, h, _) = get(
            "/r/secret",
            vec![
                ("sec-fetch-site", "same-origin"),
                ("sec-fetch-dest", "iframe"),
            ],
        )
        .await;
        assert!(ticket_of(&h).is_none(), "a frame must not earn a ticket");
        let (_, h, _) = get("/r/hidden", vec![]).await;
        assert!(
            ticket_of(&h).is_none(),
            "no ticket for a report the person cannot view"
        );
        // The pool per person is small.
        let mut issued = 1;
        for _ in 0..20 {
            let (_, h, _) = get("/r/secret", vec![]).await;
            issued += usize::from(ticket_of(&h).is_some());
        }
        assert!(issued <= 8, "{issued} live tickets for one person");
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn the_listener_not_the_host_header_decides_which_cookie_counts() {
        let p = pair();
        shared_report(&p).await;
        let (cookie, _) = login(&p.app, "reader@example.com").await;
        // The app cookie never opens a report on the report listener, whatever Host says.
        for host in ["reports.test", "app.test", "localhost:1"] {
            let (status, _, _) =
                send(&p.reports, "GET", host, "/r/secret", &[("cookie", &cookie)]).await;
            assert_ne!(status, StatusCode::OK, "Host {host}");
        }
        // A client cannot claim to be on the report listener to the app.
        let (_, _, body) = send(
            &p.app,
            "GET",
            "app.test",
            "/api/session",
            &[("cookie", &cookie), ("x-rv-listener", "report")],
        )
        .await;
        assert!(body.contains("reader@example.com"), "{body}");
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn a_report_address_without_a_session_leads_to_sign_in_not_a_dead_end() {
        let p = pair();
        shared_report(&p).await;
        let (status, headers, _) = send(&p.reports, "GET", "reports.test", "/r/secret", &[]).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(headers["location"], "http://app.test/r/secret?rv=signin");
        let (status, headers, _) =
            send(&p.app, "GET", "app.test", "/r/secret?rv=signin", &[]).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert!(
            headers["location"].to_str().unwrap().starts_with("/?m="),
            "{:?}",
            headers["location"]
        );
        // Signed in, the same address hands the reader over with a ticket.
        let (cookie, _) = login(&p.app, "reader@example.com").await;
        let (_, headers, _) = send(
            &p.app,
            "GET",
            "app.test",
            "/r/secret?rv=signin",
            &[("cookie", &cookie)],
        )
        .await;
        assert!(
            headers["location"]
                .to_str()
                .unwrap()
                .starts_with("http://reports.test/r/secret?grant=")
        );
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn a_signed_in_reader_never_loops_between_the_origins() {
        let p = pair();
        shared_report(&p).await;
        let (cookie, _) = login(&p.app, "reader@example.com").await;
        let sent_back = [("cookie", cookie.as_str()), ("sec-fetch-site", "same-site")];
        // A report they cannot open, or that is not there, answers one 404.
        for path in ["/r/hidden?rv=signin", "/r/missing?rv=signin", "/r/hidden"] {
            let (status, _, _) = send(&p.app, "GET", "app.test", path, &sent_back).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        }
        // Sent through the report origin by a link, they get one step whose click earns the ticket.
        let (status, headers, body) =
            send(&p.app, "GET", "app.test", "/r/secret?rv=signin", &sent_back).await;
        assert_eq!(status, StatusCode::OK, "{headers:?}");
        assert!(body.contains("href=\"/r/secret?rv=open\""), "{body}");
        let own = [
            ("cookie", cookie.as_str()),
            ("sec-fetch-site", "same-origin"),
            ("sec-fetch-dest", "document"),
        ];
        let (status, headers, _) = send(&p.app, "GET", "app.test", "/r/secret", &own).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert!(headers["location"].to_str().unwrap().contains("grant="));
        // The click on that step is the person's own navigation and earns the ticket.
        let (_, headers, _) = send(&p.app, "GET", "app.test", "/r/secret?rv=open", &own).await;
        assert!(ticket_of(&headers).is_some(), "{headers:?}");
        // A link from another site arrives without the Strict cookie: the same step, and its
        // click leads a signed-out person to sign-in and back.
        let cross = [
            ("sec-fetch-site", "cross-site"),
            ("sec-fetch-dest", "document"),
        ];
        let (status, _, body) =
            send(&p.app, "GET", "app.test", "/r/secret?rv=signin", &cross).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("rv=open"), "{body}");
        let (_, headers, _) = send(
            &p.app,
            "GET",
            "app.test",
            "/r/secret?rv=open",
            &[
                ("sec-fetch-site", "same-origin"),
                ("sec-fetch-dest", "document"),
            ],
        )
        .await;
        assert!(
            headers["location"]
                .to_str()
                .unwrap()
                .contains("next=/r/secret"),
            "{:?}",
            headers["location"]
        );
        // Sign-in returns to the same address, query and percent-encoded file names included.
        let (_, headers, _) = send(
            &p.app,
            "GET",
            "app.test",
            "/r/secret/v/1.0.0/files/a%20b.csv?rv=signin&t=1",
            &[],
        )
        .await;
        assert!(
            headers["location"]
                .to_str()
                .unwrap()
                .ends_with("&next=/r/secret/v/1.0.0/files/a%2520b.csv%3Ft%3D1"),
            "{:?}",
            headers["location"]
        );
        // A sign-in announcing a huge body is refused on the header.
        let (status, _, _) = send(
            &p.app,
            "POST",
            "app.test",
            "/api/session",
            &[("content-length", "31457280")],
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        // With every ticket in use, an own navigation gets a page, not a redirect back.
        let mut last = StatusCode::SEE_OTHER;
        for _ in 0..20 {
            last = send(&p.app, "GET", "app.test", "/r/secret", &own).await.0;
            if last != StatusCode::SEE_OTHER {
                break;
            }
        }
        assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
        // Signing in returns to the report.
        let (_, headers, _) = send(&p.app, "GET", "app.test", "/r/secret?rv=signin", &[]).await;
        assert!(
            headers["location"]
                .to_str()
                .unwrap()
                .ends_with("&next=/r/secret"),
            "{:?}",
            headers["location"]
        );
    }

    /// What a browser does to open a report on the split server: the app address with the
    /// person's cookie, then the report origin with the ticket, then the page with its read
    /// session. The last status is the answer; more than five hops is a loop.
    async fn open_in_browser(p: &Pair, cookie: Option<&str>, slug: &str) -> StatusCode {
        let mut jar: Vec<String> = Vec::new();
        let mut clicked = false;
        let mut site = "app.test";
        let mut path = format!("/r/{slug}");
        for _ in 0..5 {
            // Typed in at first; every later hop follows a redirect from another site.
            let first = (path == format!("/r/{slug}") && site == "app.test" && jar.is_empty())
                || std::mem::take(&mut clicked);
            let mut extra: Vec<(&str, String)> = vec![
                (
                    "sec-fetch-site",
                    if first { "same-origin" } else { "cross-site" }.into(),
                ),
                ("sec-fetch-dest", "document".into()),
            ];
            if site == "app.test" {
                if let Some(cookie) = cookie {
                    extra.push(("cookie", cookie.to_owned()));
                }
            } else if !jar.is_empty() {
                extra.push(("cookie", jar.join("; ")));
            }
            let extra: Vec<(&str, &str)> = extra.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let router = if site == "app.test" {
                &p.app
            } else {
                &p.reports
            };
            let (status, headers, body) = send(router, "GET", site, &path, &extra).await;
            // The Open report step: the person clicks its link, a same-origin navigation.
            if status == StatusCode::OK && body.contains("Open report") {
                // The site header has links too: follow the interstitial's own link.
                let href = body
                    .split_once("\">Open report</a>")
                    .unwrap()
                    .0
                    .rsplit_once("href=\"")
                    .unwrap()
                    .1;
                assert!(href.starts_with("/r/"), "unexpected report link: {href}");
                (site, path) = ("app.test", href.to_owned());
                clicked = true;
                continue;
            }
            for set in headers.get_all("set-cookie") {
                jar.push(set.to_str().unwrap().split(';').next().unwrap().to_owned());
            }
            if status != StatusCode::SEE_OTHER {
                return status;
            }
            let target = headers["location"].to_str().unwrap().to_owned();
            if let Some(rest) = target.strip_prefix("http://reports.test") {
                (site, path) = ("reports.test", rest.to_owned());
            } else if let Some(rest) = target.strip_prefix("http://app.test") {
                (site, path) = ("app.test", rest.to_owned());
            } else if target.starts_with('/') {
                path = target;
            } else {
                return status;
            }
            // A redirect to the sign-in page ends the walk: nobody is let in.
            if path.starts_with("/?") || path == "/" {
                return StatusCode::UNAUTHORIZED;
            }
        }
        panic!("redirect loop opening {slug}");
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn every_role_and_visibility_gets_the_right_answer_through_the_split_server() {
        // (visibility, may the person open it): the owner and admin always; the signed-in org on
        // `org`; anyone on `public`.
        let cases: [(&str, [bool; 5]); 3] = [
            // anonymous, reader, other, owner, admin
            ("private", [false, false, false, true, true]),
            ("org", [false, true, true, true, true]),
            ("public", [true, true, true, true, true]),
        ];
        // Signing in is slow by design, so the five callers sign in once for every visibility.
        let p = pair();
        assert_eq!(
            p.host.publish(&p.host.author, "secret").await,
            StatusCode::OK
        );
        let mut cookies: Vec<Option<String>> = vec![None];
        for email in [
            "reader@example.com",
            "other@example.com",
            "author@example.com",
            "admin@example.com",
        ] {
            cookies.push(Some(login(&p.app, email).await.0));
        }
        for (visibility, expected) in cases {
            let (status, _) = p
                .host
                .call(
                    "PUT",
                    "/api/reports/secret/access",
                    Some(&p.host.author),
                    json!({"visibility": visibility}),
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{visibility}");
            for (i, cookie) in cookies.iter().enumerate() {
                for slug in ["secret", "missing"] {
                    let status = open_in_browser(&p, cookie.as_deref(), slug).await;
                    let opens = slug == "secret" && expected[i];
                    assert_eq!(
                        status == StatusCode::OK,
                        opens,
                        "{visibility} /r/{slug} for role #{i} ended in {status}"
                    );
                    // A report the person cannot open reads like one that is not there.
                    if !opens && cookie.is_some() {
                        assert_eq!(status, StatusCode::NOT_FOUND, "{visibility} {slug} #{i}");
                    }
                    // Anonymous callers are sent to sign in for a hidden report and a missing one
                    // alike, so the answer does not say which exists.
                    if !opens && cookie.is_none() {
                        assert_eq!(
                            status,
                            StatusCode::UNAUTHORIZED,
                            "{visibility} {slug} anonymous"
                        );
                    }
                }
            }
        }
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn writes_by_cookie_need_the_csrf_token_and_the_same_origin_and_publishing_needs_the_report()
     {
        let p = pair();
        shared_report(&p).await;
        let (cookie, csrf) = login(&p.app, "author@example.com").await;
        let write = |headers: Vec<(&'static str, String)>| {
            let app = p.app.clone();
            async move {
                let mut req = Request::builder()
                    .method("PUT")
                    .uri("/api/reports/secret/access")
                    .header("host", "app.test")
                    .header("content-type", "application/json");
                for (name, value) in headers {
                    req = req.header(name, value);
                }
                app.oneshot(
                    req.body(Body::from(json!({"visibility": "org"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
            }
        };
        let with = |extra: Vec<(&'static str, String)>| {
            let mut all = vec![("cookie", cookie.clone())];
            all.extend(extra);
            all
        };
        assert_eq!(write(with(vec![])).await, StatusCode::FORBIDDEN, "no token");
        assert_eq!(
            write(with(vec![("x-richview-csrf", "wrong".into())])).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            write(with(vec![
                ("x-richview-csrf", csrf.clone()),
                ("origin", "http://evil.test".into()),
                ("sec-fetch-site", "cross-site".into()),
            ]))
            .await,
            StatusCode::FORBIDDEN,
            "cross-site"
        );
        assert_eq!(
            write(with(vec![
                ("x-richview-csrf", csrf),
                ("origin", "http://app.test".into()),
                ("sec-fetch-site", "same-origin".into()),
            ]))
            .await,
            StatusCode::OK
        );
        // Another author, a reader and no one cannot publish over the owner's report.
        for (token, name) in [
            (Some(p.host.other.as_str()), "other author"),
            (Some(p.host.reader.as_str()), "reader"),
            (None, "anonymous"),
        ] {
            let (status, _) = p
                .host
                .call(
                    "POST",
                    "/api/reports",
                    token,
                    json!({
                        "slug": "secret",
                        "source": "---\ntitle: Secret\ndescription: D\nversion: 1.0.1\n---\n\n# Taken over\n",
                        "reportFolder": {"root": "t", "source": "report.rmd", "files": ["report.rmd"]},
                    }),
                )
                .await;
            assert!(
                matches!(
                    status,
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
                ),
                "{name} got {status}"
            );
        }
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn every_form_route_sends_nobody_to_sign_in_before_reading_the_body() {
        let p = pair();
        shared_report(&p).await;
        let mut seen = 0;
        for spec in specs() {
            if !spec.path.starts_with("/ui/") || spec.rule == Rule::SignIn || spec.method != "POST"
            {
                continue;
            }
            seen += 1;
            let path = concrete(spec.path);
            let (status, headers, _) = send(&p.app, "POST", "app.test", &path, &[]).await;
            assert_eq!(status, StatusCode::SEE_OTHER, "{path}");
            assert!(
                headers["location"].to_str().unwrap().starts_with("/?m="),
                "{path}: {:?}",
                headers["location"]
            );
        }
        assert!(seen >= 6, "the form routes were not walked ({seen})");
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn refusals_cannot_push_a_change_out_of_the_audit_trail() {
        let p = pair();
        assert_eq!(
            p.host.publish(&p.host.author, "secret").await,
            StatusCode::OK
        );
        let huge = "x".repeat(300_000);
        for _ in 0..40 {
            let body =
                json!({"email": format!("{huge}@example.com"), "password": "nope"}).to_string();
            let res = p
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/session")
                        .header("content-type", "application/json")
                        .header("host", "app.test")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(res.status().is_client_error());
        }
        let (status, body) = p
            .host
            .call(
                "GET",
                "/api/governance/audit?limit=1000",
                Some(&p.host.admin),
                json!({}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body.contains("report.publish"),
            "the publish left the trail"
        );
        assert!(body.len() < 2_000_000, "logged names are bounded");
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn the_card_is_drawn_once_and_kept_beside_its_version() {
        let p = pair();
        assert_eq!(
            p.host.publish(&p.host.author, "secret").await,
            StatusCode::OK
        );
        fn find(dir: &std::path::Path, name: &str) -> bool {
            std::fs::read_dir(dir).unwrap().flatten().any(|e| {
                let path = e.path();
                path.file_name().is_some_and(|n| n == name) || (path.is_dir() && find(&path, name))
            })
        }
        assert!(find(&p.host.root.join("store"), "share.png"));
    }
}

/// The role, route and action matrix of FEA-SF-64, generated from the route table: a route with
/// no rule cannot exist, and every role gets the answer its rule gives.
mod matrix {
    use super::*;
    use richview_app::server::routes::Spec;

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Role {
        Anonymous,
        Reader,
        Other,
        Owner,
        Admin,
    }

    fn token(host: &Host, role: Role) -> Option<String> {
        match role {
            Role::Anonymous => None,
            Role::Reader => Some(host.reader.clone()),
            Role::Other => Some(host.other.clone()),
            Role::Owner => Some(host.author.clone()),
            Role::Admin => Some(host.admin.clone()),
        }
    }

    /// What the rule says a role gets on a private report only its owner and an admin may use.
    fn allowed(rule: Rule, role: Role) -> bool {
        match rule {
            Rule::Public | Rule::Shared | Rule::SignIn | Rule::Session | Rule::Preview => true,
            Rule::View | Rule::Edit | Rule::Manage | Rule::Share | Rule::Delete => {
                matches!(role, Role::Owner | Role::Admin)
            }
            Rule::Publish => !matches!(role, Role::Anonymous | Role::Reader),
            Rule::Admin => role == Role::Admin,
        }
    }

    async fn path_for(host: &Host, spec: &Spec, slug: &str) -> String {
        let (_, users) = host
            .call("GET", "/api/governance/users", Some(&host.admin), json!({}))
            .await;
        let users: Value = serde_json::from_str(&users).unwrap();
        let reader_id = users
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["email"] == "reader@example.com")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let (_, tokens) = host
            .call(
                "GET",
                &format!("/api/governance/users/{reader_id}/tokens"),
                Some(&host.admin),
                json!({}),
            )
            .await;
        let token_id = serde_json::from_str::<Value>(&tokens).unwrap()[0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        spec.path
            .replace("{slug}", slug)
            .replace("{version}", "1.0.0")
            .replace("{id}", &reader_id)
            .replace("{token}", &token_id)
            .replace("{name}", "0123456789abcdef.js")
            .replace("{*path}", "a.csv")
    }

    fn body_for(spec: &Spec) -> Value {
        if spec.path.starts_with("/ui/") {
            json!({})
        } else if spec.method == "PUT" && spec.path.ends_with("/access") {
            json!({"visibility": "private"})
        } else {
            json!({})
        }
    }

    #[tokio::test]
    #[ignore = "release: publishes reports using the real embedded runtime staged by richview-build (ARC-07)"]
    async fn every_route_gives_every_role_the_answer_its_rule_gives() {
        for spec in specs() {
            if matches!(spec.rule, Rule::Preview | Rule::SignIn | Rule::Session)
                || spec.path.starts_with("/ui/")
            {
                continue;
            }
            // Roles that cannot change what a later role sees go first; an owner's or admin's
            // success may delete or change the subject, so each gets a fresh server.
            for group in [
                vec![Role::Anonymous, Role::Reader, Role::Other, Role::Owner],
                vec![Role::Admin],
            ] {
                let host = Host::bare();
                assert_eq!(host.publish(&host.author, "secret").await, StatusCode::OK);
                let path = path_for(&host, &spec, "secret").await;
                for role in group {
                    let who = token(&host, role);
                    let (status, body) = host
                        .call(spec.method, &path, who.as_deref(), body_for(&spec))
                        .await;
                    let denied = matches!(
                        status,
                        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
                    );
                    let file_route =
                        spec.path.contains("/files/") || spec.path.starts_with("/_rv/");
                    if allowed(spec.rule, role) {
                        assert!(
                            !matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
                                && (file_route || status != StatusCode::NOT_FOUND),
                            "{role:?} on {} {} was refused with {status}: {body}",
                            spec.method,
                            spec.path
                        );
                    } else {
                        assert!(
                            denied,
                            "{role:?} on {} {} was let through with {status}: {body}",
                            spec.method, spec.path
                        );
                    }
                    // A denial reveals nothing: the same answer as for a report that is not there.
                    if matches!(
                        spec.rule,
                        Rule::View | Rule::Edit | Rule::Manage | Rule::Share | Rule::Delete
                    ) && matches!(role, Role::Reader | Role::Other)
                    {
                        let missing = path_for(&host, &spec, "no-such-report").await;
                        let (status_missing, body_missing) = host
                            .call(spec.method, &missing, who.as_deref(), body_for(&spec))
                            .await;
                        assert_eq!(
                            (status, body.replace("secret", "X")),
                            (status_missing, body_missing.replace("no-such-report", "X")),
                            "{role:?} learns whether a report exists from {} {}",
                            spec.method,
                            spec.path
                        );
                    }
                }
            }
        }
    }
}
