//! Report identity in the store and server: republishing, content identity, retention and
//! `latest` (TYP-63, TYP-64, TYP-95, TYP-96).
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use richview_app::access::{Access, Visibility};
use richview_app::{
    server::{AppState, build_app},
    storage::{FsStorage, PublishError, StoredReport, content_sha256},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc};
use tower::ServiceExt;

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::current_dir().unwrap().join("tmp").join(format!(
            "identity-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("public")).unwrap();
        Self(root)
    }
    fn app(&self, quota: Option<u64>) -> axum::Router {
        let store = self.0.join("store");
        build_app(AppState {
            storage: FsStorage::new(&store).unwrap().with_quota(quota),
            access: Access::open(&store, Visibility::Public, Some(Arc::from("token"))).unwrap(),
            preview: None,
            public_origin: None,
            report_origin: None,
        })
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let mut body = body;
    if body.get("source").is_some() && body.get("reportFolder").is_none() {
        let files: Vec<String> = std::iter::once("report.rmd".to_owned())
            .chain(
                body["data"]
                    .as_object()
                    .into_iter()
                    .flat_map(|m| m.keys().cloned()),
            )
            .collect();
        body["reportFolder"] =
            json!({"root":"identity-fixture","source":"report.rmd","files":files});
    }
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("host", "127.0.0.1:4711")
        .header("authorization", "Bearer token")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

fn doc(version: &str, heading: &str) -> String {
    format!("---\ntitle: Identity\ndescription: D\nversion: \"{version}\"\n---\n\n# {heading}\n")
}

async fn publish(
    app: &axum::Router,
    slug: &str,
    version: &str,
    heading: &str,
) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        "/api/reports",
        json!({"slug":slug,"source":doc(version, heading)}),
    )
    .await
}

fn report(slug: &str, version: &str, source: &str) -> StoredReport {
    StoredReport {
        slug: slug.into(),
        version: version.into(),
        source: source.into(),
        report_folder: None,
        html: "<h1>x</h1>".into(),
        frontmatter: json!({}),
        created_at: "now".into(),
        styles: vec![],
        data: BTreeMap::new(),
        files: Default::default(),
        content_sha256: String::new(),
    }
}

#[test]
fn account_quota_counts_only_the_owners_reports_and_keeps_refused_slugs_free() {
    let dir = Dir::new();
    let probe = Dir::new();
    let probe_store = FsStorage::new(probe.0.join("store")).unwrap();
    probe_store
        .put_version(report("first", "1.0.0", "x"))
        .unwrap();
    let size = std::fs::read_dir(probe.0.join("store/first/1.0.0"))
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .sum::<u64>();
    let store = FsStorage::new(dir.0.join("store"))
        .unwrap()
        .with_account_quota(Some(size + 5));
    let first = report("first", "1.0.0", "x");
    store.put_version_for_owner(first.clone(), &[]).unwrap();
    assert!(
        !store
            .put_version_for_owner(first, &["first".into()])
            .unwrap()
            .1
    );
    assert!(matches!(
        store.put_version_for_owner(report("second", "1.0.0", "x"), &["first".into()]),
        Err(PublishError::AccountQuota { .. })
    ));
    assert!(!dir.0.join("store/second").exists());
    // A different owner has an independent allowance and can claim the refused address.
    store
        .put_version_for_owner(report("second", "1.0.0", "x"), &[])
        .unwrap();
}

#[test]
fn deleting_a_report_removes_all_versions_and_permanently_retires_its_address() {
    let dir = Dir::new();
    let store = FsStorage::new(dir.0.join("store")).unwrap();
    store
        .put_version(report("finished", "1.0.0", "one"))
        .unwrap();
    store
        .put_version(report("finished", "2.0.0", "two"))
        .unwrap();
    store.set_pin("finished", "1.0.0").unwrap();
    assert!(store.delete_report("finished").unwrap());
    assert!(store.list().unwrap().is_empty());
    assert!(store.get("finished", Some("1.0.0")).unwrap().is_none());
    assert!(store.is_deleted("finished", "1.0.0").unwrap());
    assert!(!dir.0.join("store/finished/1.0.0/source.rmd").exists());
    assert!(store.has_slug("finished").unwrap());
    assert!(matches!(
        store.put_version(report("finished", "3.0.0", "three")),
        Err(PublishError::ReportDeleted)
    ));
    assert!(
        store.delete_report("finished").unwrap(),
        "cleanup can be retried"
    );
}

#[test]
fn interrupted_report_deletion_hides_the_report_and_a_retry_removes_its_files() {
    let dir = Dir::new();
    let store = FsStorage::new(dir.0.join("store")).unwrap();
    store
        .put_version(report("interrupted", "1.0.0", "secret"))
        .unwrap();
    std::fs::write(dir.0.join("store/interrupted/.deleted"), b"deleted").unwrap();
    assert!(store.get("interrupted", Some("1.0.0")).unwrap().is_none());
    assert!(store.latest_version("interrupted").unwrap().is_none());
    assert!(store.list().unwrap().is_empty());
    assert!(store.delete_report("interrupted").unwrap());
    assert!(!dir.0.join("store/interrupted/1.0.0").exists());
}

#[tokio::test]
#[ignore = "release: publication needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn republishing_identical_content_succeeds_and_changed_content_conflicts() {
    let dir = Dir::new();
    let app = dir.app(None);
    let (status, first) = publish(&app, "same", "0.0.1", "one").await;
    assert_eq!(
        (status, first["created"].clone()),
        (StatusCode::OK, json!(true))
    );
    let (status, retry) = publish(&app, "same", "0.0.1", "one").await;
    assert_eq!(
        (status, retry["created"].clone()),
        (StatusCode::OK, json!(false))
    );
    assert_eq!(retry["contentSha256"], first["contentSha256"]);
    let (status, conflict) = publish(&app, "same", "0.0.1", "two").await;
    assert_eq!(status, StatusCode::CONFLICT);
    let diagnostic = &conflict["diagnostics"][0];
    assert_eq!(
        (diagnostic["level"].as_str(), diagnostic["code"].as_str()),
        (Some("error"), Some("publish.version-exists"))
    );
    assert_eq!(
        diagnostic["details"]["storedContentSha256"],
        first["contentSha256"]
    );
    assert!(diagnostic["message"].as_str().unwrap().contains("Bump"));
    let raw = call(&app, "GET", "/raw/same/v/0.0.1", json!({})).await.1;
    assert!(raw.as_str().unwrap().contains("# one"));
}

#[tokio::test]
#[ignore = "release: publication needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn a_changed_data_file_or_style_under_one_version_is_a_conflict() {
    let dir = Dir::new();
    let app = dir.app(None);
    let source = "---\ntitle: Data\ndescription: D\nversion: \"1.0.0\"\n---\n\n```chart\ntype: bar\ntitle: T\ndata: {csv: data.csv}\nencoding: {x: a, y: b}\n```\n";
    let body = |csv: &str, style: &str| json!({"slug":"data","source":source,"styles":[style],"data":{"data.csv":csv}});
    let (status, _) = call(&app, "POST", "/api/reports", body("a,b\nx,1", "scientific")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&app, "POST", "/api/reports", body("a,b\nx,1", "scientific")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, changed) =
        call(&app, "POST", "/api/reports", body("a,b\nx,2", "scientific")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{changed}");
    let (status, _) = call(&app, "POST", "/api/reports", body("a,b\nx,1", "brutalist")).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[test]
fn content_identity_covers_source_ordered_styles_and_files_but_not_slug_or_version() {
    let files = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    };
    let base = content_sha256("# a", &["x".into(), "y".into()], &files(&[("d.csv", "1")]));
    assert_eq!(
        base,
        content_sha256("# a", &["x".into(), "y".into()], &files(&[("d.csv", "1")]))
    );
    for other in [
        content_sha256("# b", &["x".into(), "y".into()], &files(&[("d.csv", "1")])),
        content_sha256("# a", &["y".into(), "x".into()], &files(&[("d.csv", "1")])),
        content_sha256("# a", &["x".into(), "y".into()], &files(&[("d.csv", "2")])),
        content_sha256("# a", &["x".into(), "y".into()], &files(&[("e.csv", "1")])),
        content_sha256("# a", &["x".into(), "y".into()], &files(&[])),
        // Framing: moving bytes between path and content changes the hash.
        content_sha256("# a", &["x".into(), "y".into()], &files(&[("d.cs", "v1")])),
    ] {
        assert_ne!(base, other);
    }
    let storage_dir = Dir::new();
    let storage = FsStorage::new(storage_dir.0.join("store")).unwrap();
    let (a, _) = storage.put_version(report("one", "1.0.0", "# s")).unwrap();
    let (b, _) = storage.put_version(report("two", "2.0.0", "# s")).unwrap();
    assert_eq!(
        a.content_sha256, b.content_sha256,
        "slug and version are not identity"
    );
}

#[test]
fn a_version_stored_with_the_old_source_only_hash_still_accepts_an_identical_retry() {
    let dir = Dir::new();
    let storage = FsStorage::new(dir.0.join("store")).unwrap();
    let mut input = report("old", "1.0.0", "# s");
    input.created_at = "2026-10-29T12:00:00Z".into();
    input.report_folder = Some(richview_core::document::ReportFolder {
        root: "legacy".into(),
        source: "report.rmd".into(),
        files: vec!["report.rmd".into()],
    });
    storage.put_version(input.clone()).unwrap();
    let meta = dir.0.join("store/old/1.0.0/meta.json");
    let text = fs::read_to_string(&meta).unwrap();
    let mut value: Value = serde_json::from_str(&text).unwrap();
    assert!(value.get("updated_at").is_none());
    assert!(value.get("content_sha256").is_none());
    let created_at = value.as_object_mut().unwrap().remove("createdAt").unwrap();
    value["updated_at"] = created_at;
    value.as_object_mut().unwrap().remove("contentSha256");
    value["content_sha256"] = json!("legacy-source-only-hash");
    let old_meta = value.to_string();
    fs::write(&meta, &old_meta).unwrap();
    let assets = dir.0.join("store/old/1.0.0/assets.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&assets).unwrap()).unwrap();
    assert!(value.get("report_folder").is_none());
    let folder = value
        .as_object_mut()
        .unwrap()
        .remove("reportFolder")
        .unwrap();
    value["report_folder"] = folder;
    let old_assets = value.to_string();
    fs::write(&assets, &old_assets).unwrap();
    let mut retry = input.clone();
    retry.created_at = "2026-10-30T12:00:00Z".into();
    let (stored, created) = storage.put_version(retry).unwrap();
    assert!(!created);
    assert_ne!(stored.content_sha256, "legacy-source-only-hash");
    assert_eq!(stored.created_at, input.created_at);
    assert_eq!(
        serde_json::to_value(stored.report_folder).unwrap(),
        serde_json::to_value(input.report_folder).unwrap()
    );
    let summary = storage.summary("old", None).unwrap().unwrap();
    assert_eq!(summary["updatedAt"], input.created_at);
    assert!(summary.get("updated_at").is_none());
    assert_eq!(fs::read_to_string(&meta).unwrap(), old_meta);
    assert_eq!(fs::read_to_string(&assets).unwrap(), old_assets);
    assert!(matches!(
        storage.put_version(report("old", "1.0.0", "# changed")),
        Err(PublishError::Conflict { .. })
    ));
}

#[tokio::test]
#[ignore = "release: publication needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn latest_is_the_highest_release_and_a_lower_publication_never_moves_it() {
    let dir = Dir::new();
    let app = dir.app(None);
    for (version, heading) in [
        ("1.0.0-rc.1", "rc"),
        ("2.0.0", "two"),
        ("1.5.0", "one-five"),
        ("3.0.0-beta.1", "beta"),
    ] {
        assert_eq!(
            publish(&app, "ranked", version, heading).await.0,
            StatusCode::OK
        );
        let (_, html) = call(&app, "GET", "/r/ranked", json!({})).await;
        let expected = if version == "1.0.0-rc.1" { "rc" } else { "two" };
        assert!(
            html.as_str()
                .unwrap()
                .contains(&format!(">{expected}</h2>")),
            "after {version}"
        );
    }
    let (_, info) = call(&app, "GET", "/api/reports/ranked", json!({})).await;
    assert_eq!(info["version"], "2.0.0");
    assert_eq!(info["pinned"], Value::Null);
}

#[tokio::test]
#[ignore = "release: publication needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn a_pre_release_is_latest_only_while_no_release_exists() {
    let dir = Dir::new();
    let app = dir.app(None);
    publish(&app, "pre", "1.0.0-alpha.1", "alpha").await;
    publish(&app, "pre", "1.0.0-alpha.2", "alpha2").await;
    assert_eq!(
        call(&app, "GET", "/api/reports/pre", json!({})).await.1["version"],
        "1.0.0-alpha.2"
    );
    publish(&app, "pre", "0.1.0", "release").await;
    assert_eq!(
        call(&app, "GET", "/api/reports/pre", json!({})).await.1["version"],
        "0.1.0"
    );
}

#[tokio::test]
#[ignore = "release: publication needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn the_owner_can_pin_and_unpin_latest() {
    let dir = Dir::new();
    let app = dir.app(None);
    publish(&app, "pin", "1.0.0", "old").await;
    publish(&app, "pin", "2.0.0", "new").await;
    let (status, body) = call(
        &app,
        "PUT",
        "/api/reports/pin/latest",
        json!({"version":"1.0.0"}),
    )
    .await;
    assert_eq!(
        (status, body["pinned"].clone()),
        (StatusCode::OK, json!("1.0.0"))
    );
    let (_, html) = call(&app, "GET", "/r/pin", json!({})).await;
    assert!(html.as_str().unwrap().contains(">old</h2>"));
    let (_, info) = call(&app, "GET", "/api/reports/pin", json!({})).await;
    assert_eq!(info["pinned"], "1.0.0");
    publish(&app, "pin", "3.0.0", "newest").await;
    assert_eq!(
        call(&app, "GET", "/api/reports/pin", json!({})).await.1["version"],
        "1.0.0"
    );
    let (status, missing) = call(
        &app,
        "PUT",
        "/api/reports/pin/latest",
        json!({"version":"9.9.9"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["diagnostics"][0]["code"], "publish.version-missing");
    assert_eq!(
        call(&app, "DELETE", "/api/reports/pin/latest", json!({}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "GET", "/api/reports/pin", json!({})).await.1["version"],
        "3.0.0"
    );
}

#[tokio::test]
#[ignore = "release: publication needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn write_routes_require_the_token() {
    let dir = Dir::new();
    let app = dir.app(None);
    let (status, published) = publish(&app, "auth", "1.0.0", "x").await;
    assert_eq!(status, StatusCode::OK, "{published}");
    for (method, path, body) in [
        ("DELETE", "/api/reports/auth/v/1.0.0", json!({})),
        (
            "PUT",
            "/api/reports/auth/latest",
            json!({"version":"1.0.0"}),
        ),
        ("DELETE", "/api/reports/auth/latest", json!({})),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path}"
        );
    }
}

#[tokio::test]
#[ignore = "release: publication needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn versions_are_never_pruned_and_deletion_leaves_a_tombstone() {
    let dir = Dir::new();
    let app = dir.app(None);
    for n in 0..52 {
        assert_eq!(
            publish(&app, "keep", &format!("1.0.{n}"), "x").await.0,
            StatusCode::OK
        );
    }
    let (_, versions) = call(&app, "GET", "/api/reports/keep/versions", json!({})).await;
    assert_eq!(versions.as_array().unwrap().len(), 52);
    assert_eq!(
        call(&app, "GET", "/r/keep/v/1.0.0", json!({})).await.0,
        StatusCode::OK
    );

    let (status, _) = call(&app, "DELETE", "/api/reports/keep/v/1.0.51", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        call(&app, "GET", "/r/keep/v/1.0.51", json!({})).await.0,
        StatusCode::GONE
    );
    assert_eq!(
        call(&app, "GET", "/raw/keep/v/1.0.51", json!({})).await.0,
        StatusCode::GONE
    );
    assert_eq!(
        call(&app, "GET", "/api/reports/keep/v/1.0.51", json!({}))
            .await
            .0,
        StatusCode::GONE
    );
    assert_eq!(
        call(&app, "GET", "/r/keep/v/1.0.7", json!({})).await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "GET", "/api/reports/keep", json!({})).await.1["version"],
        "1.0.50"
    );
    // The number is retired: neither the old nor new content can come back.
    for heading in ["x", "different"] {
        let (status, body) = publish(&app, "keep", "1.0.51", heading).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["diagnostics"][0]["code"], "publish.version-deleted");
    }
    assert_eq!(
        call(&app, "DELETE", "/api/reports/keep/v/1.0.51", json!({}))
            .await
            .0,
        StatusCode::OK,
        "deleting twice is idempotent"
    );
    assert_eq!(
        call(&app, "DELETE", "/api/reports/keep/v/7.7.7", json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    // Deleting the pinned version drops the pin.
    call(
        &app,
        "PUT",
        "/api/reports/keep/latest",
        json!({"version":"1.0.3"}),
    )
    .await;
    call(&app, "DELETE", "/api/reports/keep/v/1.0.3", json!({})).await;
    assert_eq!(
        call(&app, "GET", "/api/reports/keep", json!({})).await.1["version"],
        "1.0.50"
    );
}

#[tokio::test]
#[ignore = "release: publication needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn the_byte_quota_refuses_a_publication_and_stores_nothing() {
    let dir = Dir::new();
    // The shared runtime is kept once. Allow its fixed cost plus two and a half versions.
    let probe = Dir::new();
    assert_eq!(
        publish(&probe.app(None), "quota", "1.0.0", "x").await.0,
        StatusCode::OK
    );
    fn bytes(path: &std::path::Path) -> u64 {
        fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap())
            .map(|e| {
                if e.file_type().unwrap().is_dir() {
                    bytes(&e.path())
                } else {
                    e.metadata().unwrap().len()
                }
            })
            .sum()
    }
    let version_bytes = bytes(&probe.0.join("store/quota/1.0.0"));
    let fixed_bytes = bytes(&probe.0.join("store")) - version_bytes;
    let app = dir.app(Some(fixed_bytes + version_bytes * 5 / 2));
    let mut stored = 0;
    let mut refused = None;
    for n in 0..40 {
        let (status, body) = publish(&app, "quota", &format!("1.0.{n}"), "x").await;
        if status == StatusCode::OK {
            stored += 1;
        } else {
            refused = Some((status, body));
            break;
        }
    }
    assert_eq!(stored, 2, "room for two artifacts, not three");
    let (status, body) = refused.expect("the quota is reached");
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
    assert_eq!(body["diagnostics"][0]["code"], "publish.quota");
    assert_eq!(body["diagnostics"][0]["level"], "error");
    let versions = call(&app, "GET", "/api/reports/quota/versions", json!({}))
        .await
        .1;
    assert_eq!(versions.as_array().unwrap().len(), stored);
    let leftovers = fs::read_dir(dir.0.join("store/quota"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(".staging"))
        .count();
    assert_eq!(leftovers, 0);
    // An identical retry of a stored version needs no space and still succeeds.
    assert_eq!(publish(&app, "quota", "1.0.0", "x").await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_server_rejects_slugs_outside_the_grammar_with_a_diagnostic() {
    let dir = Dir::new();
    let app = dir.app(None);
    for slug in ["my_report", "Upper", "-x", "a--b", "", "../escape"] {
        let (status, body) = call(
            &app,
            "POST",
            "/api/reports",
            json!({"slug":slug,"source":doc("1.0.0","x")}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{slug:?}");
        if !slug.is_empty() {
            assert_eq!(body["diagnostics"][0]["code"], "publish.slug", "{slug}");
        }
    }
    let (status, body) = call(
        &app,
        "POST",
        "/api/reports",
        json!({"slug":"ok","source":"---\ntitle: T\ndescription: D\nversion: \"01.0.0\"\n---\n"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["code"] == "publish.metadata")
            .count(),
        1
    );
}
