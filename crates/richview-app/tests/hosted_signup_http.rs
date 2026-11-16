//! Local hosted signup/reset proof; the transport records mail in memory and never sends it.
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use richview_app::{
    access::{Access, LinkPurpose, Principal},
    mail::{Mail, Transport},
    server::{AppState, build_app},
    storage::FsStorage,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

#[derive(Default)]
struct FakeMail(Mutex<Vec<Mail>>);
impl Transport for FakeMail {
    fn write_manual(&self, request: &richview_app::mail::ManualRequest) -> Result<(), String> {
        if request.status != richview_app::mail::ManualStatus::Decoy {
            self.0.lock().unwrap().push(Mail {
                to: request.to.clone(),
                subject: String::new(),
                body: String::new(),
                html: None,
            });
        }
        Ok(())
    }
    fn send<'a>(
        &'a self,
        mail: &'a Mail,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
        Box::pin(async move {
            self.0.lock().unwrap().push(mail.clone());
            Ok(())
        })
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    app: axum::Router,
    state: AppState,
    access: Access,
    mail: Arc<FakeMail>,
}
impl Fixture {
    async fn new(manual: bool) -> Self {
        Self::with_budget(manual, 1000).await
    }
    async fn with_budget(manual: bool, budget: u64) -> Self {
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
        let mail = Arc::new(FakeMail::default());
        access.configure_mail(mail.clone(), budget, manual).unwrap();
        let storage = FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true)
            .with_quota(Some(100_000_000));
        let state = AppState {
            storage,
            access: access.clone(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let app = build_app(state.clone());
        Self {
            _dir: dir,
            app,
            state,
            access,
            mail,
        }
    }
    async fn request(
        &self,
        method: &str,
        path: &str,
        body: &str,
        cookie: Option<&str>,
    ) -> (StatusCode, axum::http::HeaderMap, String) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "app.test")
            .header("x-real-ip", "192.0.2.1")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(Body::from(body.to_owned())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = String::from_utf8(
            to_bytes(response.into_body(), 2_000_000)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        (status, headers, body)
    }
    async fn wait_for_mail(&self, count: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while self.mail.0.lock().unwrap().len() < count {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn a_hosted_page_carries_the_site_mark_as_its_icon() {
    // The hosted origin serves no asset files, so the icon rides in the page and the policy
    // lets a data image load.
    let fixture = Fixture::new(false).await;
    let (status, headers, page) = fixture.request("GET", "/sign-up", "", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains(
            "<link rel=\"icon\" type=\"image/svg+xml\" href=\"data:image/svg+xml,%3Csvg "
        )
    );
    let icon = page.split("href=\"data:image/svg+xml,").nth(1).unwrap();
    let icon = &icon[..icon.find('"').unwrap()];
    assert!(
        icon.ends_with("%3C/svg%3E") && !icon.contains(['<', '>', '#']),
        "{icon}"
    );
    let policy = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(policy.contains("img-src data:;"), "{policy}");
}

#[tokio::test]
async fn account_limit_and_operator_pause_stop_signup_but_not_reset_requests() {
    use richview_app::server::hosted::{OperatorAction, operator_action};
    let fixture = Fixture::new(false).await;
    fixture.access.set_signup_account_limit(1).unwrap();
    assert_eq!(
        fixture.request("GET", "/sign-up", "", None).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        fixture
            .request("POST", "/ui/sign-up", "email=new%40example.test", None)
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        fixture
            .request("POST", "/ui/forgot", "email=known%40example.test", None)
            .await
            .0,
        StatusCode::OK
    );
    fixture.access.set_signup_account_limit(2).unwrap();
    operator_action(
        &fixture.state,
        OperatorAction::SetSignupsPaused { paused: true },
    )
    .await
    .unwrap();
    assert_eq!(
        fixture.request("GET", "/sign-up", "", None).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    operator_action(
        &fixture.state,
        OperatorAction::SetSignupsPaused { paused: false },
    )
    .await
    .unwrap();
    assert_eq!(
        fixture.request("GET", "/sign-up", "", None).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn mail_budget_pauses_signups_without_saying_a_reset_was_sent() {
    let fixture = Fixture::with_budget(false, 2).await;
    assert_eq!(
        fixture
            .request("POST", "/ui/sign-up", "email=new%40example.test", None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        fixture.request("GET", "/sign-up", "", None).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        fixture
            .request("POST", "/ui/forgot", "email=known%40example.test", None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        fixture
            .request("POST", "/ui/forgot", "email=known%40example.test", None)
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn cross_origin_link_requests_are_refused_before_mail_is_queued() {
    let fixture = Fixture::new(false).await;
    for path in ["/ui/sign-up", "/ui/forgot"] {
        let response = fixture
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("host", "app.test")
                    .header(header::ORIGIN, "https://attacker.test")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("email=known%40example.test"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    assert!(fixture.mail.0.lock().unwrap().is_empty());
}
fn link_path(body: &str) -> String {
    let link = body
        .lines()
        .find(|line| line.starts_with("https://app.test/link/"))
        .unwrap();
    reqwest::Url::parse(link).unwrap().path().to_owned()
}

#[tokio::test]
async fn signup_and_forgot_responses_do_not_reveal_accounts_in_either_delivery_mode() {
    for manual in [true, false] {
        let fixture = Fixture::new(manual).await;
        let known = fixture
            .request("POST", "/ui/sign-up", "email=known%40example.test", None)
            .await;
        let unknown = fixture
            .request("POST", "/ui/sign-up", "email=new%40example.test", None)
            .await;
        assert_eq!(known.0, StatusCode::OK);
        assert_eq!(known.2, unknown.2);
        assert!(known.2.contains(if manual {
            "a few hours"
        } else {
            "a few minutes"
        }));
        let forgot_known = fixture
            .request("POST", "/ui/forgot", "email=known%40example.test", None)
            .await;
        let forgot_unknown = fixture
            .request("POST", "/ui/forgot", "email=absent%40example.test", None)
            .await;
        assert_eq!(forgot_known.0, StatusCode::OK);
        assert_eq!(forgot_known.2, forgot_unknown.2);
        fixture.wait_for_mail(3).await;
        let messages = fixture.mail.0.lock().unwrap();
        assert_eq!(messages.len(), 3, "unknown reset must not send mail");
        assert!(messages.iter().all(|mail| mail.to != "absent@example.test"));
    }
}

#[tokio::test]
async fn anonymous_link_history_cannot_crowd_out_an_existing_account_reset() {
    let dir = tempfile::tempdir().unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    let signup = access
        .issue_link("known@example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    access
        .complete_hosted_link(&signup, "Known", "known password long")
        .await
        .unwrap();
    drop(access);

    let path = dir.path().join(".access/links.json");
    let mut history: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    while history.len() < 5_000 {
        let i = history.len();
        history.push(serde_json::json!({
            "sha256": format!("{i:064x}"), "purpose":"reset",
            "email":format!("unknown{i}@example.test"), "created":at,
            "expires":at + 3600, "used":false
        }));
    }
    std::fs::write(&path, serde_json::to_vec(&history).unwrap()).unwrap();

    let access = Access::open_hosted(dir.path()).unwrap();
    let mail = Arc::new(FakeMail::default());
    access.configure_mail(mail.clone(), 1_000, false).unwrap();
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
    let fixture = Fixture {
        _dir: dir,
        app: build_app(state.clone()),
        state,
        access,
        mail,
    };
    let unknown = fixture
        .request("POST", "/ui/forgot", "email=another%40example.test", None)
        .await;
    let unchanged: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(unchanged.len(), 5_000);
    let known = fixture
        .request("POST", "/ui/forgot", "email=known%40example.test", None)
        .await;
    assert_eq!(known.0, StatusCode::OK);
    assert_eq!(known.2, unknown.2);
    fixture.wait_for_mail(1).await;
    let post =
        link_path(&fixture.mail.0.lock().unwrap()[0].body).replacen("/link/", "/ui/link/", 1);
    assert_eq!(
        fixture
            .request(
                "POST",
                &post,
                "password=a-new-password&password_confirm=a-new-password",
                None,
            )
            .await
            .0,
        StatusCode::SEE_OTHER
    );
    assert!(
        fixture
            .access
            .sign_in("known@example.test", "a-new-password")
            .is_ok()
    );
}

#[tokio::test]
async fn signup_link_is_inspected_without_consumption_and_creates_a_session() {
    let fixture = Fixture::new(false).await;
    fixture
        .request("POST", "/ui/sign-up", "email=new%40example.test", None)
        .await;
    fixture.wait_for_mail(1).await;
    let path = link_path(&fixture.mail.0.lock().unwrap()[0].body);
    for _ in 0..2 {
        let (status, headers, _) = fixture.request("GET", &path, "", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    }
    let post = path.replacen("/link/", "/ui/link/", 1);
    let (status, headers, _) = fixture
        .request(
            "POST",
            &post,
            "name=New&password=a-long-password&password_confirm=a-long-password",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
    let cookie = headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let (status, _, page) = fixture.request("GET", "/", "", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("<h1>Reports</h1>"));
    assert_eq!(
        fixture.request("GET", &path, "", None).await.0,
        StatusCode::GONE
    );
    assert_eq!(fixture.access.users().len(), 2);
}

#[tokio::test]
async fn pausing_after_a_signup_link_is_issued_preserves_it_until_signups_reopen() {
    use richview_app::server::hosted::{OperatorAction, operator_action};
    let fixture = Fixture::new(false).await;
    fixture
        .request("POST", "/ui/sign-up", "email=new%40example.test", None)
        .await;
    fixture.wait_for_mail(1).await;
    let path = link_path(&fixture.mail.0.lock().unwrap()[0].body);
    let post = path.replacen("/link/", "/ui/link/", 1);
    let form = "name=New&password=a-long-password&password_confirm=a-long-password";
    operator_action(
        &fixture.state,
        OperatorAction::SetSignupsPaused { paused: true },
    )
    .await
    .unwrap();
    assert_eq!(
        fixture.request("POST", &post, form, None).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        fixture.request("GET", &path, "", None).await.0,
        StatusCode::OK
    );
    operator_action(
        &fixture.state,
        OperatorAction::SetSignupsPaused { paused: false },
    )
    .await
    .unwrap();
    assert_eq!(
        fixture.request("POST", &post, form, None).await.0,
        StatusCode::SEE_OTHER
    );
}

#[tokio::test]
async fn reset_link_revokes_old_credentials_and_signs_in_with_a_new_session() {
    let fixture = Fixture::new(false).await;
    let known = fixture
        .access
        .users()
        .into_iter()
        .find(|u| u.email == "known@example.test")
        .unwrap();
    let org = fixture.access.organizations_for(&known.id)[0].id.clone();
    let (_, old_token) = fixture
        .access
        .create_self_token_for_fixture(&known.id, &org, "old")
        .unwrap();
    let (old_session, _) = fixture
        .access
        .sign_in(&known.email, "known password long")
        .unwrap();
    fixture
        .request("POST", "/ui/forgot", "email=known%40example.test", None)
        .await;
    fixture.wait_for_mail(1).await;
    let path =
        link_path(&fixture.mail.0.lock().unwrap()[0].body).replacen("/link/", "/ui/link/", 1);
    let (status, headers, _) = fixture
        .request(
            "POST",
            &path,
            "password=a-new-password&password_confirm=a-new-password",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(matches!(
        fixture.access.authenticate(Some(&old_token), None),
        Principal::Anonymous
    ));
    assert!(matches!(
        fixture.access.authenticate(None, Some(&old_session)),
        Principal::Anonymous
    ));
    let cookie = headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    assert_eq!(
        fixture.request("GET", "/", "", Some(cookie)).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn publish_attempts_are_limited_before_repeated_render_work() {
    let fixture = Fixture::new(false).await;
    let user = fixture
        .access
        .users()
        .into_iter()
        .find(|u| u.email == "known@example.test")
        .unwrap();
    let org = fixture.access.organizations_for(&user.id)[0].id.clone();
    let (_, token) = fixture
        .access
        .create_self_token_for_fixture(&user.id, &org, "rate test")
        .unwrap();
    let body = serde_json::json!({"slug":"test","new":true,"source":"not a publishable report"})
        .to_string();
    for i in 0..61 {
        let response = fixture
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/reports")
                    .header("host", "app.test")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        if i < 60 {
            assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        } else {
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        }
    }
}

#[tokio::test]
async fn completing_the_last_mailed_signup_link_does_not_require_another_mail_slot() {
    let fixture = Fixture::with_budget(false, 2).await;
    assert_eq!(
        fixture
            .request("POST", "/ui/sign-up", "email=last%40example.test", None)
            .await
            .0,
        StatusCode::OK
    );
    fixture.wait_for_mail(1).await;
    let path = link_path(&fixture.mail.0.lock().unwrap()[0].body);
    let post = path.replacen("/link/", "/ui/link/", 1);
    assert_eq!(
        fixture
            .request(
                "POST",
                &post,
                "name=Last&password=a-long-password&password_confirm=a-long-password",
                None
            )
            .await
            .0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(fixture.access.users().len(), 2);
}

#[tokio::test]
async fn journal_write_failure_returns_the_same_generic_503_for_real_and_decoy_resets() {
    struct Durable;
    impl Transport for Durable {
        fn durable(&self) -> bool {
            true
        }
        fn send<'a>(
            &'a self,
            _: &'a Mail,
        ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let access = Access::open_hosted(dir.path()).unwrap();
    let token = access
        .issue_link("known@example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    access
        .complete_hosted_link(&token, "Known", "known password long")
        .await
        .unwrap();
    access
        .configure_mail(Arc::new(Durable), 100, false)
        .unwrap();
    let journal = dir.path().join(".access/smtp-outbox");
    std::fs::remove_dir(&journal).unwrap();
    std::fs::write(&journal, "not a directory").unwrap();
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
    let app = build_app(state);
    let mut replies = Vec::new();
    for address in ["known%40example.test", "unknown%40example.test"] {
        let request = Request::builder()
            .method("POST")
            .uri("/ui/forgot")
            .header("host", "app.test")
            .header("x-real-ip", "192.0.2.1")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("email={address}")))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), 2_000_000).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(!text.contains("Check your mail"));
        replies.push(text);
    }
    assert_eq!(replies[0], replies[1]);
}

#[tokio::test]
async fn rejected_signup_passwords_keep_the_link_and_the_escaped_name() {
    let fixture = Fixture::new(false).await;
    let token = fixture
        .access
        .issue_link("retry@example.test", LinkPurpose::Signup)
        .unwrap()
        .unwrap();
    let path = format!("/ui/link/{token}");
    let (status, _, page) = fixture
        .request(
            "POST",
            &path,
            "name=A%20%3CB%3E&password=long-password-one&password_confirm=long-password-two",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(page.contains("The two passwords did not match. Enter them again."));
    assert!(page.contains("value=\"A &lt;B&gt;\""));
    assert_eq!(
        fixture.access.inspect_link(&token),
        Some(LinkPurpose::Signup)
    );
    let (status, _, page) = fixture
        .request(
            "POST",
            &path,
            "name=A%20%3CB%3E&password=short&password_confirm=short",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(page.contains("note--error"));
    assert!(page.contains("value=\"A &lt;B&gt;\""));
    assert_eq!(
        fixture.access.inspect_link(&token),
        Some(LinkPurpose::Signup)
    );
    let (status, _, _) = fixture
        .request(
            "POST",
            &path,
            "name=A%20B&password=long-password-one&password_confirm=long-password-one",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(fixture.access.inspect_link(&token).is_none());
}

#[tokio::test]
async fn rejected_reset_password_keeps_the_reset_link_usable() {
    let fixture = Fixture::new(false).await;
    let token = fixture
        .access
        .issue_link("known@example.test", LinkPurpose::Reset)
        .unwrap()
        .unwrap();
    let path = format!("/ui/link/{token}");
    let (status, _, page) = fixture
        .request(
            "POST",
            &path,
            "password=long-password-one&password_confirm=long-password-two",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(page.contains("The two passwords did not match. Enter them again."));
    assert!(page.contains("Choose a new password"));
    assert_eq!(
        fixture.access.inspect_link(&token),
        Some(LinkPurpose::Reset)
    );
    let (status, _, _) = fixture
        .request(
            "POST",
            &path,
            "password=long-password-one&password_confirm=long-password-one",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(fixture.access.inspect_link(&token).is_none());
}
