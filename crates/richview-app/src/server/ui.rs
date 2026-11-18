//! The pages of the report host: sign-in, the report menu and the governance section
//! (FEA-SF-18, FEA-SF-19). Plain forms and links work without script; hosted pages use the docs
//! clipboard runtime. Session changes use CSRF-protected POST forms.
#[path = "hosted_pages.rs"]
mod hosted_pages;
/// The host bar draws faces as the app does.
pub(super) use hosted_pages::initials;
#[path = "hosted_teams.rs"]
mod hosted_teams;
pub use hosted_teams::*;

use super::{
    AppState, api_error,
    auth::{self, Caller, access_error},
};
use crate::access::{OrgRole, Principal, Right, User, Visibility};
use axum::{
    Form,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::Engine as _;
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;

// The site's private-build header and footer, with their site-relative links made absolute for
// the separate Cowork origin. Styles and typefaces come from the same source files as the site.
const HEADER: &str = include_str!("site-header.html");
const FOOTER: &str = include_str!("site-footer.html");
const SITE_CSS: &str = include_str!("../../../../assets/site.css");
const DOCS_CSS: &str = include_str!("../../../../assets/docs/docs.css");
const COWORK_CSS: &str = include_str!("cowork.css");
const EDITORIAL_FONTS: &str = include_str!("../../../richview-core/assets/fonts-editorial.css");
const UI_FONTS: &str = include_str!("../../../richview-core/assets/fonts-sleek.css");

/// The site's mark as the page icon, carried in the page like its styles and typefaces: the
/// hosted origin serves no asset files, and a browser asked for none finds none at /favicon.ico.
fn icon() -> &'static str {
    static ICON: OnceLock<String> = OnceLock::new();
    ICON.get_or_init(|| {
        let mark = include_str!("../../../../assets/landing/favicon.svg")
            .trim()
            .replace('%', "%25")
            .replace('#', "%23")
            .replace('<', "%3C")
            .replace('>', "%3E")
            .replace('"', "'");
        format!("<link rel=\"icon\" type=\"image/svg+xml\" href=\"data:image/svg+xml,{mark}\">")
    })
}

/// The chrome of a signed-out page: the mark above one card, and only the legal links below it,
/// as the sign-in of any product people know. A page opts in by opening with the plain
/// `cowork-auth` column; the delete confirmation adds a class and keeps the signed-in chrome.
const AUTH_OPENING: &str = "<div class=\"cowork-auth\">";
/// An invitation's card (`hosted_pages::accept_invite`), first in its body: the renderer lifts it
/// onto the ink side, where it takes the place of the flagship's card and the facts.
const INVITE_OPENING: &str = "<figure class=\"auth-invite\"";
const INVITE_END: &str = "</figure>";
const AUTH_FOOTER: &str = "<footer class=\"auth-foot\"><a href=\"https://richview.ai/privacy\">Privacy</a><a href=\"https://richview.ai/terms\">Terms</a><a href=\"https://richview.ai/impressum\">Impressum</a></footer>";

/// What the product makes, beside every signed-out form: the flagship report's share card as
/// RichView drew it (ARC-69), carried in the page like the mark because the app's pages allow images
/// only from `data:`. Drawn by the release build of 681116c81e from examples/openai-vs-anthropic.
fn auth_card() -> &'static str {
    static CARD: OnceLock<String> = OnceLock::new();
    CARD.get_or_init(|| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(include_bytes!("sign-in-card.png"))
        )
    })
}

/// The ink side of a signed-out page: the mark home, the card and three facts about the app. The
/// form's own words stay in the card beside it.
fn auth_side() -> &'static str {
    static SIDE: OnceLock<String> = OnceLock::new();
    SIDE.get_or_init(|| {
        let fact = |glyph: &str, words: &str| {
            format!(
                "<li><svg class=\"ic\" aria-hidden=\"true\"><use href=\"#i-{glyph}\"/></svg>{words}</li>"
            )
        };
        format!(
            "<aside class=\"auth-side\">{}<figure class=\"auth-show\"><img src=\"{}\" alt=\"The share card of the report The Most Expensive Race Ever Run: its title beside its first chart, a bar chart of Anthropic’s share by population.\" width=\"1200\" height=\"630\"><figcaption>A report’s share card, drawn by RichView in the report’s own style.</figcaption></figure><ul class=\"auth-facts\">{}{}{}</ul></aside>",
            auth_header(),
            auth_card(),
            fact("lock", "Private until you share them"),
            fact("users", "Owner, admin, author and reader roles"),
            fact("report", "Every version kept at its own address")
        )
    })
}

/// The ink side of a signed-out page without a form, such as an error: the mark alone, so the
/// message stands by itself.
fn auth_plain() -> &'static str {
    static SIDE: OnceLock<String> = OnceLock::new();
    SIDE.get_or_init(|| {
        format!(
            "<aside class=\"auth-side auth-side--plain\">{}</aside>",
            auth_header()
        )
    })
}

fn auth_header() -> &'static str {
    static HEAD: OnceLock<String> = OnceLock::new();
    HEAD.get_or_init(|| {
        // The site header's own brand link, so the mark and its link stay one source.
        let start = HEADER
            .find("<a class=\"nav__brand\"")
            .expect("site header has its brand");
        let end = start + HEADER[start..].find("</a>").expect("brand link closes") + 4;
        format!(
            "<header class=\"auth-head\">{}</header>",
            HEADER[start..end].replace("class=\"nav__brand\"", "class=\"auth-brand\"")
        )
    })
}

fn site_rules() -> &'static str {
    // The site stages separate font URLs; Cowork uses the same faces as data URLs instead.
    &SITE_CSS[SITE_CSS.find(":root {").expect("site CSS has its tokens")..]
}

fn display_fonts() -> &'static str {
    static FONTS: OnceLock<String> = OnceLock::new();
    FONTS.get_or_init(|| {
        [(400, include_bytes!("../../../../assets/site-fonts/bricolage-grotesque-400.woff2").as_slice()),
         (600, include_bytes!("../../../../assets/site-fonts/bricolage-grotesque-600.woff2").as_slice())]
            .into_iter()
            .map(|(weight, font)| format!("@font-face{{font-family:'Bricolage Grotesque';src:url(data:font/woff2;base64,{}) format('woff2');font-weight:{weight};font-display:swap}}", base64::engine::general_purpose::STANDARD.encode(font)))
            .collect()
    })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn page(title: &str, body: &str) -> Response {
    render_page(title, body, false)
}

/// Hosted pages reuse the docs clipboard runtime on the app origin. Report pages have a separate policy.
fn hosted_page(title: &str, body: &str) -> Response {
    render_page(title, body, true)
}

/// A signed-in page opens with the app's rail; its chrome ends with the top bar.
const APP_OPENING: &str = "<aside class=\"app-rail\"";
const APP_CHROME_END: &str = "</header>";

fn render_page(title: &str, body: &str, hosted: bool) -> Response {
    let body = body.replace("<button type=", "<button class=\"docs-button\" type=");
    let (invite, body) = body
        .starts_with(INVITE_OPENING)
        .then(|| body.find(INVITE_END))
        .flatten()
        .map_or(("", &body[..]), |end| body.split_at(end + INVITE_END.len()));
    // Signed-out pages: the ink side, the form, the legal links. Signed-in pages: the rail and top bar
    // the page opened with, lifted out of the content. Anything else keeps the site's chrome.
    let app_split = body
        .starts_with(APP_OPENING)
        .then(|| body.find(APP_CHROME_END))
        .flatten()
        .map(|end| end + APP_CHROME_END.len());
    let (classes, header, content, footer) = if body.starts_with(AUTH_OPENING) {
        (
            "docs-page auth-page",
            format!(
                "{}{}",
                hosted_pages::ICONS,
                if !invite.is_empty() {
                    format!(
                        "<aside class=\"auth-side auth-side--invite\">{}{invite}</aside>",
                        auth_header()
                    )
                } else if body.contains("<form") {
                    auth_side().to_owned()
                } else {
                    auth_plain().to_owned()
                }
            ),
            body,
            AUTH_FOOTER,
        )
    } else if let Some(end) = app_split {
        (
            "docs-page app-page",
            format!(
                "{}<a class=\"skip\" href=\"#content\">Skip to content</a>{}",
                hosted_pages::ICONS,
                &body[..end]
            ),
            &body[end..],
            "",
        )
    } else {
        ("docs-page", HEADER.to_owned(), body, FOOTER)
    };
    let main = if classes == "docs-page app-page" {
        "<main class=\"cowork\" id=\"content\">"
    } else {
        "<main class=\"cowork\">"
    };
    let (policy, page_type, scripts) = if hosted {
        (
            "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; style-src 'unsafe-inline'; font-src data:; img-src data:; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
            " data-richview-page=\"docs\"",
            "<script src=\"/richview_docs.js\" defer></script><script src=\"/bootstrap.js\" defer></script>",
        )
    } else {
        (
            "default-src 'none'; style-src 'unsafe-inline'; font-src data:; img-src data:; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
            "",
            "",
        )
    };
    let mut response = (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::CONTENT_SECURITY_POLICY, policy),
        ],
        format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{} · RichView</title>{}<style>{}\n{DOCS_CSS}\n{EDITORIAL_FONTS}\n{UI_FONTS}\n{}\n{COWORK_CSS}</style></head><body class=\"{classes}\"{page_type}>{header}{main}{content}</main>{footer}{scripts}</body></html>",
            escape(title), icon(), site_rules(), display_fonts()
        ),
    )
        .into_response();
    if hosted {
        response
            .headers_mut()
            .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    }
    response
}

/// Format the stored publication date without changing its calendar day.
fn report_date(timestamp: &str) -> String {
    crate::time::leading_date(timestamp)
        .map_or_else(|| timestamp.to_owned(), crate::time::day_month_year)
}

/// Show a stored token timestamp in UTC without losing its minute.
fn token_date(timestamp: &str) -> String {
    crate::time::parse_rfc3339(timestamp)
        .and_then(|parsed| parsed.instant.utc_minute())
        .unwrap_or_else(|| timestamp.to_owned())
}

pub(super) fn hosted_error_page(status: StatusCode) -> Response {
    let (title, message) = match status {
        StatusCode::NOT_FOUND => (
            "Not found",
            "This page or report is not available. If someone sent you this link, ask them for a new one.",
        ),
        StatusCode::TOO_MANY_REQUESTS => ("Try again shortly", "Please try again shortly."),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            ("Access unavailable", "You cannot open this page.")
        }
        _ => (
            "Something went wrong",
            "The request could not be completed.",
        ),
    };
    let mut response = hosted_page(
        title,
        &format!(
            "<div class=\"cowork-auth\"><h1>{title}</h1><p class=\"lede\">{message}</p><p class=\"ways\"><span>Go back and try again, or <a href=\"/\">sign in or go to your reports</a>.</span></p></div>"
        ),
    );
    *response.status_mut() = status;
    response
}

/// The hosted account request takes only an email address.
pub async fn sign_up_page() -> Response {
    hosted_page("Create account", &hosted_pages::sign_up())
}

/// The hosted password reset request does not disclose whether an account exists.
pub async fn forgot_page() -> Response {
    hosted_page("Reset password", &hosted_pages::forgot())
}

/// Inspect a single-use link without consuming it; mail scanners may open it first.
pub async fn link_page(
    State(state): State<AppState>,
    AxumPath(token): AxumPath<String>,
) -> Response {
    use crate::access::LinkPurpose;
    match state.access.inspect_link(&token) {
        Some(LinkPurpose::Signup) => hosted_page(
            "Set up your account",
            &hosted_pages::complete_link(&token, true),
        ),
        Some(LinkPurpose::Reset) => hosted_page(
            "Choose a new password",
            &hosted_pages::complete_link(&token, false),
        ),
        Some(LinkPurpose::Invite) => hosted_teams::invitation_page(&state, &token, "", "").await,
        None => expired_link_page(),
    }
}

pub(super) fn link_error_page(
    state: &AppState,
    token: &str,
    purpose: crate::access::LinkPurpose,
    name: &str,
    error: &str,
) -> Response {
    if state.access.inspect_link(token) != Some(purpose) {
        return expired_link_page();
    }
    let signup = purpose == crate::access::LinkPurpose::Signup;
    let title = if signup {
        "Set up your account"
    } else {
        "Choose a new password"
    };
    let mut response = hosted_page(
        title,
        &hosted_pages::complete_link_with_error(token, signup, name, error),
    );
    *response.status_mut() = StatusCode::BAD_REQUEST;
    response
}

pub(super) async fn invitation_error_page(
    state: &AppState,
    token: &str,
    name: &str,
    error: &str,
) -> Response {
    let mut response = hosted_teams::invitation_page(state, token, name, error).await;
    if response.status() == StatusCode::OK {
        *response.status_mut() = StatusCode::BAD_REQUEST;
    }
    response
}

pub(super) fn check_mail_page(state: &AppState) -> Response {
    let manual = state.access.manual_outbox().unwrap_or(true);
    let sender = state.access.mail_queue().and_then(|queue| queue.sender());
    hosted_page(
        "Check your mail",
        &hosted_pages::check_mail(manual, sender.as_deref()),
    )
}

pub(super) fn expired_link_page() -> Response {
    let mut response = hosted_page(
        "Link expired or already used",
        &hosted_pages::expired_link(),
    );
    *response.status_mut() = StatusCode::GONE;
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}

pub(super) fn signups_paused_page() -> Response {
    let mut response = hosted_page("Sign-ups are paused", &hosted_pages::signups_paused());
    *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
    response
}

pub(super) fn error_page(status: StatusCode) -> Response {
    let (title, message) = match status {
        StatusCode::NOT_FOUND => (
            "Not found",
            "This page or report is not available. If someone sent you this link, ask them for a new one.",
        ),
        StatusCode::TOO_MANY_REQUESTS => ("Try again shortly", "Please try again shortly."),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            ("Access unavailable", "You cannot open this page.")
        }
        _ => (
            "Something went wrong",
            "The request could not be completed.",
        ),
    };
    // The same centred column as every signed-out page, with one way on for someone who followed a
    // link here: the same words for every address, so an unknown and a forbidden one look alike.
    let mut response = page(
        title,
        &format!(
            "<div class=\"cowork-auth\"><h1>{title}</h1><p class=\"lede\">{message}</p><p class=\"ways\"><a href=\"https://richview.ai/\">What is RichView?</a></p></div>"
        ),
    );
    *response.status_mut() = status;
    response
}

/// The one same-origin step that lets a person open a report after a link took them through the
/// report origin: the click is their own navigation, so it carries the app cookie and earns the
/// ticket, or leads a signed-out person to sign-in.
pub(super) fn open_report(path: &str) -> Response {
    let target = safe_next(path).unwrap_or("/");
    let href = format!(
        "{target}{}rv=open",
        if target.contains('?') { '&' } else { '?' }
    );
    page(
        "Open report",
        &format!(
            "<div class=\"cowork-auth\"><h1>Your report is ready</h1><p class=\"lede\">Reports open on their own address, apart from your account, so nothing in a report can reach it.</p><p class=\"cowork-actions\"><a class=\"docs-button\" href=\"{}\">Open report</a></p><p class=\"ways\"><a href=\"/\">Your reports</a></p></div>",
            escape(&href)
        ),
    )
}

/// The report reader can retry the same address after the page-read cap clears.
pub(super) fn report_busy_page() -> Response {
    let mut response = page(
        "Try again shortly",
        r#"<div class="cowork-auth"><h1>Try again shortly</h1><p class="lede">The report service is busy. Wait a moment, then load the page again.</p><p class="ways"><a href="">Try again</a></p></div>"#,
    );
    *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, "1".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}

/// The answer when the person has opened many reports in a minute: a page, never a redirect that
/// comes straight back.
pub(super) fn too_many_tickets() -> Response {
    let mut response = page(
        "Try again shortly",
        "<div class=\"cowork-auth\"><h1>Try again shortly</h1><p class=\"lede\">You opened many reports in a minute. Wait a minute, then open the report again.</p><p class=\"ways\"><a href=\"/\">Your reports</a></p></div>",
    );
    *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, "60".parse().unwrap());
    response
}

fn redirect(to: &str, notice: Option<&str>) -> Response {
    let target = match notice {
        Some(text) => {
            let encoded: String = text
                .bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect();
            format!(
                "{to}{}m={encoded}",
                if to.contains('?') { '&' } else { '?' }
            )
        }
        None => to.to_owned(),
    };
    (StatusCode::SEE_OTHER, [(header::LOCATION, target)]).into_response()
}

#[derive(Deserialize, Default)]
pub struct Notice {
    #[serde(default)]
    m: String,
    /// Where sign-in returns to: a report address sent here from the report origin.
    #[serde(default)]
    next: String,
    /// Hosted report list: `mine`, `shared` or `org`; anything else shows all.
    #[serde(default)]
    show: String,
    /// Hosted report list: words the title must contain.
    #[serde(default)]
    q: String,
    /// A report's page reached from its share link's new account: forward to the report itself.
    #[serde(default)]
    open: String,
}

/// A return address that is a report path on this host, with its query, and nothing else.
pub(super) fn safe_next(next: &str) -> Option<&str> {
    let rest = next.strip_prefix("/r/")?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let plain = |text: &str, extra: &[u8]| {
        let bytes = text.as_bytes();
        bytes.iter().enumerate().all(|(i, &b)| {
            b.is_ascii_alphanumeric()
                || matches!(b, b'.' | b'_' | b'~' | b'-')
                || extra.contains(&b)
                || (b == b'%'
                    && bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                    && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit))
        })
    };
    // Dot segments are judged after the escapes are undone: `%2e%2e` is `..` to a browser.
    let decoded = percent_decode(path);
    (plain(path, b"/")
        && plain(query, b"=&+,")
        && !path.is_empty()
        && !decoded.contains('\\')
        && !decoded.starts_with('/')
        && !decoded.contains("//")
        && !decoded.split('/').any(|part| part == ".." || part == "."))
    .then_some(next)
}

/// The text of a path with its `%XX` escapes undone, bytes that are not UTF-8 replaced.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let pair = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], pair.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `text` as one query value: everything but unreserved characters and `/` is escaped.
pub(super) fn encode_value(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'_' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn notice_html(notice: &Notice) -> String {
    if notice.m.is_empty() {
        String::new()
    } else {
        format!(
            "<p class=\"note\" role=\"status\">{}</p>",
            escape(&notice.m)
        )
    }
}

/// The CSRF token of a signed-in browser session, or the response to send instead.
fn session_csrf(state: &AppState, caller: &Caller) -> Option<String> {
    caller
        .session
        .as_deref()
        .and_then(|s| state.access.csrf_for(s))
}

/// A form post: the caller must be signed in by cookie and the form must carry the session's
/// CSRF token. A bearer token is for the API, not for forms.
/// The answer to a form post from nobody signed in: back to the sign-in page, before any of the
/// body is read.
pub(super) fn sign_in_first() -> Response {
    redirect("/", Some("Sign in first."))
}

fn form_caller(state: &AppState, headers: &HeaderMap, csrf: &str) -> Result<Caller, Response> {
    let caller = auth::caller(state, headers);
    if matches!(caller.principal, Principal::Anonymous) {
        return Err(sign_in_first());
    }
    let wanted = session_csrf(state, &caller);
    let origin_ok = if state.access.is_hosted() {
        auth::same_origin(headers)
    } else {
        headers
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|origin| {
                let host = headers
                    .get(header::HOST)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default();
                origin
                    .split_once("://")
                    .is_some_and(|(_, authority)| authority == host)
            })
    };
    if wanted.as_deref() != Some(csrf) || !origin_ok {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Missing or wrong CSRF token",
        ));
    }
    Ok(caller)
}

fn admin_form(state: &AppState, headers: &HeaderMap, csrf: &str) -> Result<Caller, Response> {
    let caller = form_caller(state, headers, csrf)?;
    if !state.access.is_admin(&caller.principal) {
        let _ = state
            .access
            .audit(&caller.principal, "denied.governance", "", false);
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Only an admin can manage people and roles.",
        ));
    }
    Ok(caller)
}

fn audit(state: &AppState, caller: &Caller, action: &str, target: &str) {
    let _ = state.access.audit(&caller.principal, action, target, true);
}

fn user_of(principal: &Principal) -> Option<&User> {
    match principal {
        Principal::User(user) => Some(user),
        _ => None,
    }
}

fn sign_in_form(notice: &str, next: &str) -> String {
    let next = safe_next(next).map_or(String::new(), |next| {
        format!(
            "<input type=\"hidden\" name=\"next\" value=\"{}\">",
            escape(next)
        )
    });
    format!(
        "<h1>RichView</h1>{notice}<p>Sign in to see the reports shared with you.</p>\
<form method=\"post\" action=\"/ui/sign-in\">{next}<p><label>Email<br><input type=\"email\" name=\"email\" required autocomplete=\"username\"></label></p>\
<p><label>Password<br><input type=\"password\" name=\"password\" required autocomplete=\"current-password\"></label></p>\
<p><button type=\"submit\">Sign in</button></p></form>"
    )
}

/// `GET /`: the sign-in form, or the report menu of the signed-in person. On `richview serve` it
/// is the preview instead.
pub async fn index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(notice): Query<Notice>,
) -> Response {
    if let Some(page) = super::preview_page(&state).await {
        return page;
    }
    let caller = auth::caller(&state, &headers);
    if state.access.is_hosted() {
        return hosted_index(&state, &headers, &caller, &notice).await;
    }
    let next = notice.next.clone();
    let notice = notice_html(&notice);
    let Some(csrf) = session_csrf(&state, &caller) else {
        return page("Sign in", &sign_in_form(&notice, &next));
    };
    let storage = state.storage.clone();
    let rows = tokio::task::spawn_blocking(move || storage.list())
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let name = user_of(&caller.principal).map_or("", |u| u.name.as_str());
    let mut body = format!(
        "<div class=\"bar\"><h1>Reports</h1><form method=\"post\" action=\"/ui/sign-out\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><span>{} </span><button type=\"submit\">Sign out</button></form></div>{notice}",
        escape(&csrf),
        escape(name)
    );
    if state.access.is_admin(&caller.principal) {
        body.push_str("<p><a href=\"/governance\">People, roles and audit trail</a></p>");
    }
    let mut shown = 0;
    let mut table = String::from(
        "<table><thead><tr><th>Report</th><th>Visible to</th><th>Links</th><th>Share</th></tr></thead><tbody>",
    );
    for row in rows {
        let Some(slug) = row["slug"].as_str() else {
            continue;
        };
        let rights = state.access.rights(&caller.principal, slug);
        if !rights.contains(&Right::View) {
            continue;
        }
        shown += 1;
        let acl = state.access.acl(slug);
        let title = row["frontmatter"]["title"].as_str().unwrap_or(slug);
        let version = row["version"].as_str().unwrap_or("");
        let slug_e = escape(slug);
        let versions = state.storage.list_versions(slug).unwrap_or_default();
        let version_links: String = versions
            .iter()
            .map(|v| format!("<a href=\"/r/{slug_e}/v/{0}\">{0}</a> ", escape(v)))
            .collect();
        let visibility = match acl.visibility {
            Visibility::Private => "private",
            Visibility::Org => "everyone here",
            Visibility::Public => "anyone with the link",
        };
        let share = if rights.contains(&Right::Share) {
            let option = |value: &str, label: &str| {
                format!(
                    "<option value=\"{value}\"{}>{label}</option>",
                    if visibility_name(acl.visibility) == value {
                        " selected"
                    } else {
                        ""
                    }
                )
            };
            format!(
                "<form method=\"post\" action=\"/ui/share/{slug_e}\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><select name=\"visibility\" aria-label=\"Who can see {slug_e}\">{}{}{}</select> <button type=\"submit\">Save</button></form>",
                escape(&csrf),
                option("private", "Only me and people I add"),
                option("org", "Everyone signed in here"),
                option("public", "Anyone with the link"),
            )
        } else {
            String::new()
        };
        table.push_str(&format!(
            "<tr><td data-label=\"Report\"><a href=\"/r/{slug_e}\">{}</a><br><code>{slug_e}</code> <span class=\"badge\">{}</span></td>\
<td data-label=\"Visible to\">{}{}</td><td data-label=\"Links\"><input class=\"url\" readonly value=\"{}\" aria-label=\"Link to {slug_e}\"><br>\
<a href=\"/raw/{slug_e}\">Source</a> · <a href=\"/r/{slug_e}/download\">Download</a><br>Versions: {version_links}</td><td data-label=\"Share\">{share}</td></tr>",
            escape(title),
            escape(version),
            visibility,
            if acl.visibility == Visibility::Private { " <span class=\"badge\">private</span>" } else { "" },
            escape(&absolute(&state, &headers, &format!("/r/{slug}"))),
        ));
    }
    table.push_str("</tbody></table>");
    if shown == 0 {
        body.push_str("<p>No reports are shared with you yet.</p>");
    } else {
        body.push_str(&table);
    }
    page("Reports", &body)
}

/// The hosted report list: what the signed-in person owns, what their current organization
/// shows them and what was shared with them by name. It is built from the access state, never
/// from a scan of other organizations' folders.
async fn hosted_index(
    state: &AppState,
    headers: &HeaderMap,
    caller: &Caller,
    notice: &Notice,
) -> Response {
    use crate::access::Relation;
    if session_csrf(state, caller).is_none() {
        return hosted_page(
            "Sign in",
            &hosted_pages::sign_in(&notice.m, safe_next(&notice.next)),
        );
    }
    let hosted = match hosted_session(state, caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    if hosted.org.as_ref().is_some_and(|org| org.suspended) {
        return hosted_page(
            "Account unavailable",
            &hosted_pages::suspended(&hosted.nav()),
        );
    }
    let (Some(server), Some(origin)) = (
        state.public_origin.as_deref(),
        state.report_origin.as_deref(),
    ) else {
        return hosted_error_page(StatusCode::INTERNAL_SERVER_ERROR);
    };
    // Reports shared with this person's address become theirs to open once they are signed in.
    state
        .access
        .claim_shares(|| auth::caller(state, headers).principal);
    let current = hosted.org.as_ref().map(|org| org.id.as_str());
    let visible: Vec<_> = state
        .access
        .visible_reports(&caller.principal)
        .into_iter()
        // The list works in one organization at a time. A report of another of the person's
        // organizations shows here only when it was shared with them by name.
        .filter_map(|mut report| {
            if Some(report.org.as_str()) != current {
                if !report.named && report.relation != Relation::Shared {
                    return None;
                }
                report.relation = Relation::Shared;
            }
            Some(report)
        })
        .filter(|report| {
            state
                .access
                .can(&caller.principal, &report.key, Right::View)
        })
        .collect();
    let storage = state.storage.clone();
    let keys: Vec<String> = visible.iter().map(|report| report.key.clone()).collect();
    let Ok(Ok(summaries)) = tokio::task::spawn_blocking(move || {
        keys.iter()
            .map(|key| storage.summary_with_size(key))
            .collect::<Result<Vec<_>, _>>()
    })
    .await
    else {
        return hosted_error_page(StatusCode::INTERNAL_SERVER_ERROR);
    };
    struct Row {
        key: String,
        title: String,
        description: String,
        owner: String,
        kind: String,
        badge: String,
        who: String,
        size: Option<u64>,
        can_share: bool,
        stamp: String,
        updated: String,
        version: String,
        stored_version: String,
        url: String,
        relation: Relation,
    }
    // A retired address keeps its access record and has no report behind it.
    let mut rows: Vec<Row> = visible
        .into_iter()
        .zip(summaries)
        .filter_map(|(report, summary)| {
            let (summary, size) = summary?;
            let can_share = state
                .access
                .can(&caller.principal, &report.key, Right::Share);
            let kind = if !can_share && report.relation == Relation::Shared {
                "you"
            } else {
                match report.visibility {
                    Visibility::Private => "private",
                    Visibility::Org => "org",
                    Visibility::Public => "public",
                }
            };
            let badge = match kind {
                "you" => "Shared with you".to_owned(),
                "private" => "Private".to_owned(),
                "org" => report.org_name.clone(),
                _ => "Public link".to_owned(),
            };
            let who = if can_share {
                match report.visibility {
                    Visibility::Org => format!("Every member of {}", report.org_name),
                    Visibility::Public => "Anyone with the link".to_owned(),
                    Visibility::Private => {
                        let named = state
                            .access
                            .report_access(&report.key)
                            .map(|access| access.named)
                            .unwrap_or_default();
                        if let Some(first) = named.first() {
                            let person = first.name.as_deref().unwrap_or(&first.email);
                            match named.len() {
                                1 => format!("Shared with {person}"),
                                n => format!("Shared with {person} and {} more", n - 1),
                            }
                        } else {
                            "Not shared with anyone".to_owned()
                        }
                    }
                }
            } else {
                let owner = if report.owner_name.is_empty() {
                    "A former member"
                } else {
                    &report.owner_name
                };
                if hosted_pages::same_name(owner, &report.org_name) {
                    format!("By {owner}")
                } else {
                    format!("By {owner} · {}", report.org_name)
                }
            };
            let slug = report.key.split_once('/').map_or("", |(_, slug)| slug);
            let version = summary["version"].as_str().unwrap_or("");
            let stamp = summary["updatedAt"].as_str().unwrap_or("").to_owned();
            Some(Row {
                title: summary["frontmatter"]["title"]
                    .as_str()
                    .unwrap_or(slug)
                    .to_owned(),
                description: summary["frontmatter"]["description"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
                owner: owner_label(report.relation, &report.owner_name),
                kind: kind.to_owned(),
                badge,
                who,
                size,
                can_share,
                updated: report_date(&stamp),
                stamp,
                version: if version.is_empty() || version.starts_with('v') {
                    version.to_owned()
                } else {
                    format!("v{version}")
                },
                stored_version: version.to_owned(),
                url: format!("{}/r/{}", origin.trim_end_matches('/'), report.key),
                relation: report.relation,
                key: report.key,
            })
        })
        .collect();
    let total = rows.len();
    let show = match notice.show.as_str() {
        "mine" | "shared" | "org" => notice.show.as_str(),
        _ => "all",
    };
    let query: String = notice.q.trim().chars().take(120).collect();
    let needle = query.to_lowercase();
    rows.retain(|row| {
        (match show {
            "mine" => row.relation == Relation::Mine,
            "shared" => row.relation == Relation::Shared,
            "org" => row.relation != Relation::Shared,
            _ => true,
        }) && (needle.is_empty() || row.title.to_lowercase().contains(&needle))
    });
    rows.sort_by(|a, b| b.stamp.cmp(&a.stamp).then_with(|| a.key.cmp(&b.key)));
    let storage = state.storage.clone();
    let wanted: Vec<(String, String)> = rows
        .iter()
        .map(|row| (row.key.clone(), row.stored_version.clone()))
        .collect();
    let thumbs = tokio::task::spawn_blocking(move || thumbnails(&storage, &wanted, THUMB_WAIT))
        .await
        .unwrap_or_default();
    let reports: Vec<_> = rows
        .iter()
        .zip(
            thumbs
                .iter()
                .map(|thumb| thumb.as_deref())
                .chain(std::iter::repeat(None)),
        )
        .map(|(row, thumb)| hosted_pages::Report {
            key: &row.key,
            title: &row.title,
            description: &row.description,
            owner: &row.owner,
            kind: &row.kind,
            badge: &row.badge,
            who: &row.who,
            size: row.size,
            can_share: row.can_share,
            updated: &row.updated,
            version: &row.version,
            url: &row.url,
            thumb,
        })
        .collect();
    let invitations = state.access.invitations_for(&caller.principal);
    let invites: Vec<_> = invitations
        .iter()
        .filter_map(|view| {
            Some(hosted_pages::Invite {
                id: &view.invitation.id,
                organization: &view.org_name,
                inviter: &view.inviter_name,
                role: super::mail_texts::role_phrase(view.invitation.role?),
            })
        })
        .collect();
    let can_publish = matches!(
        hosted.role,
        Some(OrgRole::Owner | OrgRole::Admin | OrgRole::Author)
    );
    let usage = if can_publish {
        if let Some(org) = &hosted.org {
            super::hosted::account_usage(state, &caller.principal, &org.id)
                .await
                .ok()
                .map(|usage| hosted_pages::AccountUsage {
                    reports: usage.reports,
                    report_limit: usage.report_limit,
                    bytes: usage.bytes,
                    byte_limit: usage.byte_limit,
                })
        } else {
            None
        }
    } else {
        None
    };
    let nav = hosted.nav();
    let body = hosted_pages::reports(&hosted_pages::ReportList {
        nav: &nav,
        server,
        rows: &reports,
        total,
        show,
        query: &query,
        notice: &notice.m,
        invites: &invites,
        can_publish,
        usage: usage.as_ref(),
    });
    hosted_page("Reports", &body)
}

/// The width in pixels of a report's small card in the list: twice the 60 CSS pixels it is shown at.
const THUMB_WIDTH: u32 = 120;
/// The most small cards the server keeps; past it the oldest go first.
const THUMB_CACHE: usize = 1024;
/// How long a list waits for small cards not drawn before. The rest show the placeholder and are
/// drawn in the background, for the next view, so a list never waits on a heavy report.
const THUMB_WAIT: std::time::Duration = std::time::Duration::from_millis(200);
/// The most threads drawing small cards at once, over every list.
const THUMB_WORKERS: usize = 4;

type ThumbKey = (String, String, u32);

/// Small cards by report, version and card design ([`richview_share::CARD_DESIGN`]). A version's
/// card in one design never changes, so an entry stays right until it is pushed out; `None`
/// remembers a report whose card cannot be drawn. `drawing` holds the cards a worker has taken,
/// so two lists never draw the same card.
struct Thumbs {
    cards: super::reports::Recent<ThumbKey, Option<std::sync::Arc<str>>>,
    drawing: std::collections::HashSet<ThumbKey>,
    workers: usize,
}

fn thumbs() -> &'static std::sync::Mutex<Thumbs> {
    static THUMBS: OnceLock<std::sync::Mutex<Thumbs>> = OnceLock::new();
    THUMBS.get_or_init(|| {
        std::sync::Mutex::new(Thumbs {
            cards: super::reports::Recent::new(THUMB_CACHE),
            drawing: std::collections::HashSet::new(),
            workers: 0,
        })
    })
}

fn lock_thumbs() -> std::sync::MutexGuard<'static, Thumbs> {
    thumbs().lock().unwrap_or_else(|e| e.into_inner())
}

/// The list's small cards as `data:` URLs, in the order of `wanted` (report key, stored version):
/// the list page allows images only from `data:`, and it lists only reports the person may open.
/// Each is drawn from the stored report in today's card design rather than shrunk from the card
/// kept at publication, so one list never mixes cards drawn before a redesign with cards drawn
/// after it, and nothing is written into the store. Cards not seen before are drawn on a few
/// threads for at most `wait`; a card not ready by then is `None` this time.
fn thumbnails(
    storage: &crate::storage::FsStorage,
    wanted: &[(String, String)],
    wait: std::time::Duration,
) -> Vec<Option<std::sync::Arc<str>>> {
    let deadline = std::time::Instant::now() + wait;
    let key = |n: usize| -> ThumbKey {
        (
            wanted[n].0.clone(),
            wanted[n].1.clone(),
            richview_share::CARD_DESIGN,
        )
    };
    let (mut found, queue, spawn) = {
        let mut state = lock_thumbs();
        let found: Vec<Option<Option<std::sync::Arc<str>>>> = (0..wanted.len())
            .map(|n| state.cards.get(&key(n)))
            .collect();
        let mut queue = std::collections::VecDeque::new();
        for n in (0..wanted.len()).filter(|n| found[*n].is_none()) {
            if state.drawing.insert(key(n)) {
                queue.push_back(key(n));
            }
        }
        let spawn = THUMB_WORKERS.saturating_sub(state.workers).min(queue.len());
        if spawn == 0 {
            // No worker is free: the cards stay undrawn, for a later view to take.
            for item in &queue {
                state.drawing.remove(item);
            }
            queue.clear();
        }
        state.workers += spawn;
        (found, queue, spawn)
    };
    let (sender, receiver) = std::sync::mpsc::channel::<(ThumbKey, Option<std::sync::Arc<str>>)>();
    let queue = std::sync::Arc::new(std::sync::Mutex::new(queue));
    for _ in 0..spawn {
        let (queue, sender, storage) = (queue.clone(), sender.clone(), storage.clone());
        std::thread::spawn(move || {
            loop {
                let next = queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
                let Some(item) = next else { break };
                // A report that cannot be drawn, even by a panic, gets no card and frees its worker.
                let url = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    small_card(&storage, &item.0, &item.1)
                }))
                .unwrap_or(None);
                {
                    let mut state = lock_thumbs();
                    state.drawing.remove(&item);
                    state.cards.put(item.clone(), url.clone());
                }
                // The list may have stopped waiting; the card is kept for the next view.
                let _ = sender.send((item, url));
            }
            lock_thumbs().workers -= 1;
        });
    }
    drop(sender);
    let mut waiting = found.iter().filter(|card| card.is_none()).count();
    while waiting > 0 {
        let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) else {
            break;
        };
        match receiver.recv_timeout(left) {
            Ok((item, url)) => {
                for (n, card) in found.iter_mut().enumerate() {
                    if card.is_none() && key(n) == item {
                        *card = Some(url.clone());
                        waiting -= 1;
                    }
                }
            }
            Err(_) => break,
        }
    }
    // A card another list's worker drew meanwhile counts too.
    if waiting > 0 {
        let state = lock_thumbs();
        for (n, card) in found.iter_mut().enumerate() {
            if card.is_none() {
                *card = state.cards.get(&key(n));
            }
        }
    }
    found.into_iter().map(Option::flatten).collect()
}

/// One listed version's small card as a `data:` URL, drawn from its stored report: over its first
/// chart when the report renders, else its title card, as its link card is drawn.
fn small_card(
    storage: &crate::storage::FsStorage,
    key: &str,
    version: &str,
) -> Option<std::sync::Arc<str>> {
    let report = storage.get(key, Some(version)).ok().flatten()?;
    let svg = super::reports::share_card_svg(&report, &super::reports::charts_of(&report))
        .or_else(|_| super::reports::share_card_svg(&report, &[]))
        .ok()?;
    let png = richview_share::card_png_small(&svg, THUMB_WIDTH).ok()?;
    Some(std::sync::Arc::from(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )))
}

fn owner_label(relation: crate::access::Relation, owner: &str) -> String {
    match (relation, owner) {
        (crate::access::Relation::Mine, _) => "You".to_owned(),
        (_, "") => "A former member".to_owned(),
        (_, name) => name.to_owned(),
    }
}

fn visibility_name(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "private",
        Visibility::Org => "org",
        Visibility::Public => "public",
    }
}

fn absolute(state: &AppState, headers: &HeaderMap, path: &str) -> String {
    let origin = state.public_origin.clone().unwrap_or_else(|| {
        let host = headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .filter(|h| {
                h.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".:-[]".contains(&b))
            })
            .unwrap_or("localhost");
        format!("http://{host}")
    });
    format!("{}{path}", origin.trim_end_matches('/'))
}

#[derive(Deserialize)]
pub struct SignInForm {
    email: String,
    password: String,
    #[serde(default)]
    next: String,
}

/// `POST /ui/sign-in`
pub async fn sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<SignInForm>,
) -> Response {
    if !auth::same_origin(&headers) {
        return api_error(StatusCode::FORBIDDEN, "Cross-site sign-in is refused.");
    }
    let result = auth::sign_in_bounded(state.access.clone(), &form.email, &form.password).await;
    // A failed attempt returns to the form with the same destination.
    let back = safe_next(&form.next).map_or("/".to_owned(), |next| {
        format!("/?next={}", encode_value(next))
    });
    match result {
        Ok((cookie, _csrf)) => {
            let principal = state.access.authenticate(None, Some(&cookie));
            let _ = state.access.audit(&principal, "sign-in", "", true);
            let _ = headers;
            // A hosted report is on another origin, where a browser does not follow a form
            // under `form-action 'self'`: return to the report's page here instead.
            let hosted_report = safe_next(&form.next)
                .filter(|_| state.access.is_hosted())
                .and_then(|next| next.strip_prefix("/r/"))
                .and_then(|rest| {
                    let mut parts = rest.split(['/', '?']);
                    Some(format!("/reports/{}/{}", parts.next()?, parts.next()?))
                });
            let mut response = redirect(
                hosted_report
                    .as_deref()
                    .or(safe_next(&form.next))
                    .unwrap_or("/"),
                None,
            );
            response.headers_mut().insert(
                header::SET_COOKIE,
                auth::session_cookie(&state, &cookie, 12 * 3600)
                    .parse()
                    .unwrap(),
            );
            response
        }
        Err(error) => {
            let _ = state.access.audit(
                &Principal::Anonymous,
                "sign-in",
                &form.email.trim().to_ascii_lowercase(),
                false,
            );
            match error {
                crate::access::AccessError::Credentials | crate::access::AccessError::Throttled
                    if state.access.is_hosted() =>
                {
                    redirect(
                        &back,
                        Some("Could not sign in. Check your details or try again later."),
                    )
                }
                crate::access::AccessError::Credentials => {
                    redirect(&back, Some("Wrong email or password."))
                }
                crate::access::AccessError::Throttled => redirect(
                    &back,
                    Some("Too many failed sign-ins. Try again in 15 minutes."),
                ),
                other => access_error(other),
            }
        }
    }
}

#[derive(Deserialize)]
pub struct CsrfForm {
    csrf: String,
}

/// `POST /ui/sign-out`
pub async fn sign_out(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Some(session) = &caller.session {
        let _ = state
            .access
            .sign_out(|| auth::caller(&state, &headers).principal, session);
    }
    audit(&state, &caller, "sign-out", "");
    let mut response = redirect("/", None);
    response.headers_mut().insert(
        header::SET_COOKIE,
        auth::session_cookie(&state, "", 0).parse().unwrap(),
    );
    response
}

#[derive(Deserialize)]
pub struct ShareForm {
    csrf: String,
    visibility: String,
}

/// `POST /ui/share/{slug}`: who can see the report. Grants for named people stay as they are.
pub async fn share(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    Form(form): Form<ShareForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Err(response) = auth::authorize(&state, &caller, &slug, Right::Share) {
        return response;
    }
    let Some(visibility) = Visibility::parse(&form.visibility) else {
        return api_error(StatusCode::BAD_REQUEST, "Choose private, org or public.");
    };
    let grants = state.access.acl(&slug).grants;
    match state.access.set_acl_authorized(
        || auth::caller(&state, &headers).principal,
        &slug,
        visibility,
        grants,
    ) {
        Ok(_) => {
            audit(&state, &caller, "report.share", &slug);
            redirect(
                "/",
                Some(&format!("{slug} is now {}.", visibility_name(visibility))),
            )
        }
        Err(error) => access_error(error),
    }
}

/// Change hosted visibility only after validating and authorizing the full organization key.
pub async fn share_hosted(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, slug)): AxumPath<(String, String)>,
    Form(form): Form<ShareForm>,
) -> Response {
    let key = format!("{org}/{slug}");
    if state.storage.validate_key(&key).is_err() {
        return auth::not_found();
    }
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Err(response) = hosted_session(&state, &caller) {
        return response;
    }
    let Some(visibility) = Visibility::parse(&form.visibility) else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "Choose private, organization or public link.",
        );
    };
    match state.access.set_visibility_authorized(
        || auth::caller(&state, &headers).principal,
        &key,
        visibility,
    ) {
        Ok(acl) => {
            audit(&state, &caller, "report.share", &key);
            let organization = state
                .access
                .organization(&acl.org)
                .map(|org| org.name)
                .unwrap_or_default();
            redirect(
                &format!("/reports/{key}"),
                Some(&match visibility {
                    Visibility::Private => {
                        "The report is private: only the owner and the people named here can open it."
                            .to_owned()
                    }
                    Visibility::Org => {
                        format!("Every member of {organization} can open the report.")
                    }
                    Visibility::Public => "Anyone with the link can open the report.".to_owned(),
                }),
            )
        }
        Err(error) => access_error(error),
    }
}

/// Show the final confirmation only to someone who may delete this report.
pub async fn delete_confirmation_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, slug)): AxumPath<(String, String)>,
) -> Response {
    let key = format!("{org}/{slug}");
    if state.storage.validate_key(&key).is_err() {
        return auth::not_found();
    }
    let caller = auth::caller(&state, &headers);
    let hosted = match hosted_session(&state, &caller) {
        Ok(hosted) => hosted,
        Err(response) => return response,
    };
    if let Err(response) = auth::authorize(&state, &caller, &key, Right::Delete) {
        return response;
    }
    let storage = state.storage.clone();
    let lookup_key = key.clone();
    let report = match tokio::task::spawn_blocking(move || storage.summary(&lookup_key, None)).await
    {
        Ok(Ok(Some(report))) => report,
        Ok(Ok(None)) => return auth::not_found(),
        _ => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not read the report.",
            );
        }
    };
    let title = report["frontmatter"]["title"].as_str().unwrap_or(&slug);
    hosted_page(
        "Delete report",
        &hosted_pages::delete_confirmation(&hosted.nav(), &key, title),
    )
}

/// Delete every version only after the confirmation form passes the CSRF and delete-right checks.
pub async fn delete_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((org, slug)): AxumPath<(String, String)>,
    Form(form): Form<CsrfForm>,
) -> Response {
    let key = format!("{org}/{slug}");
    if state.storage.validate_key(&key).is_err() {
        return auth::not_found();
    }
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Err(response) = hosted_session(&state, &caller) {
        return response;
    }
    if let Err(response) = auth::authorize(&state, &caller, &key, Right::Delete) {
        return response;
    }
    // Read the title only once the caller may delete the report, to name it in the notice.
    let title = state
        .storage
        .summary(&key, None)
        .ok()
        .flatten()
        .and_then(|row| row["frontmatter"]["title"].as_str().map(str::to_owned));
    match super::reports::delete_owned_report(&state, &headers, &key).await {
        Ok(()) => redirect(
            "/",
            Some(&title.map_or_else(
                || "Report deleted.".to_owned(),
                |title| format!("Deleted “{title}”."),
            )),
        ),
        Err(response) => response,
    }
}

/// `GET /governance`: people, roles, tokens and the audit trail. Admins only.
pub async fn governance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(notice): Query<Notice>,
) -> Response {
    let caller = match auth::admin(&state, &headers, false) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let csrf = session_csrf(&state, &caller).unwrap_or_default();
    let hidden = format!(
        "<input type=\"hidden\" name=\"csrf\" value=\"{}\">",
        escape(&csrf)
    );
    let mut body = format!(
        "<div class=\"bar\"><h1>People and roles</h1><a href=\"/\">Reports</a></div>{}",
        notice_html(&notice)
    );
    body.push_str(&format!(
        "<h2>Add a person</h2><form method=\"post\" action=\"/ui/governance/users\">{hidden}\
<p><label>Name <input name=\"name\" required></label> <label>Email <input type=\"email\" name=\"email\" required></label></p>\
<p><label>Role <select name=\"role\"><option value=\"reader\">Reader: reads what is shared</option><option value=\"author\">Author: publishes reports</option><option value=\"admin\">Admin: everything</option></select></label> \
<label>Password (12+ characters) <input type=\"password\" name=\"password\" minlength=\"12\" autocomplete=\"new-password\"></label> <button type=\"submit\">Add</button></p></form>"
    ));
    body.push_str("<h2>People</h2><table><thead><tr><th>Person</th><th>Role</th><th>Standing</th><th>Password and tokens</th></tr></thead><tbody>");
    for user in state.access.users() {
        let id = escape(&user.id);
        let option = |value: &str, role: OrgRole| {
            format!(
                "<option value=\"{value}\"{}>{value}</option>",
                if user.role == role { " selected" } else { "" }
            )
        };
        let tokens: String = state
            .access
            .tokens(&user.id)
            .iter()
            .map(|t| {
                format!(
                    "<li>{} <form class=\"inline\" method=\"post\" action=\"/ui/governance/users/{id}/tokens/{}/revoke\">{hidden}<button type=\"submit\">Revoke</button></form></li>",
                    escape(t["name"].as_str().unwrap_or("token")),
                    escape(t["id"].as_str().unwrap_or(""))
                )
            })
            .collect();
        body.push_str(&format!(
            "<tr><td data-label=\"Person\">{}<br><code>{}</code></td>\
<td data-label=\"Role\"><form method=\"post\" action=\"/ui/governance/users/{id}\">{hidden}<select name=\"role\" aria-label=\"Role of {}\">{}{}{}</select> <button type=\"submit\" name=\"action\" value=\"role\">Save</button></form></td>\
<td data-label=\"Standing\">{} <form class=\"inline\" method=\"post\" action=\"/ui/governance/users/{id}\">{hidden}<button type=\"submit\" name=\"action\" value=\"{}\">{}</button></form> \
<form class=\"inline\" method=\"post\" action=\"/ui/governance/users/{id}/delete\">{hidden}<button type=\"submit\">Remove</button></form></td>\
<td data-label=\"Password and tokens\"><form method=\"post\" action=\"/ui/governance/users/{id}/password\">{hidden}<input type=\"password\" name=\"password\" minlength=\"12\" required autocomplete=\"new-password\" aria-label=\"New password for {}\"> <button type=\"submit\">Set</button></form>\
<ul>{tokens}</ul><form method=\"post\" action=\"/ui/governance/users/{id}/tokens\">{hidden}<input name=\"name\" placeholder=\"Token name\" aria-label=\"Token name for {}\"> <button type=\"submit\">New token</button></form></td></tr>",
            escape(&user.name),
            escape(&user.email),
            escape(&user.name),
            option("reader", OrgRole::Reader),
            option("author", OrgRole::Author),
            option("admin", OrgRole::Admin),
            if user.disabled { "disabled" } else { "active" },
            if user.disabled { "enable" } else { "disable" },
            if user.disabled { "Enable" } else { "Disable" },
            escape(&user.name),
            escape(&user.name),
        ));
    }
    body.push_str("</tbody></table><h2>Audit trail</h2><table><thead><tr><th>When</th><th>Who</th><th>What</th><th>On</th></tr></thead><tbody>");
    for line in state.access.read_audit(50) {
        let text = |key: &str| escape(line[key].as_str().unwrap_or(""));
        body.push_str(&format!(
            "<tr><td data-label=\"When\">{}</td><td data-label=\"Who\">{}</td><td data-label=\"What\">{}{}</td><td data-label=\"On\">{}</td></tr>",
            text("time"),
            text("actor"),
            text("action"),
            if line["allowed"] == Value::Bool(false) {
                " (refused)"
            } else {
                ""
            },
            text("target"),
        ));
    }
    body.push_str("</tbody></table>");
    page("Governance", &body)
}

#[derive(Deserialize)]
pub struct NewUserForm {
    csrf: String,
    name: String,
    email: String,
    role: OrgRole,
    #[serde(default)]
    password: String,
}

/// `POST /ui/governance/users`
pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<NewUserForm>,
) -> Response {
    let caller = match admin_form(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let access = state.access.clone();
    let password = (!form.password.is_empty()).then_some(form.password);
    let created = tokio::task::spawn_blocking(move || {
        access.create_user(&form.name, &form.email, form.role, password.as_deref())
    })
    .await;
    match created {
        Ok(Ok(user)) => {
            audit(&state, &caller, "user.create", &user.id);
            redirect("/governance", Some(&format!("Added {}.", user.name)))
        }
        Ok(Err(crate::access::AccessError::Invalid(message))) => {
            redirect("/governance", Some(&message))
        }
        Ok(Err(error)) => access_error(error),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Worker failed"),
    }
}

#[derive(Deserialize)]
pub struct UserForm {
    csrf: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    role: Option<OrgRole>,
}

/// `POST /ui/governance/users/{id}`: change the role, or disable or enable the person.
pub async fn update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Form(form): Form<UserForm>,
) -> Response {
    let caller = match admin_form(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let (role, disabled) = match form.action.as_str() {
        "role" => (form.role, None),
        "disable" => (None, Some(true)),
        "enable" => (None, Some(false)),
        _ => return api_error(StatusCode::BAD_REQUEST, "Unknown change."),
    };
    match state.access.update_user(&id, None, role, disabled) {
        Ok(user) => {
            audit(&state, &caller, "user.update", &id);
            redirect("/governance", Some(&format!("Saved {}.", user.name)))
        }
        Err(crate::access::AccessError::Invalid(message)) => {
            redirect("/governance", Some(&message))
        }
        Err(error) => access_error(error),
    }
}

#[derive(Deserialize)]
pub struct PasswordForm {
    csrf: String,
    password: String,
}

/// `POST /ui/governance/users/{id}/password`
pub async fn set_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Form(form): Form<PasswordForm>,
) -> Response {
    let caller = match admin_form(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let access = state.access.clone();
    let target = id.clone();
    let result =
        tokio::task::spawn_blocking(move || access.set_password(&target, &form.password)).await;
    match result {
        Ok(Ok(())) => {
            audit(&state, &caller, "user.password", &id);
            redirect(
                "/governance",
                Some("Password set; the person was signed out."),
            )
        }
        Ok(Err(crate::access::AccessError::Invalid(message))) => {
            redirect("/governance", Some(&message))
        }
        Ok(Err(error)) => access_error(error),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Worker failed"),
    }
}

#[derive(Deserialize)]
pub struct TokenForm {
    csrf: String,
    #[serde(default)]
    name: String,
}

/// `POST /ui/governance/users/{id}/tokens`: shows the new token once.
pub async fn create_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Form(form): Form<TokenForm>,
) -> Response {
    let caller = match admin_form(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    match state.access.create_token(&id, &form.name) {
        Ok((token_id, secret)) => {
            audit(&state, &caller, "token.create", &format!("{id}/{token_id}"));
            page(
                "New token",
                &format!(
                    "<h1>New token</h1><p class=\"note\">Copy it now. It is shown once and stored only as a digest.</p><p><code>{}</code></p><p><a href=\"/governance\">Back to people</a></p>",
                    escape(&secret)
                ),
            )
        }
        Err(error) => access_error(error),
    }
}

/// `POST /ui/governance/users/{id}/tokens/{token}/revoke`
pub async fn revoke_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((id, token)): AxumPath<(String, String)>,
    Form(form): Form<CsrfForm>,
) -> Response {
    let caller = match admin_form(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    match state.access.revoke_single_server_token(&id, &token) {
        Ok(()) => {
            audit(&state, &caller, "token.revoke", &format!("{id}/{token}"));
            redirect("/governance", Some("Token revoked."))
        }
        Err(error) => access_error(error),
    }
}

/// `POST /ui/governance/users/{id}/delete`
pub async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Form(form): Form<CsrfForm>,
) -> Response {
    let caller = match admin_form(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    match state.access.delete_user(&id) {
        Ok(()) => {
            audit(&state, &caller, "user.delete", &id);
            redirect("/governance", Some("Person removed."))
        }
        Err(crate::access::AccessError::Invalid(message)) => {
            redirect("/governance", Some(&message))
        }
        Err(error) => access_error(error),
    }
}

/// The signed-in person of a hosted page and the organization their session works in.
struct Hosted<'a> {
    person: &'a User,
    csrf: String,
    /// The current organization; `None` for a person who belongs to none.
    org: Option<crate::access::Organization>,
    role: Option<OrgRole>,
    /// Every organization the person belongs to: code and name.
    orgs: Vec<(String, String)>,
}

impl Hosted<'_> {
    fn nav(&self) -> hosted_pages::Nav<'_> {
        hosted_pages::Nav {
            name: &self.person.name,
            email: &self.person.email,
            csrf: &self.csrf,
            org: self
                .org
                .as_ref()
                .map(|org| (org.id.as_str(), org.name.as_str())),
            role: self.role.map(|role| {
                hosted_pages::role_label(match role {
                    OrgRole::Owner => "owner",
                    OrgRole::Admin => "admin",
                    OrgRole::Author => "author",
                    OrgRole::Reader => "reader",
                })
            }),
            orgs: &self.orgs,
        }
    }
}

/// Resolve a hosted account from the session, never from form data or a bearer token.
fn hosted_session<'a>(state: &AppState, caller: &'a Caller) -> Result<Hosted<'a>, Response> {
    let csrf = session_csrf(state, caller).ok_or_else(sign_in_first)?;
    let person = user_of(&caller.principal).ok_or_else(sign_in_first)?;
    let session = caller.session.as_deref().ok_or_else(sign_in_first)?;
    let organizations = state.access.organizations_for(&person.id);
    let org = state
        .access
        .current_organization(session)
        .and_then(|id| organizations.iter().find(|org| org.id == id).cloned());
    let role = org
        .as_ref()
        .and_then(|org| state.access.role_in(&person.id, &org.id));
    Ok(Hosted {
        person,
        csrf,
        org,
        role,
        orgs: organizations
            .into_iter()
            .map(|org| (org.id, org.name))
            .collect(),
    })
}

/// The same, for a page that works inside an organization: a person without one is sent to the
/// list, which says how to get one, and a suspended organization shows its notice.
fn hosted_member<'a>(
    state: &AppState,
    caller: &'a Caller,
) -> Result<(Hosted<'a>, String), Response> {
    let hosted = hosted_session(state, caller)?;
    let Some(org) = &hosted.org else {
        return Err(redirect("/", Some("Create or join an organization first.")));
    };
    if org.suspended {
        return Err(hosted_page(
            "Account unavailable",
            &hosted_pages::suspended(&hosted.nav()),
        ));
    }
    let id = org.id.clone();
    Ok((hosted, id))
}

fn hosted_token_error(error: crate::access::AccessError) -> Response {
    use crate::access::AccessError;
    let (status, title, message) = match error {
        AccessError::Invalid(message) => (StatusCode::BAD_REQUEST, "Token request failed", message),
        AccessError::NotFound => (
            StatusCode::NOT_FOUND,
            "Token unavailable",
            "This token is not available.".to_owned(),
        ),
        AccessError::Throttled => (
            StatusCode::TOO_MANY_REQUESTS,
            "Try again shortly",
            "Please try again shortly.".to_owned(),
        ),
        AccessError::Credentials => (
            StatusCode::UNAUTHORIZED,
            "Access unavailable",
            "Please sign in again.".to_owned(),
        ),
        AccessError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong",
            "The request could not be completed.".to_owned(),
        ),
    };
    let mut response = hosted_page(
        title,
        &format!(
            "<div class=\"cowork-text\"><h1>{}</h1><p>{}</p><p><a href=\"/tokens\">Back to CLI tokens</a> · <a href=\"/\">Sign in or go to your reports</a></p></div>",
            escape(title),
            escape(&message)
        ),
    );
    *response.status_mut() = status;
    response
}

fn token_page(state: &AppState, hosted: &Hosted<'_>, org: &str, secret: Option<&str>) -> Response {
    let Some(server) = state.public_origin.as_deref() else {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The server address is not configured.",
        );
    };
    let rows = state.access.tokens_for_org(&hosted.person.id, org);
    let dates: Vec<_> = rows
        .iter()
        .map(|row| token_date(row["created"].as_str().unwrap_or("")))
        .collect();
    let tokens: Vec<_> = rows
        .iter()
        .zip(&dates)
        .map(|(row, created)| hosted_pages::Token {
            id: row["id"].as_str().unwrap_or(""),
            name: row["name"].as_str().unwrap_or(""),
            created,
        })
        .collect();
    hosted_page(
        "CLI tokens",
        &hosted_pages::tokens(&hosted.nav(), server, &tokens, secret),
    )
}

pub async fn personal_tokens(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let caller = auth::caller(&state, &headers);
    let (hosted, org) = match hosted_member(&state, &caller) {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    token_page(&state, &hosted, &org, None)
}

#[derive(Deserialize)]
pub struct PersonalTokenForm {
    csrf: String,
    name: String,
}

pub async fn create_personal_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PersonalTokenForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let (hosted, org) = match hosted_member(&state, &caller) {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    if form.name.trim().is_empty() {
        return hosted_token_error(crate::access::AccessError::Invalid(
            "Enter a token name.".into(),
        ));
    }
    let person = hosted.person;
    match state.access.create_self_token(
        || auth::caller(&state, &headers).principal,
        &person.id,
        &org,
        &form.name,
    ) {
        Ok((id, secret)) => {
            audit(
                &state,
                &caller,
                "token.create",
                &format!("{}/{id}", person.id),
            );
            token_page(&state, &hosted, &org, Some(&secret))
        }
        Err(error) => hosted_token_error(error),
    }
}

pub async fn revoke_personal_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(token): AxumPath<String>,
    Form(form): Form<CsrfForm>,
) -> Response {
    let caller = match form_caller(&state, &headers, &form.csrf) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let hosted = match hosted_session(&state, &caller) {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let person = hosted.person;
    match state.access.revoke_self_token(
        || auth::caller(&state, &headers).principal,
        &person.id,
        &token,
    ) {
        Ok(()) => {
            audit(
                &state,
                &caller,
                "token.revoke",
                &format!("{}/{token}", person.id),
            );
            redirect("/tokens", None)
        }
        Err(error) => hosted_token_error(error),
    }
}

pub async fn account_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let caller = auth::caller(&state, &headers);
    let hosted = match hosted_session(&state, &caller) {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let usage = match &hosted.org {
        Some(org) if !org.suspended => {
            super::hosted::account_usage(&state, &caller.principal, &org.id)
                .await
                .ok()
                .map(|usage| hosted_pages::AccountUsage {
                    reports: usage.reports,
                    report_limit: usage.report_limit,
                    bytes: usage.bytes,
                    byte_limit: usage.byte_limit,
                })
        }
        _ => None,
    };
    hosted_page(
        "Account",
        &hosted_pages::account(&hosted.nav(), &hosted.person.email, usage.as_ref()),
    )
}

#[cfg(test)]
mod hosted_response_tests {
    use super::*;

    #[test]
    fn hosted_responses_do_not_cache_or_send_referrers() {
        for response in [hosted_page("Setup", ""), expired_link_page()] {
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
            assert!(
                response.headers()[header::CONTENT_SECURITY_POLICY]
                    .to_str()
                    .unwrap()
                    .contains("form-action 'self'")
            );
        }
    }

    #[tokio::test]
    async fn hosted_errors_offer_a_way_forward() {
        for status in [
            StatusCode::BAD_REQUEST,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let response = hosted_error_page(status);
            assert_eq!(response.status(), status);
            let body = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap();
            let body = String::from_utf8(body.to_vec()).unwrap();
            assert!(body.contains("Go back and try again"));
            assert!(body.contains("href=\"/\">sign in or go to your reports</a>"));
        }
    }

    #[tokio::test]
    async fn token_errors_are_styled_and_escape_untrusted_text() {
        let response = hosted_token_error(crate::access::AccessError::Invalid("bad <name>".into()));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("bad &lt;name&gt;"));
        assert!(!body.contains("bad <name>"));
    }

    #[test]
    fn token_dates_show_utc_minutes_and_keep_unrecognized_values() {
        assert_eq!(
            token_date("2026-11-11T15:25:47.155Z"),
            "11 Nov 2026, 15:25 UTC"
        );
        assert_eq!(
            token_date("2026-11-11T17:25:47+02:00"),
            "11 Nov 2026, 15:25 UTC"
        );
        assert_eq!(token_date("unknown"), "unknown");
    }

    #[test]
    fn publication_dates_are_readable_and_unknown_dates_remain_visible() {
        assert_eq!(report_date("2026-11-11T19:00:00Z"), "11 Nov 2026");
        assert_eq!(report_date("2026-01-02"), "2 Jan 2026");
        assert_eq!(report_date("unknown"), "unknown");
    }

    async fn body_of(response: Response) -> String {
        let body = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn signed_out_pages_get_the_mark_one_card_and_only_the_legal_links() {
        for body in [
            body_of(hosted_page("Sign in", &hosted_pages::sign_in("", None))).await,
            body_of(sign_up_page().await).await,
            body_of(expired_link_page()).await,
            body_of(signups_paused_page()).await,
        ] {
            assert!(
                body.contains("<body class=\"docs-page auth-page\" data-richview-page=\"docs\">")
            );
            let head = &body[body.find("<header").unwrap()..body.find("</header>").unwrap()];
            assert!(
                head.contains("class=\"auth-brand\" href=\"https://richview.ai/\""),
                "{head}"
            );
            assert!(head.contains("<svg"), "{head}");
            assert!(!head.contains("nav__links"), "{head}");
            let foot = &body[body.find("<footer").unwrap()..body.find("</footer>").unwrap()];
            assert_eq!(foot.matches("<a ").count(), 3, "{foot}");
            for page in ["privacy", "terms", "impressum"] {
                assert!(
                    foot.contains(&format!("href=\"https://richview.ai/{page}\"")),
                    "{foot}"
                );
            }
        }
    }

    #[tokio::test]
    async fn signed_out_pages_without_a_form_show_only_the_mark_beside_the_message() {
        let body = body_of(error_page(StatusCode::NOT_FOUND)).await;
        assert!(
            body.contains(
                "<aside class=\"auth-side auth-side--plain\"><header class=\"auth-head\">"
            )
        );
        // The inline stylesheet names both classes, so look for the elements.
        assert!(!body.contains("class=\"auth-show\"") && !body.contains("class=\"auth-facts\""));
        assert!(body.contains("<h1>Not found</h1>"));
        let sign_in = body_of(hosted_page("Sign in", &hosted_pages::sign_in("", None))).await;
        assert!(
            sign_in.contains("class=\"auth-show\"")
                && !sign_in.contains("class=\"auth-side auth-side--plain\"")
        );
    }

    #[tokio::test]
    async fn an_invitation_shows_what_it_opens_and_who_sent_it_beside_its_form() {
        let invite = |report| hosted_pages::InvitePage {
            token: "t0ken",
            inviter: "Klára Nováková",
            inviter_email: "klara@example.test",
            organization: "Okurka <Retail>",
            role: Some("a reader, who opens what is shared"),
            report,
            account_exists: false,
            submitted_name: "",
            error: "",
        };
        let body = body_of(hosted_page(
            "Invitation",
            &hosted_pages::accept_invite(&invite(Some("Backlog <fell>"))),
        ))
        .await;
        let side = &body[body
            .find("<aside class=\"auth-side auth-side--invite\">")
            .unwrap()..body.find("</aside>").unwrap()];
        assert!(side.contains("<span class=\"auth-invite__avatar\">KN</span><span><strong>Klára Nováková</strong><span>shared a report with you</span>"));
        assert!(side.contains("<p class=\"auth-invite__title\">Backlog &lt;fell&gt;</p>"));
        assert!(side.contains("Okurka &lt;Retail&gt;</p>"));
        // The card replaces the flagship's; the form and its words stay in the main column.
        assert!(!body.contains("class=\"auth-show\"") && !body.contains("class=\"auth-facts\""));
        let main = &body[body.find("<main").unwrap()..];
        assert!(main.contains("<div class=\"cowork-auth\"><h1>Open the shared report</h1>"));
        assert!(!main.contains("class=\"auth-invite__title\""));
        let join = body_of(hosted_page(
            "Invitation",
            &hosted_pages::accept_invite(&invite(None)),
        ))
        .await;
        assert!(join.contains("<span>invited you to join</span>"));
        assert!(join.contains("<p class=\"auth-invite__title\">Okurka &lt;Retail&gt;</p>"));
        assert!(!join.contains("class=\"auth-invite__org\""));
    }

    #[tokio::test]
    async fn signed_in_pages_get_the_apps_rail_instead_of_the_sites_chrome() {
        let nav = hosted_pages::Nav {
            name: "Alex",
            email: "alex@example.test",
            csrf: "csrf",
            org: Some(("ab123456", "Acme")),
            role: Some("Owner"),
            orgs: &[],
        };
        let confirmation = hosted_pages::delete_confirmation(&nav, "org/report", "Q3");
        let body = body_of(hosted_page("Delete report", &confirmation)).await;
        assert!(body.contains("<body class=\"docs-page app-page\" data-richview-page=\"docs\">"));
        // The rail and top bar sit beside the content, which the skip link reaches first.
        let main = body.find("<main class=\"cowork\" id=\"content\">").unwrap();
        assert!(body.find("<aside class=\"app-rail\"").unwrap() < main);
        assert!(body.find("<header class=\"app-top\"").unwrap() < main);
        assert!(body.find("href=\"#content\">Skip to content</a>").unwrap() < main);
        assert!(body[main..].contains("Delete report?"));
        assert!(body.contains("<symbol id=\"i-report\""));
        assert!(!body.contains("class=\"nav__links\""));
        assert!(!body.contains(">Third-party notices</a>"));
        assert!(!body.contains("<footer"));
    }

    #[tokio::test]
    async fn signed_out_pages_show_what_the_app_makes_beside_the_form() {
        let body = body_of(hosted_page("Sign in", &hosted_pages::sign_in("", None))).await;
        let side = &body
            [body.find("<aside class=\"auth-side\">").unwrap()..body.find("</aside>").unwrap()];
        assert!(side.contains("<img src=\"data:image/png;base64,iVBOR"));
        assert!(
            side.contains("alt=\"The share card of the report The Most Expensive Race Ever Run")
        );
        assert_eq!(side.matches("<li>").count(), 3);
        // The form comes after the side in reading order, inside the page's main landmark.
        assert!(body.find("</aside>").unwrap() < body.find("<main class=\"cowork\">").unwrap());
    }

    #[test]
    fn sign_in_puts_the_reset_link_beside_the_password_after_its_field() {
        let form = hosted_pages::sign_in("", None);
        let field = &form[form.find("field--password").unwrap()..];
        let input = field.find("id=\"password\"").unwrap();
        let reset = field.find("href=\"/forgot\">Forgot password?</a>").unwrap();
        // After the field in reading and tab order; the stylesheet lifts it beside the label.
        assert!(input < reset);
        assert!(reset < field.find("type=\"submit\"").unwrap());
        assert_eq!(form.matches("href=\"/forgot\"").count(), 1);
        assert!(form.contains("<h1>Sign in to RichView</h1>"));
    }

    #[test]
    fn single_server_does_not_gain_the_hosted_script_policy() {
        let response = page("Sign in", "");
        let policy = response.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(!policy.contains("script-src"));
        assert!(!response.headers().contains_key(header::REFERRER_POLICY));
    }

    #[test]
    fn a_listed_reports_small_card_is_drawn_in_todays_design_not_shrunk_from_its_kept_card() {
        use super::super::reports::{charts_of, draw_share_card, share_card_svg};
        let dir = tempfile::tempdir().unwrap();
        let storage = crate::storage::FsStorage::new(dir.path()).unwrap();
        let source = "---\ntitle: T\nstyles: [sleek]\n---\n\n```chart\ntype: bar\ntitle: Share\ndata:\n  inline:\n    - { year: \"2023\", share: 3.0 }\n    - { year: \"2024\", share: 18.2 }\nencoding: { x: year, y: share }\n```\n";
        let report: crate::storage::StoredReport = serde_json::from_value(serde_json::json!({
            "slug": "small-card-today", "version": "1.0.0", "source": source, "html": "<html>chart</html>",
            "frontmatter": { "title": "T", "styles": ["sleek"] }, "createdAt": "now",
            "reportFolder": { "root": "card-fixture", "source": "report.rmd", "files": ["report.rmd"] },
        }))
        .unwrap();
        storage.put_version(report.clone()).unwrap();
        // The card kept at publication in an earlier design; the title card stands in for it.
        let kept = draw_share_card(&report, &[]).unwrap();
        storage
            .put_share_card("small-card-today", "1.0.0", &kept)
            .unwrap();
        let data = |png: &[u8]| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            )
        };
        let today = richview_share::card_png_small(
            &share_card_svg(&report, &charts_of(&report)).unwrap(),
            THUMB_WIDTH,
        )
        .unwrap();
        let shrunk = richview_share::thumbnail(&kept, THUMB_WIDTH).unwrap();
        let wanted = [
            ("small-card-today".to_owned(), "1.0.0".to_owned()),
            ("small-card-gone".to_owned(), "1.0.0".to_owned()),
        ];
        for _ in 0..2 {
            let urls = thumbnails(&storage, &wanted, std::time::Duration::from_secs(60));
            assert_eq!(urls[0].as_deref(), Some(data(&today).as_str()));
            assert_ne!(urls[0].as_deref(), Some(data(&shrunk).as_str()));
            assert_eq!(urls[1], None, "a version that is not stored has no card");
        }
    }
}
