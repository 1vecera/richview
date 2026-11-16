//! The report origin (ARC-15). Report pages are author-controlled, so they are served from an
//! origin of their own: sign-in, governance and the API stay on the app origin, and the app's
//! cookie is never honoured on report pages. A signed-in person reaches a private report through
//! a one-minute, single-use ticket that opens one report and nothing else.
use super::{AppState, auth};
use crate::access::Right;
use crate::storage::validate_slug;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, Uri, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// The read session on the report origin; its path is the report's, so it travels nowhere else.
pub const REPORT_COOKIE: &str = "rv_report";
const SESSION_SECONDS: u64 = 3600;

/// Set by the report listener on every request, and removed from whatever a client sends: which
/// cookie counts is decided by the listener that accepted the connection, never by a `Host`
/// header the client chose.
const LISTENER: &str = "x-rv-listener";

/// Whether the request came in on the report listener.
pub fn on_report_origin(_state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get(LISTENER)
        .is_some_and(|v| v.as_bytes() == b"report")
}

/// Marks each request with the listener it arrived on (see `LISTENER`).
pub async fn mark_listener(report: bool, mut request: Request, next: Next) -> Response {
    request.headers_mut().remove(LISTENER);
    if report {
        request
            .headers_mut()
            .insert(LISTENER, header::HeaderValue::from_static("report"));
    }
    next.run(request).await
}

/// Whether a browser says this navigation is the person's own: from the app itself or typed in.
/// A script on a report page loading an app address is `same-site`, a frame is not a `document`,
/// and neither earns a ticket. Requests that carry no fetch metadata are not browser navigations
/// and are left to the cookie checks.
fn own_navigation(headers: &HeaderMap) -> bool {
    let value = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    value("sec-fetch-site").is_none_or(|site| matches!(site, "same-origin" | "none"))
        && value("sec-fetch-dest").is_none_or(|dest| dest == "document")
}

/// The report a `/r/{slug}…` path names.
fn slug_of<'a>(state: &AppState, path: &'a str) -> Option<&'a str> {
    let rest = path.strip_prefix("/r/")?;
    if state.access.is_hosted() {
        let (org, remainder) = rest.split_once('/')?;
        let slug = remainder.split('/').next()?;
        let key = rest.get(..org.len() + 1 + slug.len())?;
        state.storage.validate_key(key).ok().map(|_| key)
    } else {
        let slug = rest.split('/').next()?;
        validate_slug(slug).ok().map(|_| slug)
    }
}

/// App origin, report path: the page lives on the report origin. A signed-in person is handed a
/// ticket for that one report; anyone else is sent without one and sees what is public.
pub async fn app_fallback(State(state): State<AppState>, headers: HeaderMap, uri: Uri) -> Response {
    let (Some(origin), Some(slug)) = (state.report_origin.as_deref(), slug_of(&state, uri.path()))
    else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };
    let caller = auth::caller(&state, &headers);
    let params = |name: &str| {
        uri.query()
            .is_some_and(|q| q.split('&').any(|p| p == format!("rv={name}")))
    };
    // `signin`: sent back from the report origin because the reader had no session there.
    // `open`: the click on the Open report step below, which is the person's own navigation.
    let (sent_back, opened) = (params("signin"), params("open"));
    let own = own_navigation(&headers);
    let signed_in = caller.session.is_some();
    // A person who is not signed in here gets the sign-in page and comes back to the report.
    if (sent_back || opened) && own && !signed_in {
        return sign_in_redirect(&uri);
    }
    // A report the person cannot see or that is not there answers once, never another round trip.
    if signed_in && !state.access.can(&caller.principal, slug, Right::View) {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    // Back from the report origin by a link, another site or a lapsed read session: the app
    // cookie is SameSite=Strict and may not have travelled, so the person takes one same-origin
    // step whose click carries it and earns the ticket.
    if sent_back && !own {
        return super::ui::open_report(&without_rv(&uri));
    }
    let query: Vec<&str> = uri
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|p| !p.is_empty() && *p != "rv=signin" && *p != "rv=open")
        .collect();
    let mut target = format!("{}{}", origin.trim_end_matches('/'), uri.path());
    if !query.is_empty() {
        target.push('?');
        target.push_str(&query.join("&"));
    }
    let mut issued = false;
    if own && signed_in {
        match state
            .access
            .issue_grant(&caller.principal, slug, caller.session.as_deref())
        {
            Some(ticket) => {
                #[cfg(test)]
                crate::test_points::reach("ticket.issued", slug);
                target.push(if query.is_empty() { '?' } else { '&' });
                target.push_str("grant=");
                target.push_str(&ticket);
                issued = true;
            }
            // No ticket to give: a redirect without one would come straight back here.
            None => return super::ui::too_many_tickets(),
        }
    }
    let mut response = (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, target),
            (header::CACHE_CONTROL, "no-store".into()),
            (header::REFERRER_POLICY, "no-referrer".into()),
        ],
    )
        .into_response();
    if issued {
        // A ticket is issued once: this answer carries no report bytes, and the report origin
        // decides again when the ticket is used.
        response.extensions_mut().insert(super::Settled);
    }
    response
}

/// The path and query of an address, without the markers this module adds.
fn without_rv(uri: &Uri) -> String {
    let query: Vec<&str> = uri
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|p| !p.is_empty() && *p != "rv=signin" && *p != "rv=open")
        .collect();
    if query.is_empty() {
        uri.path().to_owned()
    } else {
        format!("{}?{}", uri.path(), query.join("&"))
    }
}

/// The sign-in page, remembering the report (and its query) to return to.
fn sign_in_redirect(uri: &Uri) -> Response {
    let next = super::ui::encode_value(&without_rv(uri));
    (
        StatusCode::SEE_OTHER,
        [
            (
                header::LOCATION,
                format!("/?m=Sign%20in%20to%20open%20this%20report.&next={next}"),
            ),
            (header::CACHE_CONTROL, "no-store".into()),
        ],
    )
        .into_response()
}

/// Report origin: a `?grant=` ticket becomes a read session cookie for that report, and the
/// address loses the ticket.
pub async fn redeem(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let uri = request.uri().clone();
    let ticket = uri.query().and_then(|q| {
        q.split('&')
            .find_map(|pair| pair.strip_prefix("grant=").map(str::to_owned))
    });
    let Some(ticket) = ticket else {
        return next.run(request).await;
    };
    let rest: Vec<&str> = uri
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.starts_with("grant="))
        .collect();
    let target = if rest.is_empty() {
        uri.path().to_owned()
    } else {
        format!("{}?{}", uri.path(), rest.join("&"))
    };
    let mut response = (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, target),
            (header::CACHE_CONTROL, "no-store".into()),
            (header::REFERRER_POLICY, "no-referrer".into()),
        ],
    )
        .into_response();
    if let Some(slug) = slug_of(&state, uri.path())
        && let Some(value) = state.access.redeem_grant(&ticket, slug)
    {
        #[cfg(test)]
        crate::test_points::reach("ticket.redeemed", slug);
        let secure = state
            .report_origin
            .as_deref()
            .is_some_and(|o| o.starts_with("https://"));
        let cookie = format!(
            "{REPORT_COOKIE}={value}; HttpOnly; SameSite=Lax; Path=/r/{slug}; Max-Age={SESSION_SECONDS}{}",
            if secure { "; Secure" } else { "" }
        );
        if let Ok(cookie) = cookie.parse() {
            response.headers_mut().insert(header::SET_COOKIE, cookie);
        }
    }
    response
}
