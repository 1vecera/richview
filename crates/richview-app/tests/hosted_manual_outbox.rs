//! Manual outbox retains requests, issuing a one-hour link only when the operator dispatches.
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use richview_app::{
    access::Access,
    mail::Outbox,
    server::{
        AppState, build_app,
        hosted::{OperatorAction, operator_action},
    },
    storage::FsStorage,
};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;

async fn signup(app: &axum::Router, email: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ui/sign-up")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header("x-real-ip", "192.0.2.44")
                .body(Body::from(format!("email={email}")))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}
async fn wait_request(path: &Path) -> std::path::PathBuf {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(entry) = std::fs::read_dir(path)
                .unwrap()
                .flatten()
                .find(|e| e.file_name().to_string_lossy().ends_with(".request.json"))
            {
                break entry.path();
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn manual_outbox_does_not_issue_a_link_until_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = dir.path().join("outbox");
    let access = Access::open_hosted(dir.path()).unwrap();
    access
        .configure_mail(Arc::new(Outbox::open(&outbox).unwrap()), 2, true)
        .unwrap();
    let app = build_app(AppState {
        storage: FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true)
            .with_quota(Some(100_000_000)),
        access,
        preview: None,
        public_origin: Some("https://app.test".into()),
        report_origin: Some("https://reports.test".into()),
    });
    let result = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ui/sign-up")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("email=late%40example.test"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(result.status(), StatusCode::OK);
    let file = wait_request(&outbox).await;
    let contents = std::fs::read_to_string(file).unwrap();
    assert!(
        !contents.contains("/link/"),
        "outbox must hold the request, not a live link"
    );
    assert!(
        !dir.path().join(".access/links.json").exists(),
        "link must not be issued before dispatch"
    );
}

#[tokio::test]
async fn delayed_manual_dispatch_issues_a_fresh_link_and_filters_expired_requests() {
    let dir = tempfile::tempdir().unwrap();
    let outbox = dir.path().join("outbox");
    let access = Access::open_hosted(dir.path()).unwrap();
    access
        .configure_mail(Arc::new(Outbox::open(&outbox).unwrap()), 2, true)
        .unwrap();
    let state = AppState {
        storage: FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true)
            .with_quota(Some(100_000_000)),
        access: access.clone(),
        preview: None,
        public_origin: Some("https://app.test".into()),
        report_origin: Some("https://reports.test".into()),
    };
    let app = build_app(state.clone());
    assert_eq!(signup(&app, "late%40example.test").await, StatusCode::OK);
    let file = wait_request(&outbox).await;
    let mut request: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert!(!request.to_string().contains("/link/"));
    let id = request["id"].as_str().unwrap().to_owned();
    request["created"] = serde_json::json!(request["created"].as_u64().unwrap() - 7200);
    std::fs::write(&file, serde_json::to_vec(&request).unwrap()).unwrap();
    let pending = operator_action(&state, OperatorAction::PendingLinks)
        .await
        .unwrap();
    assert_eq!(pending["manualRequests"].as_array().unwrap().len(), 1);
    let message = operator_action(&state, OperatorAction::DispatchManual { id: id.clone() })
        .await
        .unwrap();
    let link = message["body"]
        .as_str()
        .unwrap()
        .lines()
        .find(|line| line.starts_with("https://app.test/link/"))
        .unwrap();
    let path = reqwest::Url::parse(link)
        .unwrap()
        .path()
        .replacen("/link/", "/ui/link/", 1);
    let links: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join(".access/links.json")).unwrap())
            .unwrap();
    assert_eq!(
        links[0]["expires"].as_u64().unwrap() - links[0]["created"].as_u64().unwrap(),
        24 * 3600
    );
    assert_eq!(
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "name=Late&password=a-long-password&password_confirm=a-long-password"
                    ))
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    assert!(
        operator_action(&state, OperatorAction::DispatchManual { id })
            .await
            .is_err()
    );
    assert_eq!(
        operator_action(&state, OperatorAction::PendingLinks)
            .await
            .unwrap()["manualRequests"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    request["status"] = serde_json::json!("pending");
    request["expires"] = serde_json::json!(1);
    std::fs::write(&file, serde_json::to_vec(&request).unwrap()).unwrap();
    assert_eq!(
        operator_action(&state, OperatorAction::PendingLinks)
            .await
            .unwrap()["manualRequests"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let marked: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(marked["status"], "expired");
}

#[tokio::test]
async fn manual_reset_and_existing_account_requests_dispatch_and_unknown_reset_is_a_decoy() {
    use richview_app::access::LinkPurpose;
    let dir = tempfile::tempdir().unwrap();
    let outbox = dir.path().join("outbox");
    let access = Access::open_hosted(dir.path()).unwrap();
    let link = access
        .issue_link("known@example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    access
        .complete_hosted_link(&link, "Known", "a-long-password")
        .await
        .unwrap();
    access
        .configure_mail(Arc::new(Outbox::open(&outbox).unwrap()), 10, true)
        .unwrap();
    let state = AppState {
        storage: FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true)
            .with_quota(Some(100_000_000)),
        access,
        preview: None,
        public_origin: Some("https://app.test".into()),
        report_origin: Some("https://reports.test".into()),
    };
    let app = build_app(state.clone());
    assert_eq!(signup(&app, "known%40example.test").await, StatusCode::OK);
    for email in ["known%40example.test", "unknown%40example.test"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ui/forgot")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("email={email}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    let pending = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let pending = operator_action(&state, OperatorAction::PendingLinks)
                .await
                .unwrap();
            if pending["manualRequests"].as_array().unwrap().len() == 2 {
                break pending;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let has_decoy = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let found = std::fs::read_dir(&outbox)
                .unwrap()
                .flatten()
                .filter_map(|entry| {
                    let bytes = std::fs::read(entry.path()).ok()?;
                    let record: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
                    Some(record["to"] == "unknown@example.test" && record["status"] == "decoy")
                })
                .any(|found| found);
            if found {
                break true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    assert_eq!(
        has_decoy.ok(),
        Some(true),
        "unknown reset must retain a hidden rate record"
    );
    for request in pending["manualRequests"].as_array().unwrap() {
        assert_eq!(request["to"], "known@example.test");
        let expected = if request["kind"] == "reset" {
            "Reset your RichView password"
        } else {
            "Your RichView account"
        };
        let mail = operator_action(
            &state,
            OperatorAction::DispatchManual {
                id: request["id"].as_str().unwrap().into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(mail["subject"], expected);
        assert!(
            mail["body"]
                .as_str()
                .unwrap()
                .contains("https://app.test/link/")
        );
    }
    assert_eq!(
        operator_action(&state, OperatorAction::PendingLinks)
            .await
            .unwrap()["manualRequests"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn anonymous_manual_history_cannot_crowd_out_an_existing_account_reset() {
    use richview_app::access::LinkPurpose;
    let dir = tempfile::tempdir().unwrap();
    let outbox = dir.path().join("outbox");
    let access = Access::open_hosted(dir.path()).unwrap();
    let signup = access
        .issue_link("known@example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    access
        .complete_hosted_link(&signup, "Known", "a-long-password")
        .await
        .unwrap();
    let transport = Arc::new(Outbox::open(&outbox).unwrap());
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for i in 0..5_000 {
        let id = format!("{i:032x}");
        let record = serde_json::json!({
            "id":id.clone(), "to":format!("unknown{i}@example.test"), "kind":"reset",
            "created":at, "expires":at + 86400, "status":"decoy"
        });
        std::fs::write(
            outbox.join(format!("{id}.request.json")),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
    }
    access.configure_mail(transport, 1_000, true).unwrap();
    let state = AppState {
        storage: FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true)
            .with_quota(Some(100_000_000)),
        access,
        preview: None,
        public_origin: Some("https://app.test".into()),
        report_origin: Some("https://reports.test".into()),
    };
    let app = build_app(state.clone());
    for email in ["another%40example.test", "known%40example.test"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/ui/forgot")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("email={email}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    let pending = operator_action(&state, OperatorAction::PendingLinks)
        .await
        .unwrap();
    let requests = pending["manualRequests"].as_array().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["to"], "known@example.test");
    let mail = operator_action(
        &state,
        OperatorAction::DispatchManual {
            id: requests[0]["id"].as_str().unwrap().into(),
        },
    )
    .await
    .unwrap();
    let link = mail["body"]
        .as_str()
        .unwrap()
        .lines()
        .find(|line| line.starts_with("https://app.test/link/"))
        .unwrap();
    let post = reqwest::Url::parse(link)
        .unwrap()
        .path()
        .replacen("/link/", "/ui/link/", 1);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(post)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "password=a-new-password&password_confirm=a-new-password",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        state
            .access
            .sign_in("known@example.test", "a-new-password")
            .is_ok()
    );
}

#[tokio::test]
async fn both_delivery_modes_reject_mailbox_syntax_and_limit_the_normalized_address() {
    for manual in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let outbox = dir.path().join("outbox");
        let access = Access::open_hosted(dir.path()).unwrap();
        access
            .configure_mail(Arc::new(Outbox::open(&outbox).unwrap()), 100, manual)
            .unwrap();
        let app = build_app(AppState {
            storage: FsStorage::new(dir.path())
                .unwrap()
                .with_hosted(true)
                .with_quota(Some(100_000_000)),
            access: access.clone(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        });
        for route in ["/ui/sign-up", "/ui/forgot"] {
            for name in ["a", "b", "c", "d"] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(route)
                            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                            .body(Body::from(format!(
                                "email={name}%3Cvictim%40example.test%3E"
                            )))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
            }
        }
        assert!(std::fs::read_dir(&outbox).unwrap().next().is_none());
        for email in [
            "Victim%40Example.test",
            "victim%40example.test",
            "VICTIM%40example.test",
            "victim%40example.test",
        ] {
            assert_eq!(signup(&app, email).await, StatusCode::OK);
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let count = std::fs::read_dir(&outbox)
                    .unwrap()
                    .flatten()
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if manual {
                            name.ends_with(".request.json")
                        } else {
                            name.ends_with(".txt")
                        }
                    })
                    .count();
                if count == 3 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let entries: Vec<_> = std::fs::read_dir(&outbox).unwrap().flatten().collect();
        assert_eq!(entries.len(), 3);
        for entry in entries {
            let content = std::fs::read_to_string(entry.path()).unwrap();
            assert!(content.contains("victim@example.test"));
            assert!(!content.contains("Victim@"));
        }
        if !manual {
            let links = std::fs::read_to_string(dir.path().join(".access/links.json")).unwrap();
            assert_eq!(links.matches("victim@example.test").count(), 3);
        }
    }
}
