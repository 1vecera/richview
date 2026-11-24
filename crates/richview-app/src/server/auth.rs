//! Who is calling, and whether they may: bearer tokens, session cookies, CSRF and the one place
//! that turns a missing right into a status (FEA-SF-19, FEA-SF-64).
//!
//! A denial reveals nothing: a caller who cannot see a report gets the same 404 as for a report
//! that does not exist. Only a caller who can see it but lacks the right gets a 403.
use super::{AppState, api_error};
use crate::access::{Access, AccessError, Principal, Right};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

pub const SESSION_COOKIE: &str = "rv_session";
pub const CSRF_HEADER: &str = "x-richview-csrf";

/// A caller and how they proved who they are.
pub struct Caller {
    pub principal: Principal,
    /// The session cookie value when the caller is signed in by cookie rather than by bearer token.
    pub session: Option<String>,
}

pub(super) fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// A cookie's value from the `Cookie` header.
pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_owned())
}

/// The caller of a request. A bearer token, when present, decides alone; otherwise the session
/// cookie does.
pub fn caller(state: &AppState, headers: &HeaderMap) -> Caller {
    if let Some(token) = bearer(headers) {
        return Caller {
            principal: state.access.authenticate(Some(token), None),
            session: None,
        };
    }
    // Report pages never act on the app's sign-in cookie, even when the browser sends it along
    // (cookies are not scoped by port): only `reader` lets a per-report session through (ARC-15).
    if super::origin::on_report_origin(state, headers) {
        return Caller {
            principal: Principal::Anonymous,
            session: None,
        };
    }
    let session = cookie(headers, SESSION_COOKIE);
    let principal = state.access.authenticate(None, session.as_deref());
    Caller {
        session: session.filter(|_| !matches!(principal, Principal::Anonymous)),
        principal,
    }
}

/// The caller of a request for one report's pages: on the report origin, the read session that
/// the app origin handed over for exactly this report; elsewhere the ordinary caller.
pub fn reader(state: &AppState, headers: &HeaderMap, slug: &str) -> Caller {
    if bearer(headers).is_none() && super::origin::on_report_origin(state, headers) {
        let principal = cookie(headers, super::origin::REPORT_COOKIE)
            .map_or(Principal::Anonymous, |value| {
                state.access.authenticate_report(&value, slug)
            });
        return Caller {
            principal,
            session: None,
        };
    }
    caller(state, headers)
}

pub fn not_found() -> Response {
    api_error(StatusCode::NOT_FOUND, "Not found")
}

pub fn unauthorized() -> Response {
    let mut response = api_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, "Bearer".parse().unwrap());
    response
}

/// Whether a browser request came from this server's own pages.
pub fn same_origin(headers: &HeaderMap) -> bool {
    if headers
        .get("sec-fetch-site")
        .is_some_and(|site| site != "same-origin" && site != "none")
    {
        return false;
    }
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    // A no-referrer form submits an opaque Origin in browsers. Fetch Metadata is a
    // forbidden request header for web content, so only the browser can assert that
    // this navigation came from the same origin. Fail closed if it is absent.
    if origin == "null" {
        return headers
            .get("sec-fetch-site")
            .is_some_and(|site| site == "same-origin");
    }
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    origin
        .split_once("://")
        .is_some_and(|(_, authority)| authority == host)
}

/// A state-changing request needs a known caller; one made by cookie also needs the session's
/// CSRF token and a same-origin `Origin`. A server with no credentials at all answers 503.
pub fn mutating(state: &AppState, headers: &HeaderMap) -> Result<Caller, Response> {
    let caller = caller(state, headers);
    if matches!(caller.principal, Principal::Anonymous) {
        if !state.access.has_credentials() {
            return Err(api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Server has no API token configured",
            ));
        }
        return Err(unauthorized());
    }
    if let Some(session) = &caller.session {
        let wanted = state.access.csrf_for(session);
        let given = headers.get(CSRF_HEADER).and_then(|v| v.to_str().ok());
        if wanted.is_none() || wanted.as_deref() != given || !same_origin(headers) {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                "Missing or wrong CSRF token",
            ));
        }
    }
    Ok(caller)
}

/// The caller must hold `right` on the report; otherwise the answer says as little as possible.
pub fn authorize(
    state: &AppState,
    caller: &Caller,
    slug: &str,
    right: Right,
) -> Result<(), Response> {
    // An organization administrator's rights apply only to reports that actually have an ACL.
    // Otherwise a guessed, uncreated address would look accessible and could be pre-shared.
    if state.access.is_hosted() && !state.access.has_record(slug) {
        return Err(not_found());
    }
    let rights = state.access.rights(&caller.principal, slug);
    if rights.contains(&right) {
        #[cfg(test)]
        crate::test_points::reach(&format!("authorized.{right:?}").to_lowercase(), slug);
        return Ok(());
    }
    // Anonymous probes of reports they cannot see are not worth a line each.
    if !matches!(caller.principal, Principal::Anonymous) {
        let _ = state.access.audit(
            &caller.principal,
            &format!("denied.{right:?}").to_lowercase(),
            slug,
            false,
        );
    }
    if !rights.contains(&Right::View) {
        return Err(not_found());
    }
    Err(api_error(
        StatusCode::FORBIDDEN,
        format!("Your role does not include the right to {right:?} this report.").to_lowercase(),
    ))
}

/// The caller must be an admin (or the service).
pub fn admin(state: &AppState, headers: &HeaderMap, changes: bool) -> Result<Caller, Response> {
    let caller = if changes {
        mutating(state, headers)?
    } else {
        let caller = caller(state, headers);
        if matches!(caller.principal, Principal::Anonymous) {
            return Err(unauthorized());
        }
        caller
    };
    if state.access.is_admin(&caller.principal) {
        return Ok(caller);
    }
    let _ = state
        .access
        .audit(&caller.principal, "denied.governance", "", false);
    Err(api_error(
        StatusCode::FORBIDDEN,
        "Only an admin can manage people and roles.",
    ))
}

/// The status and sentence for an access-state error.
pub fn access_error(error: AccessError) -> Response {
    match error {
        AccessError::Credentials => api_error(StatusCode::UNAUTHORIZED, "Wrong email or password."),
        AccessError::Throttled => api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many failed sign-ins. Try again in 15 minutes.",
        ),
        AccessError::Invalid(message) => api_error(StatusCode::BAD_REQUEST, message),
        AccessError::NotFound => not_found(),
        AccessError::Storage(message) => api_error(StatusCode::INTERNAL_SERVER_ERROR, message),
    }
}

/// `Set-Cookie` for a session. `Secure` once the server's public address is HTTPS.
pub fn session_cookie(state: &AppState, value: &str, max_age: u64) -> String {
    let secure = state
        .public_origin
        .as_deref()
        .is_some_and(|o| o.starts_with("https://"));
    format!(
        "{SESSION_COOKIE}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" }
    )
}

/// `Set-Cookie` for a sign-in with a provider under way: it ties the provider's answer to the
/// browser that asked. `Lax`, because the provider's answer arrives as a top-level navigation from
/// another site, which a `Strict` cookie would not accompany; scoped to the sign-in routes; `Secure`
/// once the server's public address is HTTPS. `max_age` 0 ends it.
pub fn flow_cookie(state: &AppState, value: &str, max_age: u64) -> String {
    let secure = state
        .public_origin
        .as_deref()
        .is_some_and(|o| o.starts_with("https://"));
    format!(
        "{}={value}; HttpOnly; SameSite=Lax; Path=/sign-in; Max-Age={max_age}{}",
        crate::social::COOKIE,
        if secure { "; Secure" } else { "" }
    )
}

#[derive(Deserialize)]
pub struct SignIn {
    pub email: String,
    pub password: String,
}

/// Refuse excess work before it can occupy a blocking worker, including after a client disconnects.
pub(super) async fn sign_in_bounded(
    access: Access,
    email: &str,
    password: &str,
) -> Result<(String, String), AccessError> {
    let permit = access.admit_sign_in(email, password)?;
    let (email, password) = (email.to_owned(), password.to_owned());
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        access.sign_in(&email, &password)
    })
    .await
    .map_err(|_| AccessError::Storage("Sign-in worker failed".into()))?
}

/// `POST /api/session`: sign in with an email and a password.
pub async fn sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SignIn>,
) -> Response {
    if !same_origin(&headers) {
        return api_error(StatusCode::FORBIDDEN, "Cross-site sign-in is refused.");
    }
    let result = sign_in_bounded(state.access.clone(), &body.email, &body.password).await;
    match result {
        Ok((cookie, csrf)) => {
            let principal = state.access.authenticate(None, Some(&cookie));
            let _ = state.access.audit(&principal, "sign-in", "", true);
            let mut response =
                Json(json!({"user": user_json(&principal), "csrf": csrf})).into_response();
            response.headers_mut().insert(
                header::SET_COOKIE,
                session_cookie(&state, &cookie, 12 * 3600).parse().unwrap(),
            );
            response
        }
        Err(error) => {
            let _ = state.access.audit(
                &Principal::Anonymous,
                "sign-in",
                &body.email.trim().to_ascii_lowercase(),
                false,
            );
            access_error(error)
        }
    }
}

/// `DELETE /api/session`: sign out.
pub async fn sign_out(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let caller = match mutating(&state, &headers) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Some(session) = &caller.session
        && let Err(error) = state
            .access
            .sign_out(|| self::caller(&state, &headers).principal, session)
    {
        return access_error(error);
    }
    let _ = state.access.audit(&caller.principal, "sign-out", "", true);
    let mut response = Json(json!({"ok": true})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        session_cookie(&state, "", 0).parse().unwrap(),
    );
    response
}

/// `GET /api/session`: who the caller is, and the CSRF token to send with changes.
pub async fn whoami(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let caller = caller(&state, &headers);
    let csrf = caller
        .session
        .as_deref()
        .and_then(|s| state.access.csrf_for(s));
    Json(json!({"user": user_json(&caller.principal), "csrf": csrf})).into_response()
}

fn user_json(principal: &Principal) -> serde_json::Value {
    match principal {
        Principal::User(user) => json!({
            "id": user.id, "name": user.name, "email": user.email, "role": user.role
        }),
        Principal::Service => json!({"id": "service", "name": "Service", "role": "admin"}),
        Principal::Anonymous => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod origin_tests {
    use super::*;

    #[test]
    fn null_origin_requires_same_origin_browser_metadata() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "richview.test".parse().unwrap());
        headers.insert(header::ORIGIN, "null".parse().unwrap());
        assert!(!same_origin(&headers));
        headers.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(!same_origin(&headers));
        headers.insert("sec-fetch-site", "same-origin".parse().unwrap());
        assert!(same_origin(&headers));
        headers.insert(header::ORIGIN, "https://evil.test".parse().unwrap());
        assert!(!same_origin(&headers));
        headers.remove(header::ORIGIN);
        headers.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(!same_origin(&headers));
        headers.remove("sec-fetch-site");
        assert!(same_origin(&headers));
    }
}
