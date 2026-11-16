//! Hosted download admission remains held until each response body is released.
#![cfg(feature = "embedded-runtime")]

use axum::{
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
};
use richview_app::{
    access::Access,
    server::{AppState, reports::report_download},
    storage::{FsStorage, StoredReport},
};
use serde_json::json;
use std::collections::BTreeMap;

#[tokio::test]
async fn hosted_download_permits_remain_held_while_responses_exist() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".access")).unwrap();
    let access_state = json!({"schema":2,"tenancy":"hosted",
        "organizations":[{"id":"aaaa0001","name":"A","created":"now","limits":{}}],
        "memberships":[], "users":[], "sessions":[],
        "acls":{"aaaa0001/page":{"org":"aaaa0001","owner":"owner","visibility":"public","grants":[]}}});
    std::fs::write(
        dir.path().join(".access/state.json"),
        serde_json::to_vec(&access_state).unwrap(),
    )
    .unwrap();
    let storage = FsStorage::new(dir.path()).unwrap().with_hosted(true);
    storage
        .put_version(StoredReport {
            slug: "aaaa0001/page".into(),
            version: "1.0.0".into(),
            source: "---\ntitle: Page\ndescription: A page\nversion: 1.0.0\n---\n\n# Page\n".into(),
            report_folder: Some(richview_core::document::ReportFolder {
                root: "report".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            html: "<html>page</html>".into(),
            frontmatter: json!({"title":"Page"}),
            created_at: "now".into(),
            styles: vec![],
            data: BTreeMap::new(),
            files: BTreeMap::new(),
            content_sha256: String::new(),
        })
        .unwrap();
    let state = AppState {
        storage,
        access: Access::open_hosted(dir.path()).unwrap(),
        preview: None,
        public_origin: None,
        report_origin: None,
    };
    let download = || {
        report_download(
            State(state.clone()),
            HeaderMap::new(),
            AxumPath(("aaaa0001/page".into(), "1.0.0".into())),
        )
    };
    let first = download().await;
    let second = download().await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(download().await.status(), StatusCode::TOO_MANY_REQUESTS);
    drop(first);
    assert_eq!(download().await.status(), StatusCode::OK);
    drop(second);
}
