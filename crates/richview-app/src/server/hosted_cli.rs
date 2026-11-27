//! Signing the command line in from the browser (hosted), in two ways that end in the same
//! sign-in. On a computer with a browser the command line listens on its own loopback port and
//! sends its person to `/cli/authorize`; one click there sends the browser back to that port with
//! a one-time code, which the command line trades for its token with a PKCE verifier. Without a
//! browser it starts a sign-in and shows a short code; its person confirms the code on `/cli`;
//! the command line collects its token once. The routes that answer anyone are bounded before
//! they do any work; the pages and their forms act only for a signed-in browser session; and a
//! token can revoke itself.
//!
//! No address here ever carries a secret: the authorize page's address holds a PKCE challenge,
//! which is public by design, and the `/cli` page's address the short code, which is no
//! credential. The one-time code travels in the redirect to the command line's own loopback port
//! and is useless without the verifier; tokens and verifiers travel only in request and response
//! bodies, which are never stored or logged.
use super::{
    AppState, Hosted, api_error, audit, auth, encode_value, form_caller, hosted_member,
    hosted_page, hosted_pages, moving_on, redirect,
};
use crate::access::{
    AccessError, CLI_SIGN_IN_POLL_SECONDS, CLI_SIGN_IN_SECONDS, CliAuthRequest, CliGrant,
    CliSignInCollected, Principal, cli_user_code,
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
    /// The command line can refresh its sign-in: give it a token that ends after an hour and the
    /// refresh token that renews it, not one that never ends.
    #[serde(default)]
    refresh: bool,
}

/// `POST /api/cli/sign-in`: a command line starts a sign-in. Anyone may; it is bounded by address.
pub async fn start_cli_sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<CliSignInStart>, JsonRejection>,
) -> Response {
    // A body that is too large or no JSON object is refused as what it is.
    let (computer, refresh) = match body {
        Ok(Json(start)) => (start.computer, start.refresh),
        Err(refusal) => return api_error(refusal.status(), "Send a JSON object."),
    };
    match state.access.start_cli_sign_in(
        super::super::hosted_auth::client_ip(&headers),
        &computer,
        refresh,
    ) {
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
            refresh,
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
                    "refreshToken": refresh.as_ref().map(|(refresh, _)| refresh),
                    "expiresIn": refresh.as_ref().map(|(_, seconds)| seconds),
                    "account": user.map(|user| json!({"name": user.name, "email": user.email})),
                    "organization": organization,
                    "organizationId": org,
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

/// Whether `host` (a host name or address as a URL or a `Host` header gives it) is this computer's
/// own: `localhost` or a loopback address. A browser does not scope a cookie by port, so the
/// session cookie of an app reached there goes to every port of the host, a command line's
/// loopback port among them, where another user of the computer may be listening.
fn is_loopback_host(host: &str) -> bool {
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    // `localhost`, a name under it (a browser resolves those to this computer too) and an address
    // that reaches this computer: loopback, however written, or the unspecified one.
    host == "localhost"
        || host.ends_with(".localhost")
        || host.parse::<std::net::IpAddr>().is_ok_and(|address| {
            let address = address.to_canonical();
            address.is_loopback() || address.is_unspecified()
        })
}

/// Whether an origin or an authority names this computer, read as a URL is, so that every
/// spelling of an address is the address (`127.1`, `0x7f.0.0.1`, `2130706433`) and a user name or
/// a port before or after it changes nothing.
fn names_this_computer(origin_or_authority: &str) -> bool {
    let with_scheme = if origin_or_authority.contains("://") {
        origin_or_authority.to_owned()
    } else {
        format!("http://{origin_or_authority}")
    };
    reqwest::Url::parse(&with_scheme)
        .ok()
        .and_then(|url| url.host_str().map(is_loopback_host))
        .unwrap_or(false)
}

/// Whether the app is itself configured to be reached at this computer's own address. Such an app
/// signs a command line in with the short code only.
fn app_at_loopback_address(state: &AppState) -> bool {
    state
        .public_origin
        .as_deref()
        .is_some_and(names_this_computer)
}

/// The same for the browser that asks to authorize a command line: also when it reaches the app
/// at this computer's address under another configured origin (a listener on every interface).
fn browser_at_loopback_address(state: &AppState, headers: &HeaderMap) -> bool {
    app_at_loopback_address(state) || request_at_loopback_address(headers)
}

/// Whether the request was made to this computer's own address, by its `Host`.
fn request_at_loopback_address(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(names_this_computer)
}

/// `GET /api/cli/oauth/metadata`: how this server signs a command line in, so that one that
/// finds none of it asks for a pasted token. Anyone may ask; it says nothing about anyone.
pub async fn cli_oauth_metadata(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !state.access.is_hosted() {
        return auth::not_found();
    }
    // A server that is told no address of its own is reached at whatever the command line was
    // given, and that is where its browser goes: where the consent page would refuse a loopback
    // sign-in there, none is offered. A server with an address of its own is asked about that
    // address, and a command line that reaches it at this computer's own address is sent to
    // `--device` by the consent page's refusal.
    let flows = if app_at_loopback_address(&state)
        || (state.public_origin.is_none() && request_at_loopback_address(&headers))
    {
        json!(["device"])
    } else {
        json!(["loopback", "device"])
    };
    Json(json!({
        "flows": flows,
        "authorize": "/cli/authorize",
        "token": "/api/cli/oauth/token",
        "revoke": "/api/cli/oauth/revoke",
        "refresh": true,
    }))
    .into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliTokenRequest {
    grant_type: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    code_verifier: String,
    #[serde(default)]
    redirect_uri: String,
    #[serde(default)]
    refresh_token: String,
}

/// What the command line is given for a trade or a refresh: the same answer a short-code sign-in
/// collects, with the refresh token that renews the token.
fn granted(state: &AppState, grant: CliGrant, created: bool) -> Response {
    let user = state.access.user(&grant.person);
    if created && let Some(user) = &user {
        // The same audit line a token made on the CLI tokens page leaves.
        let _ = state.access.audit(
            &Principal::User(user.clone()),
            "token.create",
            &format!("{}/{}", grant.person, grant.token_id),
            true,
        );
    }
    let organization = state
        .access
        .organizations_for(&grant.person)
        .into_iter()
        .find(|o| o.id == grant.org)
        .map(|o| o.name);
    unstored(
        StatusCode::OK,
        json!({
            "status": "allowed",
            "token": grant.token,
            "refreshToken": grant.refresh,
            "expiresIn": grant.expires_in,
            "account": user.map(|user| json!({"name": user.name, "email": user.email})),
            "organization": organization,
            "organizationId": grant.org,
        }),
    )
}

/// `POST /api/cli/oauth/token`: the command line trades the code its browser brought back, and
/// the verifier only it knows, for its sign-in; or trades a refresh token for the next one.
/// Anyone may call it; it is bounded by address and every refusal is one answer.
pub async fn cli_oauth_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<CliTokenRequest>, JsonRejection>,
) -> Response {
    let request = match body {
        Ok(Json(request)) => request,
        Err(refusal) if refusal.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            return api_error(refusal.status(), "Send a JSON object with a grantType.");
        }
        Err(_) => {
            return api_error(
                StatusCode::BAD_REQUEST,
                "Send a JSON object with a grantType.",
            );
        }
    };
    let address = super::super::hosted_auth::client_ip(&headers);
    let (result, refreshing) = match request.grant_type.as_str() {
        "authorization_code" => (
            state.access.redeem_cli_authorization(
                address,
                &request.code,
                &request.code_verifier,
                &request.redirect_uri,
            ),
            false,
        ),
        "refresh_token" => (
            state
                .access
                .refresh_cli_grant(address, &request.refresh_token),
            true,
        ),
        _ => return api_error(StatusCode::BAD_REQUEST, "Unknown grantType."),
    };
    match result {
        Ok(grant) => granted(&state, grant, !refreshing),
        Err(AccessError::Throttled) => api_error(StatusCode::TOO_MANY_REQUESTS, WAIT),
        Err(AccessError::Invalid(message)) => unstored(
            StatusCode::CONFLICT,
            json!({"status": "failed", "error": message}),
        ),
        Err(AccessError::Storage(_)) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The sign-in could not be completed.",
        ),
        Err(_) if refreshing => unstored(
            StatusCode::UNAUTHORIZED,
            json!({"status": "ended", "error": "This computer's sign-in has ended. Sign in again."}),
        ),
        Err(_) => unstored(
            StatusCode::BAD_REQUEST,
            json!({"status": "refused", "error": "This sign-in was refused or has expired. Run the command again."}),
        ),
    }
}

#[derive(Deserialize)]
pub struct CliRevokeRequest {
    #[serde(default)]
    token: String,
}

/// `POST /api/cli/oauth/revoke`: a computer signs itself out with the refresh token it keeps,
/// which is all it has left once the hour of its token is over. Anyone may ask; only a token the
/// server knows ends anything, and the answer says whether it did.
pub async fn cli_oauth_revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<CliRevokeRequest>, JsonRejection>,
) -> Response {
    let request = match body {
        Ok(Json(request)) => request,
        Err(refusal) if refusal.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            return api_error(refusal.status(), "Send a JSON object with a token.");
        }
        Err(_) => {
            return api_error(StatusCode::BAD_REQUEST, "Send a JSON object with a token.");
        }
    };
    match state.access.revoke_cli_grant(
        super::super::hosted_auth::client_ip(&headers),
        &request.token,
    ) {
        Ok(Some((person, token_id))) => {
            let actor = state
                .access
                .user(&person)
                .map_or(Principal::Anonymous, Principal::User);
            let _ = state.access.audit(
                &actor,
                "token.revoke",
                &format!("{person}/{token_id}"),
                true,
            );
            unstored(StatusCode::OK, json!({"revoked": true}))
        }
        Ok(None) => unstored(StatusCode::OK, json!({"revoked": false})),
        Err(AccessError::Throttled) => api_error(StatusCode::TOO_MANY_REQUESTS, WAIT),
        Err(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The token could not be revoked.",
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
    /// The organization the page said it would sign in to.
    #[serde(default)]
    org: String,
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
    // The page named one organization; the session may work in another since (a second tab), and
    // a yes is for the one the person read, nothing else.
    if allow && form.org != org {
        return cli_refusal(
            &hosted,
            &AccessError::Invalid(
                "Your organization changed while this page was open. Run the command again.".into(),
            ),
        );
    }
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

#[derive(Deserialize, Default)]
pub struct CliAuthorizeQuery {
    #[serde(default)]
    redirect_uri: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    code_challenge: String,
    #[serde(default)]
    code_challenge_method: String,
    #[serde(default)]
    computer: String,
}

/// The sign-in page's own refusal of an address no command line of this version makes. It names
/// no value of the address and sends nothing to the address's `redirect_uri`.
fn cli_link_refused() -> Response {
    let mut response = hosted_page(
        "Sign in the command line",
        &hosted_pages::sign_in_problem(
            "This sign-in link is not valid",
            "Run richview login in your terminal again; it opens the right page itself.",
        ),
    );
    *response.status_mut() = StatusCode::BAD_REQUEST;
    response
}

/// The refusal for an app reached at `127.0.0.1`: its session cookie would go to the command
/// line's port with the browser (`app_at_loopback_address`).
fn cli_loopback_refused(_state: &AppState) -> Response {
    let mut response = hosted_page(
        "Sign in the command line",
        &hosted_pages::sign_in_problem(
            "Sign in with a code instead",
            "This server is reached at 127.0.0.1, so a sign-in that returns to your computer would carry your browser session with it. Run richview login --device in your terminal and confirm the code on the page it shows.",
        ),
    );
    *response.status_mut() = StatusCode::BAD_REQUEST;
    response
}

/// `GET /cli/authorize`: where a command line on this person's computer sends them. Someone not
/// signed in signs in first, with any of the ways the server offers, and comes back to the same
/// request; a signed-in person sees who is asking and answers with one click.
pub async fn cli_authorize_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CliAuthorizeQuery>,
) -> Response {
    let request = (query.code_challenge_method == "S256")
        .then(|| {
            CliAuthRequest::new(
                &query.redirect_uri,
                &query.state,
                &query.code_challenge,
                &query.computer,
            )
        })
        .flatten();
    let Some(request) = request else {
        return cli_link_refused();
    };
    if browser_at_loopback_address(&state, &headers) {
        return cli_loopback_refused(&state);
    }
    let caller = auth::caller(&state, &headers);
    if matches!(caller.principal, Principal::Anonymous) || caller.session.is_none() {
        return redirect(
            &format!("/?next={}", encode_value(&request.path())),
            Some("Sign in to let the command line publish as you."),
        );
    }
    let (hosted, _) = match hosted_member(&state, &caller) {
        Ok(found) => found,
        Err(response) => return response,
    };
    hosted_page(
        "Sign in the command line",
        &hosted_pages::cli_sign_in(
            &hosted.nav(),
            &hosted_pages::CliSignIn::Authorize { request: &request },
        ),
    )
}

#[derive(Deserialize)]
pub struct CliAuthorizeDecision {
    csrf: String,
    #[serde(default)]
    redirect_uri: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    code_challenge: String,
    #[serde(default)]
    computer: String,
    /// The organization the page said it would sign in to.
    #[serde(default)]
    org: String,
    decision: String,
}

/// `POST /ui/cli/authorize`: the person's answer. The organization is the one their session works
/// in, never a form field. The browser is sent on to the command line's own loopback port with
/// the one-time code, or with a refusal; never anywhere else.
pub async fn decide_cli_authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CliAuthorizeDecision>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let (hosted, org) = match hosted_member(&state, &caller) {
        Ok(found) => found,
        Err(response) => return response,
    };
    let refused = |problem: &str| {
        hosted_page(
            "Sign in the command line",
            &hosted_pages::cli_sign_in(
                &hosted.nav(),
                &hosted_pages::CliSignIn::Refused { problem },
            ),
        )
    };
    if browser_at_loopback_address(&state, &headers) {
        return cli_loopback_refused(&state);
    }
    let Some(request) = CliAuthRequest::new(
        &form.redirect_uri,
        &form.state,
        &form.code_challenge,
        &form.computer,
    ) else {
        return refused(
            "Run richview login in your terminal again; it opens the right page itself.",
        );
    };
    // The page named one organization; the session may work in another since (a second tab).
    if form.decision == "allow" && form.org != org {
        return refused(
            "Your organization changed while this page was open. Run richview login again.",
        );
    }
    if form.decision != "allow" {
        audit(
            &state,
            &caller,
            "cli.sign-in.cancel",
            &format!("{}/{org}", hosted.person.id),
        );
        return moving_on(
            "Cancelled",
            "Nothing was signed in. Taking you back to your terminal.",
            &request.answer_denied(),
        );
    }
    match state.access.issue_cli_authorization(
        || auth::caller(&state, &headers).principal,
        &hosted.person.id,
        &org,
        &request,
    ) {
        Ok(code) => {
            audit(
                &state,
                &caller,
                "cli.sign-in.allow",
                &format!("{}/{org}", hosted.person.id),
            );
            moving_on(
                "Signed in",
                "Taking you back to your terminal.",
                &request.answer_with_code(&code),
            )
        }
        Err(AccessError::Invalid(message)) => refused(&message),
        Err(AccessError::Throttled) => {
            refused("Too many sign-ins are waiting. Wait a minute, then run the command again.")
        }
        Err(_) => refused("This sign-in could not be finished. Run the command again."),
    }
}

#[cfg(test)]
mod loopback_host_tests {
    use super::names_this_computer;

    #[test]
    fn every_spelling_of_this_computers_address_is_this_computer() {
        for named in [
            "http://127.0.0.1:8080",
            "http://127.1:8080",
            "http://0x7f.0.0.1",
            "http://2130706433/",
            "http://127.255.255.254",
            "http://[::1]:3000",
            "http://LOCALHOST:3000",
            "http://user@127.0.0.1:8080/path",
            "127.0.0.1:8080",
            "localhost",
            "localhost.",
            "http://app.localhost:3000",
            "http://[::ffff:127.0.0.1]:80",
            "http://0.0.0.0:8080",
        ] {
            assert!(names_this_computer(named), "{named}");
        }
        for other in [
            "https://app.test",
            "http://127.0.0.1.evil.test",
            "http://10.0.0.1",
            "http://localhost.evil.test",
            "https://app.richview.ai",
            "http://notlocalhost",
            "",
        ] {
            assert!(!names_this_computer(other), "{other}");
        }
    }
}
