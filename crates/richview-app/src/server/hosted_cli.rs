//! Signing the command line in from the browser (hosted). The command line starts a sign-in and
//! shows a short code; its person confirms the code on a page here; the command line collects a
//! CLI token once. Two routes answer anyone and are bounded before they do any work; the page
//! and its form act only for a signed-in browser session; and a token can revoke itself.
//!
//! No address here ever carries a secret: the page's address holds the short code, which is no
//! credential, and the command line's secret and the token travel only in request and response
//! bodies, which are never stored or logged.
use super::{
    AppState, Hosted, api_error, audit, auth, encode_value, form_caller, hosted_member,
    hosted_page, hosted_pages, redirect,
};
use crate::access::{
    AccessError, CLI_SIGN_IN_POLL_SECONDS, CLI_SIGN_IN_SECONDS, CliSignInCollected, Principal,
    cli_user_code,
};
use axum::{
    Form, Json,
    extract::{Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

const WAIT: &str = "Too many sign-ins from this address. Wait a few minutes, then try again.";

/// An answer that carries a secret, or the state of one: never kept by a cache.
fn unstored(status: StatusCode, body: serde_json::Value) -> Response {
    let mut response = (status, Json(body)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

#[derive(Deserialize, Default)]
pub struct CliSignInStart {
    /// What the command line calls its computer; shown to the person, never trusted.
    #[serde(default)]
    computer: String,
}

/// `POST /api/cli/sign-in`: a command line starts a sign-in. Anyone may; it is bounded by address.
pub async fn start_cli_sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<CliSignInStart>, JsonRejection>,
) -> Response {
    // A body that is too large or no JSON object is refused as what it is.
    let computer = match body {
        Ok(Json(start)) => start.computer,
        Err(refusal) => return api_error(refusal.status(), "Send a JSON object."),
    };
    match state
        .access
        .start_cli_sign_in(super::super::hosted_auth::client_ip(&headers), &computer)
    {
        Ok(started) => unstored(
            StatusCode::OK,
            json!({
                "code": started.code,
                "userCode": started.user_code,
                "expiresIn": CLI_SIGN_IN_SECONDS,
                "interval": CLI_SIGN_IN_POLL_SECONDS,
            }),
        ),
        Err(AccessError::Throttled) => api_error(StatusCode::TOO_MANY_REQUESTS, WAIT),
        Err(AccessError::NotFound) => auth::not_found(),
        Err(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The sign-in could not be started.",
        ),
    }
}

#[derive(Deserialize)]
pub struct CliSignInCollect {
    code: String,
}

/// `POST /api/cli/sign-in/token`: the command line asks whether its person has decided, and gets
/// its token once if they said yes.
pub async fn collect_cli_sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<CliSignInCollect>, JsonRejection>,
) -> Response {
    let body = match body {
        Ok(Json(body)) => body,
        Err(refusal) if refusal.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            return api_error(refusal.status(), "Send the sign-in's code.");
        }
        Err(_) => return api_error(StatusCode::BAD_REQUEST, "Send the sign-in's code."),
    };
    let collected = state
        .access
        .collect_cli_sign_in(super::super::hosted_auth::client_ip(&headers), &body.code);
    match collected {
        Ok(CliSignInCollected::Waiting) => unstored(StatusCode::OK, json!({"status": "waiting"})),
        Ok(CliSignInCollected::Allowed {
            token,
            token_id,
            person,
            org,
        }) => {
            let user = state.access.user(&person);
            if let Some(user) = &user {
                // The same audit line a token made on the CLI tokens page leaves.
                let _ = state.access.audit(
                    &Principal::User(user.clone()),
                    "token.create",
                    &format!("{person}/{token_id}"),
                    true,
                );
            }
            let organization = state
                .access
                .organizations_for(&person)
                .into_iter()
                .find(|o| o.id == org)
                .map(|o| o.name);
            unstored(
                StatusCode::OK,
                json!({
                    "status": "allowed",
                    "token": token,
                    "account": user.map(|user| json!({"name": user.name, "email": user.email})),
                    "organization": organization,
                }),
            )
        }
        Ok(CliSignInCollected::Denied) => unstored(
            StatusCode::FORBIDDEN,
            json!({"status": "denied", "error": "The sign-in was cancelled in the browser."}),
        ),
        Ok(CliSignInCollected::Gone) => unstored(
            StatusCode::GONE,
            json!({"status": "expired", "error": "This sign-in expired or was already used. Start it again."}),
        ),
        Ok(CliSignInCollected::Failed(message)) => unstored(
            StatusCode::CONFLICT,
            json!({"status": "failed", "error": message}),
        ),
        Err(AccessError::Throttled) => api_error(StatusCode::TOO_MANY_REQUESTS, WAIT),
        Err(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The sign-in could not be completed.",
        ),
    }
}

/// `DELETE /api/cli/token`: the token a command line presents revokes itself, so signing out on
/// a computer ends what that computer could do.
pub async fn revoke_cli_token(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(token) = auth::bearer(&headers) else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "Only a CLI token can revoke itself.",
        );
    };
    let caller = auth::caller(&state, &headers);
    match state.access.revoke_presented_token(token) {
        Ok((person, token_id)) => {
            audit(
                &state,
                &caller,
                "token.revoke",
                &format!("{person}/{token_id}"),
            );
            Json(json!({"ok": true, "revoked": true})).into_response()
        }
        Err(AccessError::NotFound) => auth::unauthorized(),
        Err(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The token could not be revoked.",
        ),
    }
}

#[derive(Deserialize, Default)]
pub struct CliCode {
    #[serde(default)]
    code: String,
}

/// The page for a code nothing waits under, or for too many of them.
fn cli_refusal(hosted: &Hosted<'_>, error: &AccessError) -> Response {
    let problem = match error {
        AccessError::Throttled => {
            "Too many codes did not match. Wait ten minutes, then run the command again."
        }
        AccessError::Invalid(message) => message.as_str(),
        _ => {
            "No sign-in is waiting under that code. A code lasts 10 minutes and works once. Run the command again for a new one."
        }
    };
    hosted_page(
        "Sign in the command line",
        &hosted_pages::cli_sign_in(
            &hosted.nav(),
            &hosted_pages::CliSignIn::Ask {
                problem: Some(problem),
            },
        ),
    )
}

/// `GET /cli`: where a signed-in person confirms the code their command line shows. Someone not
/// signed in signs in first and comes back to the same code.
pub async fn cli_sign_in_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CliCode>,
) -> Response {
    let caller = auth::caller(&state, &headers);
    let code = cli_user_code(&query.code);
    if matches!(caller.principal, Principal::Anonymous) || caller.session.is_none() {
        let back = code.map_or_else(|| "/cli".to_owned(), |code| format!("/cli?code={code}"));
        return redirect(
            &format!("/?next={}", encode_value(&back)),
            Some("Sign in to confirm the command line."),
        );
    }
    let (hosted, _) = match hosted_member(&state, &caller) {
        Ok(found) => found,
        Err(response) => return response,
    };
    if query.code.trim().is_empty() {
        return hosted_page(
            "Sign in the command line",
            &hosted_pages::cli_sign_in(
                &hosted.nav(),
                &hosted_pages::CliSignIn::Ask { problem: None },
            ),
        );
    }
    match state
        .access
        .waiting_cli_sign_in(&hosted.person.id, &query.code)
    {
        Ok(waiting) => hosted_page(
            "Sign in the command line",
            &hosted_pages::cli_sign_in(
                &hosted.nav(),
                &hosted_pages::CliSignIn::Confirm {
                    code: &waiting.user_code,
                    computer: &waiting.computer,
                },
            ),
        ),
        Err(error) => cli_refusal(&hosted, &error),
    }
}

#[derive(Deserialize)]
pub struct CliDecision {
    csrf: String,
    code: String,
    decision: String,
}

/// `POST /ui/cli`: the person's answer. The organization is the one their session works in,
/// never a form field.
pub async fn decide_cli_sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CliDecision>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let (hosted, org) = match hosted_member(&state, &caller) {
        Ok(found) => found,
        Err(response) => return response,
    };
    let allow = form.decision == "allow";
    let computer = state
        .access
        .waiting_cli_sign_in(&hosted.person.id, &form.code)
        .map(|waiting| waiting.computer);
    let computer = match computer {
        Ok(computer) => computer,
        Err(error) => return cli_refusal(&hosted, &error),
    };
    if let Err(error) = state.access.decide_cli_sign_in(
        || auth::caller(&state, &headers).principal,
        &hosted.person.id,
        &org,
        &form.code,
        allow,
    ) {
        return cli_refusal(&hosted, &error);
    }
    audit(
        &state,
        &caller,
        if allow {
            "cli.sign-in.allow"
        } else {
            "cli.sign-in.cancel"
        },
        &format!("{}/{org}", hosted.person.id),
    );
    let done = if allow {
        hosted_pages::CliSignIn::Allowed {
            computer: &computer,
        }
    } else {
        hosted_pages::CliSignIn::Cancelled
    };
    hosted_page(
        "Sign in the command line",
        &hosted_pages::cli_sign_in(&hosted.nav(), &done),
    )
}
