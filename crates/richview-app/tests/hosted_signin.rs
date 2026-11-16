//! A hosted sign-in response does not reveal whether an email is registered.
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use richview_app::{
    access::{Access, LinkPurpose},
    server::{AppState, build_app},
    storage::FsStorage,
};
use serde_json::json;
use tower::ServiceExt;

async fn fixture() -> (tempfile::TempDir, axum::Router) {
    let dir = tempfile::tempdir().unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    let link = access
        .issue_link("known@example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    access
        .complete_hosted_link(&link, "Known", "known password long")
        .await
        .unwrap();
    let app = build_app(AppState {
        storage: FsStorage::new(dir.path()).unwrap().with_hosted(true),
        access,
        preview: None,
        public_origin: Some("https://app.test".into()),
        report_origin: Some("https://reports.test".into()),
    });
    (dir, app)
}
async fn sign_in(app: &axum::Router, email: &str, password: &str) -> (StatusCode, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("host", "app.test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"email":email,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status(),
        to_bytes(response.into_body(), 65536)
            .await
            .unwrap()
            .to_vec(),
    )
}

#[tokio::test]
async fn known_and_unknown_have_the_same_api_response_after_either_bucket_fills() {
    let (_dir, app) = fixture().await;
    for i in 0..5 {
        assert_eq!(
            sign_in(
                &app,
                &format!("unknown-{i}@example.test"),
                "wrong password long"
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let unknown_blocked =
        sign_in(&app, "another-unknown@example.test", "wrong password long").await;
    let known_wrong = sign_in(&app, "known@example.test", "wrong password long").await;
    assert_eq!(unknown_blocked.0, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown_blocked, known_wrong);
    assert_eq!(
        sign_in(&app, "known@example.test", "known password long")
            .await
            .0,
        StatusCode::OK
    );

    let (_other_dir, other) = fixture().await;
    for _ in 0..5 {
        assert_eq!(
            sign_in(&other, "known@example.test", "wrong password long")
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let known_blocked = sign_in(&other, "known@example.test", "wrong password long").await;
    let unknown_fresh = sign_in(&other, "missing@example.test", "wrong password long").await;
    assert_eq!(known_blocked.0, StatusCode::UNAUTHORIZED);
    assert_eq!(known_blocked, unknown_fresh);
}
