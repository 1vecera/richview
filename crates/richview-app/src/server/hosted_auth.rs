//! Hosted email-link forms. Delivery is queued; responses do not disclose account existence.
use super::{AppState, auth, mail_texts, ui};
use crate::{
    access::{AccessError, LinkPurpose},
    mail::{Mail, ManualKind},
};
use axum::{
    Form,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct EmailForm {
    email: String,
}
#[derive(Deserialize)]
pub struct LinkForm {
    #[serde(default)]
    name: String,
    // An invitation accepted by an existing account sends no password.
    #[serde(default)]
    password: String,
    #[serde(default)]
    password_confirm: String,
}

static CHECKS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::OnceLock::new();

async fn signup_open(state: &AppState) -> bool {
    state
        .access
        .mail_queue()
        .is_some_and(|queue| !queue.signup_exhausted())
        && signup_completion_open(state).await
}

// Already delivered links need capacity, but do not spend another mail slot.
async fn signup_completion_open(state: &AppState) -> bool {
    if !state.access.signup_account_capacity() {
        return false;
    }
    // Brief contention is not a capacity failure. Keep scans bounded and stop waiting if busy.
    let Ok(Ok(permit)) = tokio::time::timeout(
        std::time::Duration::from_millis(250),
        CHECKS
            .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(2)))
            .clone()
            .acquire_owned(),
    )
    .await
    else {
        return false;
    };
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        storage.signup_space_available()
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or(false)
}

pub(super) fn client_ip(headers: &HeaderMap) -> Option<std::net::IpAddr> {
    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}
pub(super) fn manual(state: &AppState) -> bool {
    state.access.manual_outbox().unwrap_or(true)
}

pub async fn signup_page(State(state): State<AppState>) -> Response {
    if signup_open(&state).await {
        ui::sign_up_page().await
    } else {
        ui::signups_paused_page()
    }
}

pub async fn signup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(permit): Extension<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>,
    Form(form): Form<EmailForm>,
) -> Response {
    if !auth::same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !signup_open(&state).await {
        return ui::signups_paused_page();
    }
    if request_link(&state, &form.email, true, permit)
        .await
        .is_err()
    {
        return ui::hosted_error_page(StatusCode::SERVICE_UNAVAILABLE);
    }
    ui::check_mail_page(&state)
}

pub async fn forgot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(permit): Extension<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>,
    Form(form): Form<EmailForm>,
) -> Response {
    if !auth::same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if state
        .access
        .mail_queue()
        .is_none_or(|queue| queue.exhausted())
    {
        return ui::hosted_error_page(StatusCode::SERVICE_UNAVAILABLE);
    }
    if request_link(&state, &form.email, false, permit)
        .await
        .is_err()
    {
        return ui::hosted_error_page(StatusCode::SERVICE_UNAVAILABLE);
    }
    ui::check_mail_page(&state)
}

async fn request_link(
    state: &AppState,
    email: &str,
    signup: bool,
    permit: std::sync::Arc<tokio::sync::OwnedSemaphorePermit>,
) -> Result<(), ()> {
    // Keep the privacy lock until any admitted link or queued mail is committed, even if the
    // browser disconnects; deletion waits for the same lock before flushing the queue.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(250);
    let privacy = state.access.mail_privacy.clone().lock_owned().await;
    let state = state.clone();
    let email = email.to_owned();
    let result = tokio::spawn(async move {
        let _privacy = privacy;
        request_link_inner(&state, &email, signup, permit).await
    })
    .await
    .map_err(|_| ());
    tokio::time::sleep_until(deadline).await;
    result?
}

async fn request_link_inner(
    state: &AppState,
    email: &str,
    signup: bool,
    permit: std::sync::Arc<tokio::sync::OwnedSemaphorePermit>,
) -> Result<(), ()> {
    let Some(queue) = state.access.mail_queue().cloned() else {
        return Err(());
    };
    let access = state.access.clone();
    let Some(email) = crate::email::normalize(email) else {
        return Ok(());
    };
    if manual(state) {
        let checked_email = email.clone();
        let checked = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            access.hosted_email_exists(&checked_email)
        })
        .await;
        let known = checked.map_err(|_| ())?;
        let kind = if signup && !known {
            ManualKind::Signup
        } else if signup {
            ManualKind::AlreadyRegistered
        } else {
            ManualKind::Reset
        };
        return queue
            .enqueue_manual(email, kind, signup || known)
            .await
            .map_err(|_| ());
    }
    let issued = tokio::task::spawn_blocking(move || {
        let known = access.hosted_email_exists(&email);
        let purpose = if signup && !known {
            LinkPurpose::Signup
        } else {
            LinkPurpose::Reset
        };
        let issued = if signup || !known {
            access.issue_anonymous_link(&email, purpose)
        } else {
            access.issue_link(&email, purpose)
        };
        (permit, issued.map(|token| (email, known, purpose, token)))
    })
    .await;
    let (_permit, issued) = issued.map_err(|_| ())?;
    let (email, known, purpose, token) = issued.map_err(|_| ())?;
    let Some(token) = token else { return Ok(()) };
    let origin = state.public_origin.as_deref().ok_or(())?;
    let base = reqwest::Url::parse(origin).map_err(|_| ())?;
    let link = base.join(&format!("/link/{token}")).map_err(|_| ())?;
    let link = link.as_str();
    let (subject, body, html) = if purpose == LinkPurpose::Signup {
        (
            "Finish creating your RichView account",
            mail_texts::signup(link),
            mail_texts::signup_html(link),
        )
    } else if signup {
        (
            "Your RichView account",
            mail_texts::already_registered(link),
            mail_texts::already_registered_html(link),
        )
    } else {
        (
            "Reset your RichView password",
            mail_texts::reset(link),
            mail_texts::reset_html(link),
        )
    };
    let mail = Mail {
        to: email,
        subject: subject.into(),
        body,
        html: Some(html),
    };
    // The forgot flow performs the same link, queue and durable budget work for either identity;
    // a nonexistent account never receives a message or a usable reset.
    if !signup && !known {
        queue.enqueue_decoy(mail, token).await.map_err(|_| ())
    } else if signup {
        queue.enqueue_signup(mail, token).await.map_err(|_| ())
    } else {
        queue.enqueue_link(mail, token).await.map_err(|_| ())
    }
}

const PASSWORD_MISMATCH: &str = "The two passwords did not match. Enter them again.";

fn safe_link_error(message: &str) -> &str {
    match message {
        "Check the name and password." => message,
        // The rule a too-short password broke, in the server's own words.
        _ if message == crate::access::short_password() => message,
        _ => "That password can't be used. Choose another of at least 12 characters.",
    }
}

pub async fn complete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
    Form(form): Form<LinkForm>,
) -> Response {
    if !auth::same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(purpose) = state.access.inspect_link(&token) else {
        return ui::expired_link_page();
    };
    if purpose == LinkPurpose::Invite {
        return accept_invitation(&state, &token, &form).await;
    }
    // Order signup's storage-threshold check and account creation with report writes.
    let publication = if purpose == LinkPurpose::Signup {
        Some(state.access.publication.clone().lock_owned().await)
    } else {
        None
    };
    if purpose == LinkPurpose::Signup && !signup_completion_open(&state).await {
        return ui::signups_paused_page();
    }
    if form.password != form.password_confirm {
        drop(publication);
        return ui::link_error_page(&state, &token, purpose, &form.name, PASSWORD_MISMATCH);
    }
    match state
        .access
        .complete_hosted_link_and_sign_in(&token, &form.name, &form.password, publication)
        .await
    {
        Ok((_user, cookie, _csrf)) => {
            let mut response = (StatusCode::SEE_OTHER, [(header::LOCATION, "/")]).into_response();
            if let Ok(cookie) = auth::session_cookie(&state, &cookie, 12 * 3600).parse() {
                response.headers_mut().insert(header::SET_COOKIE, cookie);
            }
            response
        }
        Err(AccessError::Throttled) if purpose == LinkPurpose::Signup => ui::signups_paused_page(),
        Err(AccessError::Invalid(message)) => ui::link_error_page(
            &state,
            &token,
            purpose,
            &form.name,
            safe_link_error(&message),
        ),
        Err(AccessError::Credentials | AccessError::NotFound) => ui::expired_link_page(),
        Err(_) => ui::hosted_error_page(StatusCode::SERVICE_UNAVAILABLE),
    }
}

/// Accept an invitation from its mail link. An address without an account creates one here and
/// is signed in; an existing account receives what was offered and signs in as usual.
async fn accept_invitation(state: &AppState, token: &str, form: &LinkForm) -> Response {
    use crate::access::Landing;
    let Some(view) = state.access.inspect_invite(token) else {
        return ui::expired_link_page();
    };
    // A new account is ordered with report writes and the storage threshold, like sign-up.
    let publication = if view.account_exists {
        None
    } else {
        let publication = state.access.publication.clone().lock_owned().await;
        if !signup_completion_open(state).await {
            return ui::signups_paused_page();
        }
        if form.password != form.password_confirm {
            drop(publication);
            return ui::invitation_error_page(state, token, &form.name, PASSWORD_MISMATCH).await;
        }
        Some(publication)
    };
    match state
        .access
        .accept_invitation_link(token, &form.name, &form.password, publication)
        .await
    {
        Ok(accepted) => {
            let _ = state.access.audit(
                &crate::access::Principal::User(accepted.user.clone()),
                "invitation.accept",
                &view.invitation.id,
                true,
            );
            let target = match (&accepted.landing, &accepted.session) {
                // The report lives on another origin, and a browser does not follow a form
                // there under `form-action 'self'`: land on the report's page here, which sends
                // the browser on to the report in a navigation of its own (`open`).
                (Landing::Report(key), Some(_)) => {
                    format!("/reports/{key}?m=This%20report%20was%20shared%20with%20you.&open=1")
                }
                (Landing::Report(key), None) => format!(
                    "/?m=Accepted.%20Sign%20in%20to%20open%20the%20report.&next={}",
                    ui::encode_value(&format!("/r/{key}"))
                ),
                (Landing::Organization(_), Some(_)) => "/".to_owned(),
                (Landing::Organization(_), None) => {
                    "/?m=Accepted.%20Sign%20in%20to%20continue.".to_owned()
                }
            };
            let mut response =
                (StatusCode::SEE_OTHER, [(header::LOCATION, target)]).into_response();
            if let Some((cookie, _)) = &accepted.session
                && let Ok(cookie) = auth::session_cookie(state, cookie, 12 * 3600).parse()
            {
                response.headers_mut().insert(header::SET_COOKIE, cookie);
            }
            response
        }
        Err(AccessError::Throttled) => ui::signups_paused_page(),
        Err(AccessError::Invalid(message)) => {
            ui::invitation_error_page(state, token, &form.name, safe_link_error(&message)).await
        }
        Err(AccessError::Credentials | AccessError::NotFound) => ui::expired_link_page(),
        Err(_) => ui::hosted_error_page(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{access::Access, mail::Outbox, server::build_app, storage::FsStorage};
    use axum::{body::Body, http::Request};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn first_signup_waits_for_briefly_busy_capacity_checks() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        access
            .configure_mail(
                Arc::new(Outbox::open(&dir.path().join("outbox")).unwrap()),
                2,
                true,
            )
            .unwrap();
        assert!(!access.mail_queue().unwrap().exhausted());
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
        let permits = CHECKS
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(2)))
            .clone()
            .acquire_many_owned(2)
            .await
            .unwrap();
        // A sustained blockage still times out without spending the fresh mail budget.
        assert!(!signup_completion_open(&state).await);
        assert!(!access.mail_queue().unwrap().exhausted());
        let app = build_app(state);
        let request = Request::builder()
            .method("POST")
            .uri("/ui/sign-up")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("email=first%40example.test"))
            .unwrap();
        let task = tokio::spawn(async move { app.oneshot(request).await.unwrap() });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!access.mail_queue().unwrap().exhausted());
        drop(permits);
        let response = tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(access.mail_queue().unwrap().signup_exhausted());
        assert!(!access.mail_queue().unwrap().exhausted());
    }
}
