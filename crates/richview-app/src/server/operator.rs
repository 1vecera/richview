//! A separate loopback-only operator API; administrative actions are audited by the backend.
use super::{
    AppState,
    hosted::{self, OperatorAction},
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use sha2::{Digest, Sha256};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

#[derive(Clone)]
struct OperatorState {
    token_hash: [u8; 32],
    app: AppState,
}

pub(super) fn token_matches(expected: &[u8; 32], token: &str) -> bool {
    let actual: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    actual
        .iter()
        .zip(expected)
        .fold(0u8, |difference, (a, b)| difference | (*a ^ *b))
        == 0
}

async fn authorize(State(state): State<OperatorState>, request: Request, next: Next) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if !token.is_some_and(|token| token_matches(&state.token_hash, token)) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

async fn execute(state: OperatorState, action: OperatorAction) -> Response {
    match hosted::operator_action(&state.app, action).await {
        Ok(value) => Json(value).into_response(),
        Err(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": message})),
        )
            .into_response(),
    }
}

async fn organizations(State(state): State<OperatorState>) -> Response {
    execute(state, OperatorAction::ListOrgs).await
}
async fn suspend(State(state): State<OperatorState>, Path(org): Path<String>) -> Response {
    execute(
        state,
        OperatorAction::SetSuspended {
            org,
            suspended: true,
        },
    )
    .await
}
async fn resume(State(state): State<OperatorState>, Path(org): Path<String>) -> Response {
    execute(
        state,
        OperatorAction::SetSuspended {
            org,
            suspended: false,
        },
    )
    .await
}
async fn remove_report(
    State(state): State<OperatorState>,
    Path((org, slug)): Path<(String, String)>,
) -> Response {
    execute(state, OperatorAction::DeleteReport { org, slug }).await
}
async fn disable_person(
    State(state): State<OperatorState>,
    Path(person): Path<String>,
) -> Response {
    execute(
        state,
        OperatorAction::DisablePerson {
            person,
            disabled: true,
        },
    )
    .await
}
#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeletePersonRequest {
    #[serde(default)]
    successors: std::collections::BTreeMap<String, String>,
}

async fn delete_person(
    State(state): State<OperatorState>,
    Path(person): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request = if body.is_empty() && !headers.contains_key(header::CONTENT_TYPE) {
        DeletePersonRequest::default()
    } else {
        let json = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
        if !json {
            return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
        }
        match serde_json::from_slice::<DeletePersonRequest>(&body) {
            Ok(request) => request,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    };
    let successors = request.successors;
    match hosted::delete_account(&state.app, person, successors).await {
        Ok(value) => Json(value).into_response(),
        Err(hosted::DeleteAccountError::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(hosted::DeleteAccountError::NeedsSuccessor) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Account is the last owner of an organization with other members; name an eligible successor for each organization"})),
        ).into_response(),
        Err(hosted::DeleteAccountError::Failed(message)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": message})),
        )
            .into_response(),
    }
}
async fn pause_signups(State(state): State<OperatorState>) -> Response {
    execute(state, OperatorAction::SetSignupsPaused { paused: true }).await
}
async fn open_signups(State(state): State<OperatorState>) -> Response {
    execute(state, OperatorAction::SetSignupsPaused { paused: false }).await
}
async fn waiting_links(State(state): State<OperatorState>) -> Response {
    execute(state, OperatorAction::PendingLinks).await
}
async fn dispatch_manual(State(state): State<OperatorState>, Path(id): Path<String>) -> Response {
    execute(state, OperatorAction::DispatchManual { id }).await
}
async fn summary(State(state): State<OperatorState>) -> Response {
    execute(state, OperatorAction::Summary).await
}

fn router(token: &str, app: AppState) -> Result<Router, String> {
    if token.len() < 32
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(
            "operator token must be at least 32 ASCII letters, digits, hyphens or underscores"
                .into(),
        );
    }
    let inspections =
        super::staff_access::router(app.clone(), token, super::staff_access::directory());
    let state = OperatorState {
        token_hash: Sha256::digest(token.as_bytes()).into(),
        app,
    };
    Ok(Router::new()
        .route("/operator/organizations", get(organizations))
        .route("/operator/organizations/{org}/suspend", post(suspend))
        .route("/operator/organizations/{org}/resume", post(resume))
        .route("/operator/reports/{org}/{slug}", delete(remove_report))
        .route("/operator/people/{person}/disable", post(disable_person))
        .route("/operator/people/{person}", delete(delete_person))
        .route("/operator/signups/pause", post(pause_signups))
        .route("/operator/signups/open", post(open_signups))
        .route("/operator/links", get(waiting_links))
        .route("/operator/mail/{id}/dispatch", post(dispatch_manual))
        .route("/operator/summary", get(summary))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize))
        .with_state(state)
        .merge(inspections))
}

/// Bind the operator API to IPv4 loopback only; it is never mounted on the public router.
pub(super) async fn serve(port: u16, token: Arc<str>, state: AppState) -> Result<(), String> {
    let app = router(&token, state.clone())?;
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .map_err(|e| format!("cannot bind operator listener: {e}"))?;
    // Log threshold crossings for the host's operator monitor. Never block request workers on scans.
    let monitor = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        let mut previous = (false, false);
        loop {
            interval.tick().await;
            let state = state.clone();
            let Ok(Ok((accounts, storage))) = tokio::task::spawn_blocking(move || {
                let accounts = state.access.operator_accounts_half_full();
                let used = state.storage.operator_total_bytes()?;
                Ok::<_, String>((accounts, state.storage.operator_storage_half_full(used)))
            })
            .await
            else {
                eprintln!("richview: operator capacity scan failed");
                continue;
            };
            if accounts && !previous.0 {
                eprintln!("richview: ALERT hosted account limit is at least 50% full");
            }
            if storage && !previous.1 {
                eprintln!("richview: ALERT hosted store quota is at least 50% full");
            }
            previous = (accounts, storage);
        }
    });
    let (stop, mut wait) = tokio::sync::watch::channel(false);
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        let _ = wait.changed().await;
    });
    let server = std::future::IntoFuture::into_future(server);
    tokio::pin!(server);
    let result = tokio::select! {
        result = &mut server => result.map_err(|e| format!("operator listener failed: {e}")),
        _ = super::shutdown_signal() => {
            let _ = stop.send(true);
            match tokio::time::timeout(std::time::Duration::from_secs(25), &mut server).await {
                Ok(result) => result.map_err(|e| format!("operator listener failed: {e}")),
                Err(_) => Ok(()),
            }
        }
    };
    monitor.abort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deletion_redacts_padded_clipped_sign_ins_in_current_and_rotated_logs() {
        use crate::{
            access::{Access, LinkPurpose},
            mail::Outbox,
            storage::FsStorage,
        };
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let email = format!(
            "alice@{}.{}.{}.test",
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(63)
        );
        assert_eq!(email.len(), 202);
        let token = access
            .issue_link(&email, LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        let user = access
            .complete_hosted_link(&token, "Alice", "a-long-password")
            .await
            .unwrap();
        access
            .configure_mail(
                Arc::new(Outbox::open(&dir.path().join("outbox")).unwrap()),
                20,
                true,
            )
            .unwrap();
        let state = AppState {
            storage: FsStorage::new(dir.path()).unwrap().with_hosted(true),
            access: access.clone(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let audit_dir = dir.path().join(".access");
        let body = reqwest::Url::parse_with_params(
            "https://app.test/",
            &[
                ("email", format!(" {}", email.to_uppercase())),
                ("password", "incorrect".into()),
            ],
        )
        .unwrap()
        .query()
        .unwrap()
        .to_owned();
        let app = super::super::build_app(state.clone());
        for attempt in 0..2 {
            let request = if attempt == 0 {
                Request::builder()
                    .method("POST")
                    .uri("/ui/sign-in")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body.clone()))
                    .unwrap()
            } else {
                Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({"email":format!(" {}", email.to_uppercase()), "password":"incorrect"})
                            .to_string(),
                    ))
                    .unwrap()
            };
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(
                response.status(),
                if attempt == 0 {
                    StatusCode::SEE_OTHER
                } else {
                    StatusCode::UNAUTHORIZED
                }
            );
            if attempt == 0 {
                std::fs::rename(
                    audit_dir.join("audit-refused.log"),
                    audit_dir.join("audit-refused.log.1"),
                )
                .unwrap();
            }
        }
        let secret = concat!("operator-fixture-", "0123456789abcdef");
        let response = router(secret, state)
            .unwrap()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/operator/people/{}", user.id))
                    .header(header::AUTHORIZATION, format!("Bearer {secret}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        for name in ["audit-refused.log.1", "audit-refused.log"] {
            let content = std::fs::read_to_string(audit_dir.join(name)).unwrap();
            let targets: Vec<String> = content
                .lines()
                .map(|line| {
                    serde_json::from_str::<serde_json::Value>(line).unwrap()["target"]
                        .as_str()
                        .unwrap()
                        .into()
                })
                .collect();
            assert_eq!(targets, ["[deleted]"], "{name}");
        }
    }

    #[tokio::test]
    async fn deleting_a_hosted_account_removes_its_data_and_leaves_others() {
        use crate::{
            access::{Access, LinkPurpose, Principal},
            mail::{ManualKind, Outbox},
            storage::{FsStorage, StoredReport},
        };
        use axum::{body::Body, http::Request};
        use serde_json::json;
        use sha2::{Digest, Sha256};
        use std::collections::BTreeMap;
        use tower::ServiceExt;

        std::fs::create_dir_all("tmp").unwrap();
        let root = tempfile::tempdir_in("tmp").unwrap();
        let access_dir = root.path().join(".access");
        std::fs::create_dir_all(&access_dir).unwrap();
        let hash = |value: &str| format!("{:x}", Sha256::digest(value.as_bytes()));
        let mut state = json!({"schema":2,"tenancy":"hosted",
            "organizations":[{"id":"aaaa0001","name":"A","created":"now","suspended":false,"limits":{}},{"id":"bbbb0002","name":"B","created":"now","suspended":false,"limits":{}}],
            "memberships":[{"person":"person-a","org":"aaaa0001","role":"owner","created":"now"},{"person":"person-b","org":"bbbb0002","role":"owner","created":"now"}],
            "users":[{"id":"person-a","name":"A","email":"alice@example.test","created":"now","verified":"now","tokens":[{"id":"ta","name":"CLI","created":"now","sha256":hash("token-a"),"org":"aaaa0001"}]},{"id":"person-b","name":"B","email":"malice@example.test","created":"now","verified":"now","tokens":[{"id":"tb","name":"CLI","created":"now","sha256":hash("token-b"),"org":"bbbb0002"}]}],
            "sessions":[{"sha256":hash("session-a"),"csrf":"csrf-a","user":"person-a","expires":4102444800u64},{"sha256":hash("session-b"),"csrf":"csrf-b","user":"person-b","expires":4102444800u64}],"acls":{}});
        state["organizations"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"cccc0003","name":"Shared","created":"now","limits":{}}));
        for person in ["person-c", "person-d"] {
            state["memberships"]
                .as_array_mut()
                .unwrap()
                .push(json!({"person":person,"org":"cccc0003","role":if person == "person-c" { "owner" } else { "reader" },"created":"now"}));
            state["users"].as_array_mut().unwrap().push(json!({"id":person,"name":person,"email":format!("{person}@example.test"),"created":"now","verified":"now"}));
        }
        state["invitations"] = json!([
            {"id":"i-sent-by-alice","org":"aaaa0001","email":"friend@example.test","role":"reader","inviter":"person-a","created":1,"expires":4102444800u64},
            {"id":"i-sent-by-bob","org":"bbbb0002","email":"friend@example.test","role":"reader","inviter":"person-b","created":1,"expires":4102444800u64}
        ]);
        let backup_state = serde_json::to_vec(&state).unwrap();
        std::fs::write(access_dir.join("state.json"), &backup_state).unwrap();
        let access = Access::open_hosted(root.path()).unwrap();
        std::fs::write(access_dir.join("state.v1.json"), &backup_state).unwrap();
        std::fs::write(
            access_dir.join("state.v1.partial-test.json"),
            b"{\"id\":\"person-a\"",
        )
        .unwrap();
        let outbox = root.path().join("outbox");
        access
            .configure_mail(Arc::new(Outbox::open(&outbox).unwrap()), 20, true)
            .unwrap();
        std::fs::write(
            outbox.join("legacy.txt"),
            "To: alice@example.test\nSubject: old link\n\nold link",
        )
        .unwrap();
        let queue = access.mail_queue().unwrap();
        for invitation in ["i-sent-by-alice", "i-sent-by-bob"] {
            queue
                .enqueue_manual_invitation("friend@example.test".into(), invitation.into())
                .await
                .unwrap();
        }
        let alice_request_id = queue
            .pending_manual()
            .unwrap()
            .into_iter()
            .find(|request| request.invitation.as_deref() == Some("i-sent-by-alice"))
            .unwrap()
            .id;
        let bob_request_id = queue
            .pending_manual()
            .unwrap()
            .into_iter()
            .find(|request| request.invitation.as_deref() == Some("i-sent-by-bob"))
            .unwrap()
            .id;
        std::fs::copy(
            outbox.join(format!("{alice_request_id}.request.json")),
            outbox.join(".tmp-alice-invite"),
        )
        .unwrap();
        std::fs::copy(
            outbox.join(format!("{bob_request_id}.request.json")),
            outbox.join(".tmp-bob-invite"),
        )
        .unwrap();
        let partial = std::fs::read_to_string(outbox.join(".tmp-alice-invite")).unwrap();
        std::fs::write(
            outbox.join(".tmp-alice-partial"),
            partial.strip_suffix('}').unwrap(),
        )
        .unwrap();
        let sent_invitation = access
            .invitation_view("i-sent-by-alice")
            .unwrap()
            .invitation;
        let sent_token = access
            .issue_invitation_link(&sent_invitation)
            .unwrap()
            .unwrap();
        assert!(access.inspect_invite(&sent_token).is_some());
        let alice = crate::server::mail_texts::Sender {
            name: "Alice",
            email: "alice@example.test",
        };
        let bob = crate::server::mail_texts::Sender {
            name: "Bob",
            email: "malice@example.test",
        };
        let invite = |sender: &crate::server::mail_texts::Sender<'_>| {
            crate::server::mail_texts::invite(
                sender,
                "A",
                crate::access::OrgRole::Reader,
                "https://app.test/link/test",
            )
        };
        let shared = crate::server::mail_texts::share(
            &alice,
            &crate::server::mail_texts::Shared {
                organization: "A",
                title: "Report",
                description: "",
                editor: false,
            },
            "https://app.test/link/test",
        );
        std::fs::write(
            outbox.join("sent-by-alice.txt"),
            format!(
                "To: friend@example.test\nSubject: Invitation\n\n{}",
                invite(&alice)
            ),
        )
        .unwrap();
        std::fs::write(
            outbox.join("shared-by-alice.txt"),
            format!("To: friend@example.test\nSubject: Report\n\n{shared}"),
        )
        .unwrap();
        std::fs::write(
            outbox.join("sent-by-bob.txt"),
            format!(
                "To: friend@example.test\nSubject: Invitation\n\n{}",
                invite(&bob)
            ),
        )
        .unwrap();
        for email in ["alice@example.test", "malice@example.test"] {
            access
                .issue_link(email, LinkPurpose::Reset)
                .unwrap()
                .unwrap();
            queue
                .enqueue_manual(email.into(), ManualKind::Reset, true)
                .await
                .unwrap();
            access
                .audit(&Principal::Anonymous, "sign-in", email, false)
                .unwrap();
        }
        access
            .audit(
                &Principal::Anonymous,
                "sign-in",
                " ALICE@example.test ",
                false,
            )
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while queue.pending_manual().unwrap().len() != 4 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let storage = FsStorage::new(root.path()).unwrap().with_hosted(true);
        for (org, marker) in [
            ("aaaa0001", "alice-private-report"),
            ("bbbb0002", "bob-private-report"),
        ] {
            storage
                .put_version(StoredReport {
                    slug: format!("{org}/report"),
                    version: "1.0.0".into(),
                    source: marker.into(),
                    report_folder: None,
                    html: marker.into(),
                    frontmatter: json!({}),
                    created_at: "now".into(),
                    styles: vec![],
                    data: BTreeMap::from([("data.csv".into(), marker.into())]),
                    files: BTreeMap::from([("data.bin".into(), marker.into())]),
                    content_sha256: String::new(),
                })
                .unwrap();
        }
        let state = AppState {
            storage: storage.clone(),
            access: access.clone(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let secret = concat!("operator-fixture-", "0123456789abcdef");
        let app = router(secret, state.clone()).unwrap();
        let request = |person: &str, authorized: bool| {
            let mut request = Request::builder()
                .method("DELETE")
                .uri(format!("/operator/people/{person}"));
            if authorized {
                request = request.header(header::AUTHORIZATION, format!("Bearer {secret}"));
            }
            request.body(Body::empty()).unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(request("person-c", true))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        assert!(access.user("person-c").is_some());
        assert!(access.user("person-d").is_some());
        // A damaged backup makes deletion fail closed after disabling the account. Restoring
        // the backup and retrying must finish without touching the other organization.
        std::fs::write(access_dir.join("state.v1.json"), b"invalid").unwrap();
        assert_eq!(
            app.clone()
                .oneshot(request("person-a", true))
                .await
                .unwrap()
                .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert!(matches!(
            access.authenticate(Some("token-a"), None),
            Principal::Anonymous
        ));
        std::fs::write(access_dir.join("state.v1.json"), &backup_state).unwrap();
        assert_eq!(
            app.clone()
                .oneshot(request("person-a", true))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(request("person-b", false))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.clone()
                .oneshot(request("missing", true))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        assert!(!root.path().join("orgs/aaaa0001").exists());
        assert!(
            storage
                .get("bbbb0002/report", Some("1.0.0"))
                .unwrap()
                .is_some()
        );
        assert!(matches!(
            access.authenticate(Some("token-a"), None),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.authenticate(None, Some("session-a")),
            Principal::Anonymous
        ));
        assert!(matches!(
            access.authenticate(Some("token-b"), None),
            Principal::User(_)
        ));
        assert!(matches!(
            access.authenticate(None, Some("session-b")),
            Principal::User(_)
        ));
        let links = std::fs::read_to_string(access_dir.join("links.json")).unwrap();
        assert!(!links.contains("\"alice@example.test\""));
        assert!(links.contains("malice@example.test"));
        let pending = queue.pending_manual().unwrap();
        assert_eq!(pending.len(), 2);
        assert!(
            pending
                .iter()
                .any(|request| request.to == "malice@example.test")
        );
        assert!(
            pending
                .iter()
                .any(|request| request.invitation.as_deref() == Some("i-sent-by-bob"))
        );
        assert!(!outbox.join("sent-by-alice.txt").exists());
        assert!(!outbox.join("shared-by-alice.txt").exists());
        assert!(
            !outbox
                .join(format!("{alice_request_id}.request.json"))
                .exists()
        );
        assert!(!outbox.join(".tmp-alice-invite").exists());
        assert!(!outbox.join(".tmp-alice-partial").exists());
        assert!(outbox.join(".tmp-bob-invite").exists());
        assert!(outbox.join("sent-by-bob.txt").exists());
        assert!(access.inspect_invite(&sent_token).is_none());
        let persisted = std::fs::read_to_string(access_dir.join("state.json")).unwrap();
        assert!(!persisted.contains("\"alice@example.test\""));
        assert!(!persisted.contains("person-a"));
        assert!(!persisted.contains("aaaa0001"));
        assert!(persisted.contains("malice@example.test"));
        let backup = std::fs::read_to_string(access_dir.join("state.v1.json")).unwrap();
        assert!(!backup.contains("\"alice@example.test\""));
        assert!(backup.contains("malice@example.test"));
        assert!(!access_dir.join("state.v1.partial-test.json").exists());
        assert!(!outbox.join("legacy.txt").exists());
        let refused = std::fs::read_to_string(access_dir.join("audit-refused.log")).unwrap();
        let targets: Vec<_> = refused
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["target"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert!(
            !targets
                .iter()
                .any(|target| target.trim().eq_ignore_ascii_case("alice@example.test"))
        );
        assert!(targets.iter().any(|target| target == "[deleted]"));
        assert!(targets.iter().any(|target| target == "malice@example.test"));
        let audit = std::fs::read_to_string(access_dir.join("audit.log")).unwrap();
        assert_eq!(
            audit
                .lines()
                .filter(|line| line.contains("operator.delete-account"))
                .count(),
            2
        );
        assert!(!audit.contains("\"alice@example.test\""));
        let public = super::super::build_app(state);
        assert_eq!(
            public
                .oneshot(request("person-b", true))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn deletion_promotes_the_named_remaining_member_and_keeps_shared_reports() {
        use crate::{
            access::{Access, OrgRole},
            mail::Outbox,
            storage::{FsStorage, StoredReport},
        };
        use axum::{body::Body, http::Request};
        use serde_json::json;
        use std::collections::BTreeMap;
        use tower::ServiceExt;

        let root = tempfile::tempdir().unwrap();
        let access_dir = root.path().join(".access");
        std::fs::create_dir_all(&access_dir).unwrap();
        let original = json!({"schema":2,"tenancy":"hosted",
            "organizations":[{"id":"aaaa0001","name":"Shared","created":"now","limits":{}},{"id":"bbbb0002","name":"Other shared","created":"now","limits":{}}],
            "memberships":[{"person":"person-a","org":"aaaa0001","role":"owner","created":"now"},{"person":"person-b","org":"aaaa0001","role":"author","created":"now"},{"person":"person-a","org":"bbbb0002","role":"owner","created":"now"},{"person":"person-c","org":"bbbb0002","role":"reader","created":"now"}],
            "users":[{"id":"person-a","name":"A","email":"a@example.test","created":"now","verified":"now"},{"id":"person-b","name":"B","email":"b@example.test","created":"now","verified":"now"},{"id":"person-c","name":"C","email":"c@example.test","created":"now","verified":"now","disabled":true},{"id":"outsider","name":"D","email":"d@example.test","created":"now","verified":"now"}],
            "sessions":[],"acls":{"aaaa0001/report":{"org":"aaaa0001","owner":"person-a","visibility":"org","grants":[]}}});
        std::fs::write(
            access_dir.join("state.json"),
            serde_json::to_vec(&original).unwrap(),
        )
        .unwrap();
        let access = Access::open_hosted(root.path()).unwrap();
        access
            .configure_mail(
                Arc::new(Outbox::open(&root.path().join("outbox")).unwrap()),
                20,
                true,
            )
            .unwrap();
        let storage = FsStorage::new(root.path()).unwrap().with_hosted(true);
        storage
            .put_version(StoredReport {
                slug: "aaaa0001/report".into(),
                version: "1.0.0".into(),
                source: "source".into(),
                report_folder: None,
                html: "report".into(),
                frontmatter: json!({}),
                created_at: "now".into(),
                styles: vec![],
                data: BTreeMap::new(),
                files: BTreeMap::new(),
                content_sha256: String::new(),
            })
            .unwrap();
        let state = AppState {
            storage: storage.clone(),
            access: access.clone(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let secret = concat!("operator-fixture-", "0123456789abcdef");
        let app = router(secret, state).unwrap();
        let missing_content_type = Request::builder()
            .method("DELETE")
            .uri("/operator/people/person-a")
            .header(header::AUTHORIZATION, format!("Bearer {secret}"))
            .body(Body::from(r#"{"successors":{"aaaa0001":"person-b"}}"#))
            .unwrap();
        assert_eq!(
            app.clone()
                .oneshot(missing_content_type)
                .await
                .unwrap()
                .status(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        let missing = Request::builder()
            .method("DELETE")
            .uri("/operator/people/person-a")
            .header(header::AUTHORIZATION, format!("Bearer {secret}"))
            .body(Body::empty())
            .unwrap();
        let refused = app.clone().oneshot(missing).await.unwrap();
        assert_eq!(refused.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(refused.into_body(), 4096)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body["error"],
            "Account is the last owner of an organization with other members; name an eligible successor for each organization"
        );
        let audit = std::fs::read_to_string(access_dir.join("audit-refused.log")).unwrap();
        let attempts: Vec<serde_json::Value> = audit
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|row: &serde_json::Value| row["action"] == "operator.delete-account")
            .collect();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0]["target"], "person-a");
        assert_eq!(attempts[0]["allowed"], false);
        let request = |successor: &str| {
            Request::builder()
                .method("DELETE")
                .uri("/operator/people/person-a")
                .header(header::AUTHORIZATION, format!("Bearer {secret}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"successors":{"aaaa0001":successor,"bbbb0002":"person-c"}}).to_string(),
                ))
                .unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(request("outsider"))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        assert!(access.user("person-a").is_some());
        assert_eq!(
            access.role_in("person-b", "aaaa0001"),
            Some(OrgRole::Author)
        );
        assert_eq!(
            access.role_in("person-c", "bbbb0002"),
            Some(OrgRole::Reader)
        );
        assert_eq!(
            app.oneshot(request("person-b")).await.unwrap().status(),
            StatusCode::OK
        );
        assert!(access.user("person-a").is_none());
        assert_eq!(access.role_in("person-b", "aaaa0001"), Some(OrgRole::Owner));
        assert_eq!(access.role_in("person-c", "bbbb0002"), Some(OrgRole::Owner));
        assert!(access.user("person-c").unwrap().disabled);
        assert_eq!(access.acl("aaaa0001/report").owner, "person-b");
        assert!(
            storage
                .get("aaaa0001/report", Some("1.0.0"))
                .unwrap()
                .is_some()
        );
        drop(access);
        let reopened = Access::reopen_hosted(root.path()).await;
        assert!(reopened.user("person-a").is_none());
        assert_eq!(
            reopened.role_in("person-b", "aaaa0001"),
            Some(OrgRole::Owner)
        );
        assert_eq!(
            reopened.role_in("person-c", "bbbb0002"),
            Some(OrgRole::Owner)
        );
        assert!(reopened.user("person-c").unwrap().disabled);
        assert_eq!(reopened.acl("aaaa0001/report").owner, "person-b");
    }

    #[tokio::test]
    async fn operator_routes_require_bearer_and_audit_the_operator() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let temp = std::env::current_dir().unwrap().join("tmp");
        std::fs::create_dir_all(&temp).unwrap();
        let dir = tempfile::Builder::new()
            .prefix("operator-test-")
            .tempdir_in(temp)
            .unwrap();
        let access = crate::access::Access::open_hosted(dir.path()).unwrap();
        let storage = crate::storage::FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true);
        let state = AppState {
            storage,
            access: access.clone(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let secret = concat!("abcdefghijklmno", "pqrstuvwxyz123456");
        let app = router(secret, state.clone()).unwrap();
        let request = || {
            Request::builder()
                .uri("/operator/organizations")
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        assert!(access.read_audit(10).is_empty());
        let authorized = Request::builder()
            .uri("/operator/organizations")
            .header(header::AUTHORIZATION, format!("Bearer {secret}"))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(authorized).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let audit = access.read_audit(10);
        assert!(
            audit.iter().any(|line| line["actor"] == "operator"
                && line["action"] == "operator.complete.list-orgs")
        );
        let public = super::super::build_app(state);
        assert_eq!(
            public.oneshot(request()).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn summary_counts_waiting_mail_without_addresses() {
        use axum::{
            body::{Body, to_bytes},
            http::Request,
        };
        use tower::ServiceExt;
        let dir = tempfile::tempdir().unwrap();
        let access = crate::access::Access::open_hosted(dir.path()).unwrap();
        access
            .configure_mail(
                Arc::new(crate::mail::Outbox::open(&dir.path().join("outbox")).unwrap()),
                10,
                true,
            )
            .unwrap();
        let queue = access.mail_queue().unwrap();
        queue
            .enqueue_manual(
                "private@example.test".into(),
                crate::mail::ManualKind::Signup,
                true,
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while queue.pending_manual().unwrap().is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let state = AppState {
            storage: crate::storage::FsStorage::new(dir.path())
                .unwrap()
                .with_hosted(true),
            access,
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let secret = concat!("abcdefghijklmno", "pqrstuvwxyz123456");
        let response = router(secret, state)
            .unwrap()
            .oneshot(
                Request::builder()
                    .uri("/operator/summary")
                    .header(header::AUTHORIZATION, format!("Bearer {secret}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 100_000).await.unwrap();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(!text.contains("private@example.test"));
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["mailsWaiting"], 1);
        assert!(body["oldestMailAgeSeconds"].as_u64().is_some());
        assert_eq!(body["mailsSentSinceStart"], 0);
        assert_eq!(body["mailsSentTotal"], 0);
    }

    #[tokio::test]
    async fn manual_dispatch_requires_operator_bearer_and_returns_no_store_mail() {
        use axum::{
            body::{Body, to_bytes},
            http::Request,
        };
        use tower::ServiceExt;
        let dir = tempfile::tempdir().unwrap();
        let access = crate::access::Access::open_hosted(dir.path()).unwrap();
        access
            .configure_mail(
                Arc::new(crate::mail::Outbox::open(&dir.path().join("outbox")).unwrap()),
                10,
                true,
            )
            .unwrap();
        let queue = access.mail_queue().unwrap();
        queue
            .enqueue_manual(
                "manual@example.test".into(),
                crate::mail::ManualKind::Signup,
                true,
            )
            .await
            .unwrap();
        let pending = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let pending = queue.pending_manual().unwrap();
                if let Some(request) = pending.into_iter().next() {
                    break request;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let state = AppState {
            storage: crate::storage::FsStorage::new(dir.path())
                .unwrap()
                .with_hosted(true),
            access,
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let secret = concat!("abcdefghijklmno", "pqrstuvwxyz123456");
        let app = router(secret, state).unwrap();
        let path = format!("/operator/mail/{}/dispatch", pending.id);
        let request = |token: Option<&str>| {
            let mut request = Request::builder().method("POST").uri(&path);
            if let Some(token) = token {
                request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }
            request.body(Body::empty()).unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request(None)).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let response = app.oneshot(request(Some(secret))).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = to_bytes(response.into_body(), 100_000).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["to"], "manual@example.test");
        assert!(
            body["body"]
                .as_str()
                .unwrap()
                .contains("https://app.test/link/")
        );
    }

    #[test]
    fn operator_token_must_match_the_whole_value() {
        let expected = Sha256::digest(b"12345678901234567890123456789012").into();
        assert!(token_matches(&expected, "12345678901234567890123456789012"));
        assert!(!token_matches(&expected, "1234567890123456789012345678901"));
        assert!(!token_matches(
            &expected,
            "12345678901234567890123456789013"
        ));
    }
}
