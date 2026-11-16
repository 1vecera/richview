use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use richview_app::access::{Access, Visibility};
use richview_app::{
    mcp, resolver,
    server::{AppState, build_app},
    storage::{FsStorage, StoredReport},
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, sync::Arc};
use tower::ServiceExt;

struct Fixture {
    root: PathBuf,
    storage: FsStorage,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::current_dir().unwrap().join("tmp").join(format!(
            "app-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("public")).unwrap();
        fs::write(root.join("report.rmd"), "# Saved source").unwrap();
        fs::write(root.join("public/index.html"), "<h1>Rust host</h1>").unwrap();
        fs::write(
            root.join("public/playground.html"),
            "<textarea id=rv-source></textarea>",
        )
        .unwrap();
        let storage = FsStorage::new(root.join("store")).unwrap();
        Self { root, storage }
    }
    fn app(&self, token: Option<&str>) -> axum::Router {
        build_app(AppState {
            storage: self.storage.clone(),
            access: Access::open(self.storage.dir(), Visibility::Public, token.map(Arc::from))
                .unwrap(),
            preview: None,
            public_origin: None,
            report_origin: None,
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
async fn request(
    app: &axum::Router,
    method: &str,
    path: &str,
    mut body: Value,
    token: Option<&str>,
) -> (StatusCode, String) {
    if body.get("source").is_some() && body.get("reportFolder").is_none() {
        let files = std::iter::once("report.rmd".to_owned())
            .chain(
                body["data"]
                    .as_object()
                    .into_iter()
                    .flat_map(|m| m.keys().cloned()),
            )
            .collect::<Vec<_>>();
        body["reportFolder"] =
            json!({"root":"request-fixture","source":"report.rmd","files":files});
    }
    // Tests publish new addresses unless a test says otherwise.
    if body.get("slug").is_some() && body.get("new").is_none() {
        body["new"] = json!(true);
    }
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("host", "127.0.0.1:4711");
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"))
    }
    let res = app
        .clone()
        .oneshot(
            req.body(if method == "GET" {
                Body::empty()
            } else {
                Body::from(body.to_string())
            })
            .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 32 * 1024 * 1024).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}
fn source(version: &str, heading: &str) -> String {
    format!(
        "---\ntitle: Contract\ndescription: A fixture\nversion: {version}\n---\n\n# {heading}\n"
    )
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn auth_immutable_versions_latest_raw_regeneration_and_listing() {
    let f = Fixture::new();
    let app = f.app(Some("test-token"));
    let body = json!({"slug":"contract","source":source("0.0.1","first"),"html":"<script>client forgery</script>","frontmatter":{"title":"forged"}});
    assert_eq!(
        request(&app, "POST", "/api/reports", body.clone(), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, first) = request(
        &app,
        "POST",
        "/api/reports",
        body.clone(),
        Some("test-token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(
        serde_json::from_str::<Value>(&first).unwrap()["created"],
        true
    );
    let second = json!({"slug":"contract","source":source("0.0.2","second")});
    assert_eq!(
        request(&app, "POST", "/api/reports", second, Some("test-token"))
            .await
            .0,
        StatusCode::OK
    );
    let forged = json!({"slug":"contract","source":source("0.0.1","overwrite")});
    let (status, again) = request(&app, "POST", "/api/reports", forged, Some("test-token")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");
    assert_eq!(
        serde_json::from_str::<Value>(&again).unwrap()["diagnostics"][0]["code"],
        "publish.version-exists"
    );
    let (_, latest) = request(&app, "GET", "/r/contract", json!({}), None).await;
    assert!(latest.contains(">second</h2>"));
    assert!(!latest.contains("client forgery"));
    // The published report pins the bytes returned by its immutable runtime route.
    let runtime_src = latest
        .split("<script src=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap();
    let (status, runtime_js) = request(&app, "GET", runtime_src, json!({}), None).await;
    assert_eq!(status, StatusCode::OK);
    let hash = {
        use base64::Engine as _;
        use sha2::{Digest, Sha256};
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(runtime_js.as_bytes()))
    };
    assert!(latest.contains(&format!("integrity=\"sha256-{hash}\"")));
    let (_, old) = request(&app, "GET", "/r/contract/v/0.0.1", json!({}), None).await;
    assert!(old.contains(">first</h2>"));
    assert!(!old.contains("overwrite"));
    let (_, raw) = request(&app, "GET", "/raw/contract/v/0.0.1", json!({}), None).await;
    assert_eq!(raw, source("0.0.1", "first"));
    let (_, versions) = request(
        &app,
        "GET",
        "/api/reports/contract/versions",
        json!({}),
        None,
    )
    .await;
    assert_eq!(
        serde_json::from_str::<Value>(&versions).unwrap(),
        json!(["0.0.2", "0.0.1"])
    );
    // Anyone-with-the-link reports are not listed to anonymous callers; the token's holder sees them.
    let (_, anonymous) = request(&app, "GET", "/api/reports", json!({}), None).await;
    assert_eq!(
        serde_json::from_str::<Value>(&anonymous).unwrap(),
        json!([])
    );
    let (_, listing) = request(&app, "GET", "/api/reports", json!({}), Some("test-token")).await;
    assert_eq!(
        serde_json::from_str::<Value>(&listing).unwrap()[0]["version"],
        "0.0.2"
    );
    let (status, regenerated) = request(
        &app,
        "POST",
        "/api/reports/contract/v/0.0.1/regenerate",
        json!({}),
        Some("test-token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/reports/contract/v/0.0.1/regenerate",
            json!({}),
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED,
        "regenerate needs the token"
    );
    assert!(
        serde_json::from_str::<Value>(&regenerated).unwrap()["html"]
            .as_str()
            .unwrap()
            .contains(">first</h2>")
    );
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn untrusted_source_cannot_fetch_local_files_or_inject_html() {
    let f = Fixture::new();
    let app = f.app(Some("token"));
    let source = "---\ntitle: Security\ndescription: D\nversion: 0.0.1\n---\n\n<script>alert(1)</script>\n\n```chart\ntype: line\ntitle: T\ndata: {csv: /etc/passwd}\nencoding: {x: user,y: value}\n```";
    let (status, body) = request(
        &app,
        "POST",
        "/api/reports",
        json!({"slug":"hostile","source":source}),
        Some("token"),
    )
    .await;
    assert!(status.is_client_error(), "{status} {body}");
    assert!(body.contains("report.asset.escape"));
    assert!(f.storage.get("hostile", None).unwrap().is_none());
    let html_source =
        "---\ntitle: Raw html\ndescription: D\nversion: 0.0.1\n---\n\n<script>alert(1)</script>\n";
    let (status, body) = request(
        &app,
        "POST",
        "/api/reports",
        json!({"slug":"raw-html","source":html_source}),
        Some("token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stored = f.storage.get("raw-html", None).unwrap().unwrap();
    assert!(!stored.html.contains("<script>alert(1)</script>"));
    assert!(!stored.html.contains("root:x:0"));
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/reports",
            json!({"slug":"../escape","source":self::source("0.0.1","x")}),
            Some("token")
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/reports",
            json!({"slug":"x","source":"# valid local draft"}),
            Some("token")
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&f.app(None), "POST", "/api/reports", json!({}), None)
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn owner_actions_reach_the_server_through_the_operation_layer() {
    use richview_app::ops::{OwnerAction, report};
    let f = Fixture::new();
    let app = f.app(Some("owner-token"));
    for version in ["0.0.1", "0.0.2"] {
        let body = json!({"slug":"owned","source":source(version, "Owned")});
        assert_eq!(
            request(&app, "POST", "/api/reports", body, Some("owner-token"))
                .await
                .0,
            StatusCode::OK
        );
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let pin = report(
        &server,
        "owner-token",
        OwnerAction::Pin {
            slug: "owned",
            version: "0.0.1",
        },
    )
    .await
    .unwrap();
    assert_eq!(pin["pinned"], "0.0.1");
    assert_eq!(
        f.storage.latest_version("owned").unwrap().as_deref(),
        Some("0.0.1")
    );
    report(&server, "owner-token", OwnerAction::Unpin { slug: "owned" })
        .await
        .unwrap();
    assert_eq!(
        f.storage.latest_version("owned").unwrap().as_deref(),
        Some("0.0.2")
    );
    let regenerated = report(
        &server,
        "owner-token",
        OwnerAction::Regenerate {
            slug: "owned",
            version: "0.0.2",
        },
    )
    .await
    .unwrap();
    assert!(regenerated["changed"].is_boolean());
    assert!(
        f.storage.get("owned", Some("0.0.2")).unwrap().is_some(),
        "regenerate stores nothing new"
    );
    report(
        &server,
        "owner-token",
        OwnerAction::Delete {
            slug: "owned",
            version: "0.0.1",
        },
    )
    .await
    .unwrap();
    assert!(f.storage.is_deleted("owned", "0.0.1").unwrap());
    let refused = report(&server, "wrong", OwnerAction::Unpin { slug: "owned" })
        .await
        .unwrap_err();
    assert!(
        refused
            .message()
            .starts_with("The server did not accept this token."),
        "{}",
        refused.message()
    );
    let missing = report(
        &server,
        "owner-token",
        OwnerAction::Pin {
            slug: "owned",
            version: "9.9.9",
        },
    )
    .await
    .unwrap_err();
    assert_eq!(missing.diagnostics()[0].code, "publish.version-missing");
    worker.abort();
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn linked_files_are_published_and_served_read_only() {
    use base64::Engine;
    let f = Fixture::new();
    let app = f.app(Some("token"));
    let source = "---\ntitle: Linked\ndescription: D\nversion: 0.0.1\n---\n\nSee the [orders](data/orders.csv).\n";
    let folder =
        json!({"root":"linked","source":"report.rmd","files":["report.rmd","data/orders.csv"]});
    let (status, body) = request(
        &app,
        "POST",
        "/api/reports",
        json!({"slug":"linked","source":source,"reportFolder":folder}),
        Some("token"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("report.asset.missing"));
    assert!(f.storage.get("linked", None).unwrap().is_none());
    let encoded = base64::engine::general_purpose::STANDARD.encode("id,total\n1,9\n");
    let (status, body) = request(
        &app,
        "POST",
        "/api/reports",
        json!({"slug":"linked","source":source,"reportFolder":folder,"files":{"data/orders.csv":encoded,"stray.csv":encoded}}),
        Some("token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stored = f.storage.get("linked", None).unwrap().unwrap();
    assert_eq!(
        stored.files.keys().collect::<Vec<_>>(),
        ["data/orders.csv"],
        "only linked files are kept"
    );
    assert!(
        stored
            .html
            .contains("href=\"/r/linked/v/0.0.1/files/data/orders.csv\"")
    );
    let response = f
        .app(None)
        .oneshot(
            Request::builder()
                .uri("/r/linked/v/0.0.1/files/data/orders.csv")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment")
    );
    assert_eq!(
        response.headers()["content-type"],
        "text/csv; charset=utf-8"
    );
    for missing in [
        "/r/linked/v/0.0.1/files/stray.csv",
        "/r/linked/v/0.0.1/files/../x",
    ] {
        let response = f
            .app(None)
            .oneshot(Request::builder().uri(missing).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(response.status().is_client_error(), "{missing}");
    }
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn a_new_report_address_needs_confirmation_and_later_versions_do_not() {
    let f = Fixture::new();
    let app = f.app(Some("token"));
    let body = json!({"slug":"fresh","source":source("0.0.1","Fresh"),"new":false});
    let (status, text) = request(&app, "POST", "/api/reports", body.clone(), Some("token")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{text}");
    assert!(text.contains("publish.new-slug") && text.contains("--new"));
    assert!(
        f.storage.get("fresh", None).unwrap().is_none(),
        "nothing stored"
    );
    let mut confirmed = body.clone();
    confirmed["new"] = json!(true);
    let (status, text) = request(&app, "POST", "/api/reports", confirmed, Some("token")).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    // A publish result is the same object as every other operation's (TYP-54).
    let published: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        (&published["ok"], &published["status"], &published["output"]),
        (&json!(true), &json!("clean"), &Value::Null),
        "{text}"
    );
    assert!(published["diagnostics"].is_array(), "{text}");
    let next = json!({"slug":"fresh","source":source("0.0.2","Fresh"),"new":false});
    assert_eq!(
        request(&app, "POST", "/api/reports", next, Some("token"))
            .await
            .0,
        StatusCode::OK,
        "a stored address needs no confirmation"
    );
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn styles_and_external_data_are_retained_for_regeneration() {
    let f = Fixture::new();
    let app = f.app(Some("token"));
    let src = "---\ntitle: External data\ndescription: D\nversion: 0.0.1\nstyles: [./custom.rstyle.yaml]\n---\n\n```chart\ntype: bar\ntitle: T\ndata: {csv: data.csv}\nencoding: {x: label,y: value}\n```";
    let style = "tokens:\n  accent: '#176430'\n";
    let (status,body)=request(&app,"POST","/api/reports",json!({"slug":"external","source":src,"styles":[style],"data":{"data.csv":"label,value\nA,12\nB,19","./custom.rstyle.yaml":style},"reportFolder":{"root":"external","source":"report.rmd","files":["report.rmd","data.csv","custom.rstyle.yaml"]}}),Some("token")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stored = f.storage.get("external", None).unwrap().unwrap();
    assert_eq!(stored.styles, vec![style]);
    assert_eq!(stored.data["data.csv"], "label,value\nA,12\nB,19");
    let (status, body) = request(
        &app,
        "POST",
        "/api/reports/external/v/0.0.1/regenerate",
        json!({}),
        Some("token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let value: Value = serde_json::from_str(&body).unwrap();
    assert!(value["html"].as_str().unwrap().contains("svg"));
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn public_artifacts_and_metadata_read_only_their_required_files() {
    let f = Fixture::new();
    let app = f.app(Some("token"));
    let source = source("1.0.0", "Independent source");
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/reports",
            json!({"slug":"partial","source":source}),
            Some("token")
        )
        .await
        .0,
        StatusCode::OK
    );
    let directory = f.storage.dir().join("partial/1.0.0");
    fs::remove_file(directory.join("rendered.html")).unwrap();
    fs::write(directory.join("assets.json"), "not a source bundle").unwrap();
    assert_eq!(
        request(&app, "GET", "/raw/partial", json!({}), None).await,
        (StatusCode::OK, source)
    );
    assert_eq!(
        request(&app, "GET", "/api/reports", json!({}), None)
            .await
            .0,
        StatusCode::OK
    );
    let (status, metadata) = request(&app, "GET", "/api/reports/partial", json!({}), None).await;
    assert_eq!(status, StatusCode::OK);
    let metadata: Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(metadata["version"], "1.0.0");
    assert_eq!(metadata["versions"], json!(["1.0.0"]));
    #[cfg(unix)]
    {
        fs::remove_file(directory.join("source.rmd")).unwrap();
        std::os::unix::fs::symlink(
            f.root.join("public/index.html"),
            directory.join("source.rmd"),
        )
        .unwrap();
        assert_eq!(
            request(&app, "GET", "/raw/partial", json!({}), None)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn strict_semver_versions_publish_and_resolve_without_path_escape() {
    let f = Fixture::new();
    let app = f.app(Some("token"));
    for version in ["1.2.3-beta.1", "2.0.0", "3.0.0", "10.0.0"] {
        let source = source(version, "Strict version");
        let (status, body) = request(
            &app,
            "POST",
            "/api/reports",
            json!({"slug":"strict","source":source}),
            Some("token"),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (_, response) = request(
            &app,
            "POST",
            "/api/reports",
            json!({"slug":"strict","source":source}),
            Some("token"),
        )
        .await;
        assert_eq!(
            serde_json::from_str::<Value>(&response).unwrap()["created"],
            false
        );
        let (_, raw) = request(
            &app,
            "GET",
            &format!("/raw/strict/v/{version}"),
            json!({}),
            None,
        )
        .await;
        assert_eq!(raw, source);
    }
    assert_eq!(
        f.storage.list_versions("strict").unwrap(),
        vec!["10.0.0", "3.0.0", "2.0.0", "1.2.3-beta.1"]
    );
    for invalid in [
        "01.2.3",
        "2.00.0",
        "1.2.3-beta.01",
        "1.0.0+build",
        "../1.2.3",
        "1.2.3/other",
        "1.2.3 ",
        "1.2.3\n",
        "1.2.3\\",
        "1.0",
        "",
    ] {
        assert!(richview_app::storage::validate_version(invalid).is_err());
    }
}

#[tokio::test]
async fn the_report_host_serves_reports_only() {
    let f = Fixture::new();
    let app = f.app(None);
    assert_eq!(
        request(&app, "GET", "/health", json!({}), None).await.0,
        StatusCode::OK
    );
    for path in [
        "/playground",
        "/app",
        "/showcase",
        "/richview_runtime.js",
        "/docs/",
    ] {
        assert_eq!(
            request(&app, "GET", path, json!({}), None).await.0,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    // No legacy domain redirect (INT-47).
    let res = app
        .oneshot(
            Request::builder()
                .uri("/playground")
                .header("host", "therichview.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn a_broken_source_keeps_the_last_good_preview_under_a_banner() {
    let f = Fixture::new();
    let folder = f.root.join("preview-report");
    fs::create_dir(&folder).unwrap();
    let source = folder.join("report.rmd");
    fs::write(&source, "# Good heading\n").unwrap();
    let app = build_app(AppState {
        storage: f.storage.clone(),
        access: Access::open(f.storage.dir(), Visibility::Public, None).unwrap(),
        preview: Some(richview_app::server::Preview {
            source: source.clone(),
            styles: vec![],
            allow_network: false,
            last_good: Default::default(),
        }),
        public_origin: None,
        report_origin: None,
    });
    let (status, html) = request(&app, "GET", "/", json!({}), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Good heading") && !html.contains("richview-banner"));
    // A source with findings still renders, with the findings above it.
    fs::write(&source, "---\ntitle: [unclosed\n---\n\n# Edited\n").unwrap();
    let (status, html) = request(&app, "GET", "/", json!({}), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains("richview-banner") && html.contains("Edited"),
        "{html}"
    );
    // A render that fails outright keeps the last page under the reason.
    fs::remove_file(&source).unwrap();
    let (status, html) = request(&app, "GET", "/", json!({}), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("richview-banner"), "{html}");
    assert!(html.contains("Edited"), "the last good render stays");
    fs::write(&source, "# Fixed\n").unwrap();
    let (_, html) = request(&app, "GET", "/", json!({}), None).await;
    assert!(html.contains("Fixed") && !html.contains("richview-banner"));
}

#[test]
fn resolver_enforces_source_directory_and_resolves_chart_local_data() {
    let f = Fixture::new();
    fs::create_dir(f.root.join("report")).unwrap();
    fs::write(f.root.join("outside.csv"), "x,y\n1,2").unwrap();
    fs::write(f.root.join("report/data.csv"), "x,y\n1,2").unwrap();
    let path = f.root.join("report/source.rmd");
    let src = "```table\ndata: {csv: data.csv}\n```";
    fs::write(&path, src).unwrap();
    let options = resolver::resolve_document_options_policy(src, &path, &[], false, false)
        .unwrap()
        .1;
    assert_eq!(options.data["data.csv"], "x,y\n1,2");
    // A reference that escapes the folder is a finding on the report; the file is never read.
    let escaping = resolver::resolve_document_options_policy(
        &src.replace("data.csv", "../outside.csv"),
        &path,
        &[],
        false,
        false,
    )
    .unwrap()
    .1;
    assert!(!escaping.data.values().any(|v| v.contains("1,2")));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(f.root.join("outside.csv"), f.root.join("report/link.csv"))
            .unwrap();
        let linked = resolver::resolve_document_options_policy(
            &src.replace("data.csv", "link.csv"),
            &path,
            &[],
            false,
            false,
        )
        .unwrap()
        .1;
        assert!(!linked.data.contains_key("link.csv"));
    }
}

#[test]
fn combined_resource_budget_includes_styles_and_unique_data() {
    let f = Fixture::new();
    let mib = 1024 * 1024;
    let mut csv = String::from("x,y\nA,1\n");
    csv.extend(std::iter::repeat_n(' ', 20 * mib - csv.len()));
    for name in ["a.csv", "b.csv", "c.csv"] {
        fs::write(f.root.join(name), &csv).unwrap();
    }
    drop(csv);
    let source = "```table\ndata: {csv: a.csv}\n```\n```table\ndata: {csv: b.csv}\n```\n```table\ndata: {csv: c.csv}\n```";
    let path = f.root.join("report.rmd");
    let (_, options) =
        resolver::resolve_document_options_policy(source, &path, &[], false, false).unwrap();
    assert_eq!(options.data.len(), 3);
    drop(options);
    let mut style = String::from("tokens:\n  accent: '#123456'\n# ");
    style.extend(std::iter::repeat_n(' ', 8 * mib - style.len()));
    fs::write(f.root.join("large.rstyle.yaml"), style).unwrap();
    let error = resolver::resolve_document_options_policy(
        source,
        &path,
        &["large.rstyle.yaml".into()],
        false,
        false,
    )
    .unwrap_err();
    assert!(
        error.contains("64 MiB combined data/media/style budget"),
        "{error}"
    );
    assert!(
        error.contains("c.csv") && error.contains("Reduce referenced file sizes"),
        "{error}"
    );
}

#[test]
fn chart_local_references_reuse_resolved_bytes_without_charging_again() {
    let f = Fixture::new();
    let size = 24 * 1024 * 1024;
    let mut csv = String::from("x,y\nA,1\n");
    csv.extend(std::iter::repeat_n(' ', size - csv.len()));
    fs::write(f.root.join("shared.csv"), csv).unwrap();
    let source = "```table\ndata: {csv: shared.csv}\n```\n\n```table\ndata: {csv: shared.csv}\n```\n\n```table\ndata: {csv: shared.csv}\n```";
    let (_, options) = resolver::resolve_document_options_policy(
        source,
        &f.root.join("report.rmd"),
        &[],
        false,
        false,
    )
    .unwrap();
    assert_eq!(options.data.len(), 1);
    assert_eq!(options.data["shared.csv"].len(), size);
}

#[test]
fn combined_resource_budget_counts_media_base64_expansion() {
    let f = Fixture::new();
    let size = 13 * 1024 * 1024;
    let end = "</desc></svg>";
    let mut svg = String::from("<svg xmlns='http://www.w3.org/2000/svg'><desc>");
    svg.extend(std::iter::repeat_n(' ', size - svg.len() - end.len()));
    svg.push_str(end);
    let mut source = String::new();
    for name in ["a.svg", "b.svg", "c.svg", "d.svg"] {
        fs::write(f.root.join(name), &svg).unwrap();
        source.push_str(&format!(
            "```image\nsrc: {name}\nalt: Budget fixture\n```\n\n"
        ));
    }
    drop(svg);
    let error = resolver::resolve_document_options_policy(
        &source,
        &f.root.join("report.rmd"),
        &[],
        false,
        false,
    )
    .unwrap_err();
    // Four legal 13 MiB files total only 52 MiB raw, but their retained data URIs exceed64 MiB.
    assert!(
        error.contains("64 MiB combined data/media/style budget"),
        "{error}"
    );
    assert!(
        error.contains("d.svg") && error.contains("base64 expansion"),
        "{error}"
    );
}

#[tokio::test]
async fn occupied_server_ports_name_the_address_and_the_option_to_change() {
    let fixture = Fixture::new();
    let state = AppState {
        storage: fixture.storage.clone(),
        access: Access::open(fixture.storage.dir(), Visibility::Public, None).unwrap(),
        preview: None,
        public_origin: None,
        report_origin: None,
    };
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = occupied.local_addr().unwrap();
    for (error, option) in [
        (
            richview_app::server::serve(state.clone(), "127.0.0.1", address.port())
                .await
                .unwrap_err(),
            "--port",
        ),
        (
            richview_app::server::serve_split(state.clone(), "127.0.0.1", address.port(), 0, None)
                .await
                .unwrap_err(),
            "--port",
        ),
        (
            richview_app::server::serve_split(state, "127.0.0.1", 0, address.port(), None)
                .await
                .unwrap_err(),
            "--report-port",
        ),
    ] {
        assert!(error.contains(&address.to_string()), "{error}");
        assert!(error.contains(option), "{error}");
    }
}

#[tokio::test]
async fn the_mcp_hello_world_resource_supplies_a_complete_checkable_report() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(env!("CARGO_BIN_EXE_richview"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"resources/read",
        "params":{"uri":"richview://examples/hello-world"}});
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    let contents = response["result"]["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 2, "The example must include its local CSV");
    assert_eq!(contents[0]["text"], richview_app::ops::HELLO_REPORT);
    let root = tempfile::tempdir().unwrap();
    for content in contents {
        let uri = content["uri"].as_str().unwrap();
        let name = if uri == "richview://examples/hello-world" {
            "report.rmd"
        } else {
            uri.strip_prefix("richview://examples/hello-world/")
                .unwrap()
        };
        assert!(richview_core::document::report_relative_path(name));
        let file = root.path().join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content["text"].as_str().unwrap()).unwrap();
    }
    let checked = mcp::dispatch_tool(
        "richview_check",
        &json!({"inputPath":root.path().join("report.rmd")}),
    )
    .await;
    assert_eq!(checked["isError"], false, "{checked}");
    assert_eq!(checked["structuredContent"]["status"], "clean", "{checked}");
    assert_eq!(
        checked["structuredContent"]["charts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn mcp_tool_names_resources_and_hard_failure_contract() {
    let names: Vec<_> = mcp::tool_definitions()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names.len(), 9);
    for n in [
        "richview_render",
        "richview_validate",
        "richview_check",
        "richview_publish",
        "richview_learn",
        "richview_asset_put",
        "richview_asset_verify",
        "richview_image_search",
        "richview_image_use",
    ] {
        assert!(names.contains(&n.into()))
    }
    assert_eq!(mcp::list_resources().len(), 2);
    // The token goes only to the configured server: naming another one is refused before any request.
    if std::env::var("RICHVIEW_SERVER").is_err() {
        let refused = mcp::dispatch_tool(
            "richview_publish",
            &json!({"inputPath": "x/report.rmd", "server": "https://elsewhere.example"}),
        )
        .await;
        assert_eq!(refused["isError"], true);
        assert!(
            refused["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("RICHVIEW_SERVER")
        );
    }
    let learn = mcp::dispatch_tool("richview_learn", &json!({})).await;
    assert_eq!(learn["isError"], false);
    assert_eq!(
        learn["content"][0]["text"],
        richview_core::document::learn_bundle(),
        "MCP learn returns the core bundle"
    );
    // The CLI and MCP print the same bytes for the bundle and every section (FEA-SF-47).
    let mut learned = vec![(
        vec!["learn".to_owned()],
        richview_core::document::learn_bundle(),
    )];
    for section in richview_core::document::learn_sections() {
        let text =
            mcp::dispatch_tool("richview_learn", &json!({"section": section.kind.clone()})).await;
        learned.push((
            vec!["learn".to_owned(), section.kind],
            text["content"][0]["text"].as_str().unwrap().to_owned(),
        ));
    }
    for (args, expected) in learned {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_richview"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{args:?}");
    }
    // A call that cannot run is an error: unknown tool, unknown section, missing path, no token.
    for (tool, args) in [
        ("richview_generate", json!({})),
        ("richview_learn", json!({"section": "nope"})),
        ("richview_validate", json!({})),
        ("richview_publish", json!({"inputPath": "x/report.rmd"})),
    ] {
        assert_eq!(
            mcp::dispatch_tool(tool, &args).await["isError"],
            true,
            "{tool}"
        );
    }
    // A report with errors is a result: ok false, isError false.
    let f = Fixture::new();
    let broken = Fixture::new().root.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("report.rmd"), "```chart\ntype: unknown\n```\n").unwrap();
    let result = mcp::dispatch_tool(
        "richview_validate",
        &json!({"inputPath": broken.join("report.rmd")}),
    )
    .await;
    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(result["structuredContent"]["ok"], false);
    assert!(result["structuredContent"].get("error").is_none());
    let saved = mcp::dispatch_tool(
        "richview_check",
        &json!({"inputPath": f.root.join("report.rmd"), "chartWidth": 400}),
    )
    .await;
    assert_eq!(saved["isError"], false);
    assert_eq!(saved["structuredContent"]["ok"], true);
    fs::write(
        f.root.join("report.rmd"),
        "```callout\nkind: neutral\nbody: A useful note\nunknown_field: value\n```\n",
    )
    .unwrap();
    let warned = mcp::dispatch_tool(
        "richview_validate",
        &json!({"inputPath":f.root.join("report.rmd")}),
    )
    .await;
    assert_eq!(warned["isError"], false, "{warned}");
    assert_eq!(warned["structuredContent"]["ok"], true, "{warned}");
    assert_eq!(warned["structuredContent"]["status"], "issues", "{warned}");
    let summary = warned["content"][0]["text"].as_str().unwrap();
    assert!(summary.contains("status: issues"), "{summary}");
    assert_eq!(summary.lines().count(), 1);

    // `inputPath` reads a folder as the commands do (FEA-SF-01): the folder holding a report is
    // that report, and a folder of report folders returns every report's result in `reports`.
    let folder = mcp::dispatch_tool("richview_validate", &json!({"inputPath": f.root})).await;
    assert_eq!(folder["structuredContent"], warned["structuredContent"]);
    let holder = Fixture::new();
    let parent = holder.root.join("reports");
    for (name, body) in [
        ("a", "Body text.\n"),
        ("b", "```chart\ntype: unknown\n```\n"),
    ] {
        fs::create_dir_all(parent.join(name)).unwrap();
        fs::write(parent.join(name).join("report.rmd"), body).unwrap();
    }
    for tool in ["richview_validate", "richview_check"] {
        let all = mcp::dispatch_tool(tool, &json!({"inputPath": parent})).await;
        assert_eq!(all["isError"], false, "{tool}: {all}");
        let result = &all["structuredContent"];
        assert_eq!(result["ok"], false, "{tool}: {result}");
        assert_eq!(
            result["reports"]
                .as_array()
                .unwrap()
                .iter()
                .map(|report| (&report["input"], &report["ok"]))
                .collect::<Vec<_>>(),
            [
                (&json!(parent.join("a/report.rmd")), &json!(true)),
                (&json!(parent.join("b/report.rmd")), &json!(false)),
            ],
            "{tool}: {result}"
        );
        let summary = all["content"][0]["text"].as_str().unwrap();
        assert!(
            summary.contains("2 reports, 1 with errors"),
            "{tool}: {summary}"
        );
    }
    // Publishing stays one report: the refusal names the reports the folder holds.
    let refused = mcp::dispatch_tool("richview_publish", &json!({"inputPath": parent})).await;
    assert_eq!(refused["isError"], true);
    let text = refused["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("holds 2 reports"), "{text}");
}

#[test]
fn storage_keeps_every_version_and_concurrent_publication_is_idempotent() {
    let f = Fixture::new();
    for n in 0..51 {
        f.storage
            .put_version(StoredReport {
                report_folder: None,
                slug: "versions".into(),
                version: format!("1.0.{n}"),
                source: "# source".into(),
                html: "<h1>source</h1>".into(),
                frontmatter: json!({}),
                created_at: "now".into(),
                styles: vec![],
                data: Default::default(),
                files: Default::default(),
                content_sha256: String::new(),
            })
            .unwrap();
    }
    assert_eq!(f.storage.list_versions("versions").unwrap().len(), 51);
    assert_eq!(
        f.storage.get("versions", None).unwrap().unwrap().version,
        "1.0.50"
    );
    assert!(f.storage.get("versions", Some("1.0.0")).unwrap().is_some());
    let report = StoredReport {
        report_folder: None,
        slug: "race".into(),
        version: "0.0.1".into(),
        source: "# source".into(),
        html: "artifact".into(),
        frontmatter: json!({}),
        created_at: "now".into(),
        styles: vec![],
        data: Default::default(),
        files: Default::default(),
        content_sha256: String::new(),
    };
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let dir = f.root.join("store");
            let report = report.clone();
            std::thread::spawn(move || FsStorage::new(dir).unwrap().put_version(report).unwrap().1)
        })
        .collect();
    let created = workers
        .into_iter()
        .map(|w| w.join().unwrap())
        .filter(|created| *created)
        .count();
    assert_eq!(created, 1);
}

#[tokio::test]
async fn stored_version_and_summary_json_use_public_names_and_preserve_authored_data() {
    let f = Fixture::new();
    let (stored, _) = f
        .storage
        .put_version(StoredReport {
            slug: "weekly".into(),
            version: "1.0.0".into(),
            source: "# Authored_title".into(),
            report_folder: Some(richview_core::document::ReportFolder {
                root: "weekly".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into(), "data/order_history.csv".into()],
            }),
            html: String::new(),
            frontmatter: json!({"title_image": "media/header_image.png"}),
            created_at: "2026-10-29T12:00:00Z".into(),
            styles: vec!["custom_style.rstyle.yaml".into()],
            data: [(
                "data/order_history.csv".into(),
                "order_id,value\nA_1,2\n".into(),
            )]
            .into(),
            files: Default::default(),
            content_sha256: String::new(),
        })
        .unwrap();
    let app = f.app(Some("wire-token"));
    let (status, body) = request(
        &app,
        "GET",
        "/api/reports/weekly/v/1.0.0",
        json!({}),
        Some("wire-token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({
            "slug": "weekly", "version": "1.0.0", "source": "# Authored_title",
            "reportFolder": {"root": "", "source": "report.rmd", "files": ["report.rmd", "data/order_history.csv"]},
            "html": "", "frontmatter": {"title_image": "media/header_image.png"},
            "createdAt": "2026-10-29T12:00:00Z", "styles": ["custom_style.rstyle.yaml"],
            "data": {"data/order_history.csv": "order_id,value\nA_1,2\n"}, "files": {},
            "contentSha256": stored.content_sha256,
        }),
        "stored version JSON contract"
    );
    let (status, body) = request(
        &app,
        "GET",
        "/api/reports/weekly",
        json!({}),
        Some("wire-token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let summary: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(summary["updatedAt"], "2026-10-29T12:00:00Z");
    assert!(summary.get("updated_at").is_none());
    assert!(summary.get("createdAt").is_none());
}

#[test]
fn unsaved_buffers_media_embedding_and_preview_dependency_changes() {
    let f = Fixture::new();
    let report = f.root.join("report.rmd");
    fs::write(
        f.root.join("image.png"),
        [137u8, 80, 78, 71, 13, 10, 26, 10],
    )
    .unwrap();
    fs::write(f.root.join("data.csv"), "x,y\nA,1").unwrap();
    fs::write(
        f.root.join("custom.rstyle.yaml"),
        "tokens:\n  accent: '#176430'\n",
    )
    .unwrap();
    let source = "---\ntitle_image: image.png\nstyles: [custom.rstyle.yaml]\n---\n\n![Inline](image.png)\n\n```chart\ntype: bar\ntitle: T\ndata: {csv: data.csv}\nencoding: {x: x,y: y}\n```";
    let options = resolver::resolve_document_options_policy(source, &report, &[], false, false)
        .unwrap()
        .1;
    assert!(options.data["image.png"].starts_with("data:image/png;base64,"));
    let rendered = richview_core::document::render(source, &options).unwrap();
    assert!(rendered.html.contains("data:image/png;base64,"));
    fs::write(&report, source).unwrap();
    let preview = richview_app::server::Preview {
        source: report,
        styles: vec![],
        allow_network: false,
        last_good: Default::default(),
    };
    let a = richview_app::server::preview_source_revision(&preview).unwrap();
    let b = richview_app::server::preview_source_revision(&preview).unwrap();
    assert_eq!(a, b);
    fs::write(f.root.join("data.csv"), "x,y\nA,12\nB,3").unwrap();
    let c = richview_app::server::preview_source_revision(&preview).unwrap();
    assert_ne!(a, c);
    fs::write(
        f.root.join("custom.rstyle.yaml"),
        "tokens:\n  accent: '#123456'\n  bg: white\n",
    )
    .unwrap();
    let d = richview_app::server::preview_source_revision(&preview).unwrap();
    assert_ne!(c, d);
    fs::write(
        f.root.join("image.png"),
        [137u8, 80, 78, 71, 13, 10, 26, 10, 0],
    )
    .unwrap();
    let e = richview_app::server::preview_source_revision(&preview).unwrap();
    assert_ne!(d, e);
    let (document, options) =
        resolver::resolve_document_options_policy(source, &preview.source, &[], false, false)
            .unwrap();
    assert_eq!(document.blocks.len(), 1);
    assert!(options.runtime_wasm.is_none());
    assert!(options.runtime_js.is_none());
    let checked = richview_core::document::check_document(document, &options).unwrap();
    assert!(!checked.diagnostics.iter().any(|d| d.level == "error"));
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn trusted_public_origin_and_real_http_publish_client() {
    let f = Fixture::new();
    let state = AppState {
        storage: f.storage.clone(),
        access: Access::open(
            f.storage.dir(),
            Visibility::Public,
            Some(Arc::from("local-token")),
        )
        .unwrap(),
        preview: None,
        public_origin: Some("https://reports.example.test".into()),
        report_origin: None,
    };
    let app = build_app(state);
    let (_, body) = request(
        &app,
        "POST",
        "/api/reports",
        json!({"slug":"origin","source":source("0.0.1","Origin")}),
        Some("local-token"),
    )
    .await;
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["url"],
        "https://reports.example.test/r/origin"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let published = richview_app::cli::publish(
        &source("0.0.1", "Real HTTP"),
        &richview_core::document::RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "http-fixture".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        },
        &format!("http://{addr}"),
        "local-token",
        Some("actual"),
        true,
    )
    .await
    .unwrap();
    assert_eq!(published["created"], true);
    assert_eq!(published["url"], format!("http://{addr}/r/actual"));
    assert_eq!(
        f.storage.get("actual", None).unwrap().unwrap().source,
        source("0.0.1", "Real HTTP")
    );
    assert!(
        richview_app::cli::publish(
            "# draft",
            &Default::default(),
            &format!("http://{addr}"),
            "local-token",
            Some("draft"),
            true
        )
        .await
        .is_err()
    );
    assert!(
        richview_app::cli::publish(
            &source("0.0.1", "Policy"),
            &Default::default(),
            "http://example.com",
            "local-token",
            None,
            true
        )
        .await
        .is_err()
    );
    worker.abort();
}

#[test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
fn the_preview_page_may_poll_its_own_host_for_the_next_revision() {
    let f = Fixture::new();
    let report = f.root.join("report.rmd");
    fs::write(&report, "---\ntitle: T\ndescription: d\n---\n\nHello.\n").unwrap();
    let preview = richview_app::server::Preview {
        source: report,
        styles: vec![],
        allow_network: false,
        last_good: Default::default(),
    };
    let (rendered, revision) = richview_app::server::preview_render(&preview).unwrap();
    // The policy is a meta attribute, so its quotes are escaped; it must allow the poll.
    assert!(
        rendered.html.contains("connect-src &#39;self&#39;"),
        "the preview policy blocks the reload poll"
    );
    assert!(rendered.html.contains(&format!("content=\"{revision}\"")));
}

#[test]
fn validate_passes_over_a_warehouse_block_and_still_reports_everything_else() {
    let f = Fixture::new();
    let source = "---\ntitle: T\ndescription: d\nversion: 0.0.1\n---\n\n```chart\ntype: bar\ntitle: From the warehouse\ndata:\n  sql:\n    connector: warehouse\n    query: q.sql\nencoding: {x: month, y: orders}\n```\n\n```chart\ntype: bar\ntitle: From a file\ndata: {csv: sales.csv}\nencoding: {x: month, y: value}\n```\n";
    let report = f.root.join("report.rmd");
    fs::write(&report, source).unwrap();
    let input = richview_app::ops::Input {
        path: report,
        ..Default::default()
    };
    let result = richview_app::ops::validate(&input, false).unwrap();
    let codes: Vec<&str> = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(
        codes.contains(&"report.asset.missing"),
        "the missing csv is still a finding: {result}"
    );
    // The connector is not declared, which is a finding with a fix in the author's terms.
    let undeclared = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "resource.resolve")
        .unwrap_or_else(|| panic!("an undeclared connector is a finding: {result}"));
    assert!(
        undeclared["fix"]
            .as_str()
            .unwrap()
            .contains("report.connectors.yaml")
    );
    // Declared, the warehouse block is passed over.
    fs::write(
        f.root.join("report.connectors.yaml"),
        "connectors:\n  warehouse:\n    kind: duckdb\n    database: data/w.duckdb\n",
    )
    .unwrap();
    let result = richview_app::ops::validate(&input, false).unwrap();
    assert!(
        !result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "resource.resolve"),
        "the warehouse block is passed over: {result}"
    );
}

#[test]
fn attached_connector_failures_keep_their_complete_diagnostics() {
    let f = Fixture::new();
    fs::write(
        f.root.join("report.connectors.yaml"),
        "connectors:\n  warehouse:\n    kind: duckdb\n    database: data/w.duckdb\n",
    )
    .unwrap();
    let registry = richview_app::connectors::Registry::new(f.root.join("cache"));
    for (params, code, message, path) in [
        (
            "",
            "data.connector.unknown",
            "no connector named 'warehouse' is registered",
            "table.data",
        ),
        (
            "    params: {invalid: null}\n",
            "data.connector.rejected",
            "query rejected: data.sql.params values must be strings, numbers, booleans or date strings",
            "table.data.sql.params",
        ),
    ] {
        let source = format!(
            "---\ntitle: T\n---\n\n```table\ndata:\n  sql:\n    connector: warehouse\n    inline: SELECT 1 AS value\n{params}```\n"
        );
        let report = f.root.join("report.rmd");
        fs::write(&report, &source).unwrap();
        let error = resolver::resolve_document_options_full(
            &source,
            &report,
            &[],
            false,
            false,
            Some(&registry),
        )
        .unwrap_err();
        let diagnostics: Vec<richview_core::document::Diagnostic> =
            serde_json::from_str(&error).unwrap();
        let mut expected = richview_core::document::Diagnostic::new("error", code, message, path);
        expected.line = Some(5);
        expected.tag = Some("table".into());
        assert_eq!(diagnostics, vec![expected]);
    }
}

#[tokio::test]
async fn publish_refusals_say_ok_false_with_their_diagnostics() {
    let f = Fixture::new();
    let app = f.app(Some("token"));
    for body in [
        json!({"slug":"x","source":"# valid local draft"}),
        json!({"slug":"x","source":"---\ntitle: [\n---\n"}),
    ] {
        let (status, text) = request(&app, "POST", "/api/reports", body, Some("token")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["ok"], json!(false), "{text}");
        assert!(value["diagnostics"].is_array(), "{text}");
        // The failure object every surface returns (TYP-54), with TYP-70's one-sentence `error`.
        assert_eq!(value["status"], json!("issues"), "{text}");
        assert_eq!(value["output"], Value::Null, "{text}");
        assert!(
            value["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{text}"
        );
    }
    // A refused caller gets the same object, not a bare message.
    let (status, text) = request(
        &app,
        "POST",
        "/api/reports",
        json!({"slug":"x","source":"# x"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{text}");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        (&value["ok"], &value["status"], &value["output"]),
        (&json!(false), &json!("issues"), &Value::Null),
        "{text}"
    );
    assert_eq!(value["diagnostics"], json!([]), "{text}");
}

#[tokio::test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn serve_answers_the_preview_its_thumbnail_and_nothing_else_and_starts_on_a_broken_source() {
    let f = Fixture::new();
    let folder = f.root.join("serve-report");
    fs::create_dir(&folder).unwrap();
    let source = folder.join("report.rmd");
    // Broken from the start: the banner, not a refusal.
    fs::remove_file(&source).ok();
    fs::write(&source, "---\ntitle: [unclosed\n---\n").unwrap();
    let app = build_app(AppState {
        storage: f.storage.clone(),
        access: Access::open(f.storage.dir(), Visibility::Public, None).unwrap(),
        preview: Some(richview_app::server::Preview {
            source: source.clone(),
            styles: vec![],
            allow_network: false,
            last_good: Default::default(),
        }),
        public_origin: None,
        report_origin: None,
    });
    let (status, html) = request(&app, "GET", "/", json!({}), None).await;
    assert_eq!(status, StatusCode::OK, "{html}");
    assert!(html.contains("richview-banner"), "{html}");
    // Nothing of the publishing host is served.
    for path in [
        "/api/reports",
        "/api/session",
        "/r/anything",
        "/ui/governance",
    ] {
        let (status, _) = request(&app, "GET", path, json!({}), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    // Fixed, the thumbnail is the 1200x630 card of the story.
    fs::write(
        &source,
        "---\ntitle: Served story\ndescription: d\n---\n\nHello.\n",
    )
    .unwrap();
    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/thumbnail")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "image/png");
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 24)
        .await
        .unwrap();
    assert_eq!(&bytes[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    assert_eq!(&bytes[16..24], &[0, 0, 4, 176, 0, 0, 2, 118], "1200 x 630");
}
