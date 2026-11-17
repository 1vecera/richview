//! The report host: stored reports, their API, sign-in, governance and the pages for them
//! (TYP-70, FEA-SF-18..20), plus the local preview of `richview serve`.
// A rejected request is the response itself; boxing it would only add noise.
#![allow(clippy::result_large_err)]
use crate::{
    access::{Access, Principal},
    resolver,
    storage::FsStorage,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
use tower_http::compression::{
    CompressionLayer,
    predicate::{DefaultPredicate, NotForContentType, Predicate},
};

pub mod auth;
pub mod governance;
mod host_bar;
pub mod hosted;
mod hosted_auth;
mod hosted_routes;
pub(crate) mod mail_texts;
mod operator;
pub mod origin;
pub mod reports;
pub mod routes;
pub mod ui;

pub use reports::{PublishRequest, RenderRequest};

pub(crate) async fn serve_operator(
    port: u16,
    token: Arc<str>,
    state: AppState,
) -> Result<(), String> {
    operator::serve(port, token, state).await
}

#[derive(Clone)]
pub struct AppState {
    pub storage: FsStorage,
    pub access: Access,
    pub preview: Option<Preview>,
    pub public_origin: Option<String>,
    /// The origin that serves report pages when they are split from the app (ARC-15): sign-in,
    /// governance and the API stay on the app origin, and no app cookie reaches report pages.
    pub report_origin: Option<String>,
}

#[derive(Clone)]
pub struct Preview {
    pub source: PathBuf,
    pub styles: Vec<String>,
    pub allow_network: bool,
    /// The last render that succeeded, kept under a findings banner while the source is broken
    /// (FEA-SF-08).
    pub last_good: Arc<std::sync::Mutex<Option<String>>>,
}

/// The app, or the whole host when reports are not split off to their own origin.
pub fn build_app(state: AppState) -> Router {
    if state.access.is_hosted() != state.storage.is_hosted() {
        return Router::new().fallback(|| async {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Server storage mode does not match access mode",
            )
        });
    }
    let preview = state.preview.is_some();
    if preview {
        // `richview serve` answers the preview and nothing else.
        return finish(
            routes::router_for(true, routes::Side::Preview, false).fallback(not_found),
            state,
            false,
        );
    }
    if state.report_origin.is_some() {
        return finish(
            routes::router_for(preview, routes::Side::App, state.access.is_hosted())
                .fallback(origin::app_fallback),
            state,
            false,
        );
    }
    finish(
        routes::router_for(preview, routes::Side::All, state.access.is_hosted())
            .fallback(not_found),
        state,
        false,
    )
}

/// The report origin: report pages and their files, with the ticket exchange in front.
pub fn build_report_app(state: AppState) -> Router {
    if state.access.is_hosted() != state.storage.is_hosted() {
        return Router::new().fallback(|| async {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Server storage mode does not match access mode",
            )
        });
    }
    finish(
        routes::router_for(false, routes::Side::Report, state.access.is_hosted())
            .fallback(not_found),
        state,
        true,
    )
}

/// Marks a response whose handler must not run again for the same request: it handed out
/// something that exists once.
#[derive(Clone, Copy)]
pub(crate) struct Settled;

/// A read answers from one state. A publication changes who may open a report and which
/// versions exist; a reader whose rights check came before that and whose bytes came after
/// would get the new version on the old rights. So a GET that a publication commit overlapped
/// is run again, and one that arrives during a commit waits for it.
async fn consistent_read(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) {
        return next.run(request).await;
    }
    let (parts, _) = request.into_parts();
    // Waiting for a commit costs time, not attempts: a large publication can write for
    // seconds. Publications are serialized, so a handful of them overlapping one read in a
    // row means the server is saturated.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut runs = 0;
    while runs < 8 && std::time::Instant::now() < deadline {
        let before = state.access.commit_epoch();
        if before % 2 == 1 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            continue;
        }
        runs += 1;
        let response = next
            .clone()
            .run(Request::from_parts(parts.clone(), Body::empty()))
            .await;
        // An answer that gave something out once (a ticket) is not asked for twice.
        if state.access.commit_epoch() == before || response.extensions().get::<Settled>().is_some()
        {
            return response;
        }
    }
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::RETRY_AFTER, "1")],
        "The server is busy publishing; try again.",
    )
        .into_response()
}

async fn redeem_on_report_origin(
    State((state, report_listener)): State<(AppState, bool)>,
    request: Request,
    next: Next,
) -> Response {
    if report_listener {
        origin::redeem(State(state), request, next).await
    } else {
        next.run(request).await
    }
}

fn finish(router: Router<AppState>, state: AppState, report_listener: bool) -> Router {
    router
        // Innermost, so a repeated read passes through nothing twice but its own handler.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            consistent_read,
        ))
        // Outside the repeated read: a ticket is single-use, so it is redeemed once and its
        // cookie is never thrown away with a repeated answer.
        .layer(middleware::from_fn_with_state(
            (state.clone(), report_listener),
            redeem_on_report_origin,
        ))
        .layer(DefaultBodyLimit::max(crate::MAX_INPUT_BYTES))
        // Inside the limit and ahead of every extractor: an API write is refused on its headers
        // before any of its body is buffered (FEA-SF-19).
        .layer(middleware::from_fn_with_state(
            state.clone(),
            refuse_before_body,
        ))
        // Offline export needs the gzip file itself. Recompressing it wastes work and
        // some browser stacks retain that outer layer for application/gzip responses.
        .layer(CompressionLayer::new().compress_when(
            DefaultPredicate::new().and(NotForContentType::const_new("application/gzip")),
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security_headers,
        ))
        // Outermost: every handler sees which listener took the connection.
        .layer(middleware::from_fn(move |request, next| {
            origin::mark_listener(report_listener, request, next)
        }))
        .with_state(state)
}

// Limit total body time, not just idle time between bytes. The proxy streams request bodies
// to the app, so expiration releases upload connections and form admission workers.
fn request_body_deadline(body: Body, duration: std::time::Duration) -> Body {
    use futures_util::StreamExt;
    let deadline = tokio::time::Instant::now() + duration;
    let stream = body.into_data_stream();
    Body::from_stream(futures_util::stream::unfold(
        (stream, deadline, false),
        |(mut stream, deadline, done)| async move {
            if done {
                return None;
            }
            if tokio::time::Instant::now() >= deadline {
                return Some((
                    Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Upload timed out",
                    )),
                    (stream, deadline, true),
                ));
            }
            match tokio::time::timeout_at(deadline, stream.next()).await {
                Ok(Some(item)) => Some((
                    item.map_err(std::io::Error::other),
                    (stream, deadline, false),
                )),
                Ok(None) => None,
                Err(_) => Some((
                    Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Upload timed out",
                    )),
                    (stream, deadline, true),
                )),
            }
        },
    ))
}

fn bound_hosted_request_body(request: Request, duration: std::time::Duration) -> Request {
    if request.method() != axum::http::Method::POST
        || !matches!(
            request.uri().path(),
            "/api/reports"
                | "/api/check"
                | "/api/validate"
                | "/api/session"
                | "/ui/sign-in"
                | "/ui/sign-up"
                | "/ui/forgot"
        )
    {
        return request;
    }
    let (parts, body) = request.into_parts();
    Request::from_parts(parts, request_body_deadline(body, duration))
}

/// API and form writes need a signed-in caller, decided on the headers before any body is read, so
/// an anonymous caller cannot make the server buffer a large body only to refuse it. Sign-in is the
/// exception; its body is bounded as it streams (see `routes::sign_in_limit`).
async fn refuse_before_body(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let writes = matches!(
        *request.method(),
        axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::PATCH
            | axum::http::Method::DELETE
    );
    let path = request.uri().path();
    // Sign-in takes credentials and nothing else: a body announced as large is refused on the
    // header; the streaming cap on the route covers a body that does not announce itself.
    if request.method() == axum::http::Method::POST
        && matches!(path, "/api/session" | "/ui/sign-in")
        && request
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|n| n > routes::SIGN_IN_BODY_BYTES)
    {
        return api_error(StatusCode::PAYLOAD_TOO_LARGE, "Sign-in request too large");
    }
    let signing_in = request.method() == axum::http::Method::POST
        && matches!(path, "/api/session" | "/ui/sign-in");
    let hosted_link_form = state.access.is_hosted()
        && request.method() == axum::http::Method::POST
        && (matches!(path, "/ui/sign-up" | "/ui/forgot") || path.starts_with("/ui/link/"));
    let hosted_publish = state.access.is_hosted()
        && request.method() == axum::http::Method::POST
        && path == "/api/reports";
    if writes
        && !signing_in
        && !hosted_link_form
        && (path.starts_with("/api/") || path.starts_with("/ui/"))
        && matches!(
            auth::caller(&state, request.headers()).principal,
            Principal::Anonymous
        )
    {
        // A form post from a page whose session lapsed goes back to the sign-in page.
        if path.starts_with("/ui/") {
            return ui::sign_in_first();
        }
        if let Err(refusal) = auth::mutating(&state, request.headers()) {
            return refusal;
        }
    }
    let hosted_compute = state.access.is_hosted()
        && request.method() == axum::http::Method::POST
        && (matches!(path, "/api/check" | "/api/validate")
            || matches!(
                path.split('/').collect::<Vec<_>>().as_slice(),
                ["", "api", "reports", _, _, "v", _, "regenerate"]
            ));
    if hosted_compute {
        let caller = match auth::mutating(&state, request.headers()) {
            Ok(caller) => caller,
            Err(response) => return response,
        };
        if matches!(path, "/api/check" | "/api/validate")
            && !state.access.can_create(&caller.principal)
        {
            return api_error(
                StatusCode::FORBIDDEN,
                "Your role does not include the right to publish.",
            );
        }
        let permit = match state.access.admit_hosted_compute(&caller.principal) {
            Ok(permit) => permit,
            Err(crate::access::AccessError::Throttled) => {
                return api_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Compute is busy or the hourly organization limit was reached.",
                );
            }
            Err(_) => return api_error(StatusCode::FORBIDDEN, "Compute is unavailable."),
        };
        request.extensions_mut().insert(Arc::new(permit));
    }
    if hosted_publish {
        let permit = match state.access.admit_hosted_publish_worker() {
            Ok(permit) => permit,
            Err(_) => {
                return api_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Publishing is busy; try again shortly.",
                );
            }
        };
        request.extensions_mut().insert(Arc::new(permit));
    }
    if state.access.is_hosted()
        && request.method() == axum::http::Method::POST
        && matches!(request.uri().path(), "/ui/sign-up" | "/ui/forgot")
    {
        if !auth::same_origin(request.headers()) {
            return StatusCode::FORBIDDEN.into_response();
        }
        let permit = match state
            .access
            .admit_link_request(hosted_auth::client_ip(request.headers()))
        {
            Ok(permit) => permit,
            Err(_) => return ui::check_mail_page(&state),
        };
        request.extensions_mut().insert(Arc::new(permit));
    }
    if state.access.is_hosted() {
        request = bound_hosted_request_body(request, std::time::Duration::from_secs(120));
    }
    next.run(request).await
}

/// The report host serves reports only; site paths are not found (ARC-24).
async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Not found").into_response()
}

async fn security_headers(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let sensitive_link = state.access.is_hosted()
        && ["/link/", "/ui/link/"]
            .iter()
            .any(|prefix| request.uri().path().starts_with(*prefix));
    let hosted_report = state.access.is_hosted()
        && ["/r/", "/raw/", "/api/reports/"]
            .iter()
            .any(|prefix| request.uri().path().starts_with(*prefix));
    let browser_page = !request.uri().path().starts_with("/api/")
        && request
            .headers()
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|accept| {
                accept
                    .split(',')
                    .any(|item| item.trim().starts_with("text/html"))
            });
    let hosted_app_page =
        state.access.is_hosted() && !origin::on_report_origin(&state, request.headers());
    let mut response = next.run(request).await;
    let is_html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|content_type| content_type.starts_with("text/html"));
    if browser_page
        && !is_html
        && (response.status().is_client_error() || response.status().is_server_error())
    {
        let mut preserved = Vec::new();
        for name in [
            header::ALLOW,
            header::WWW_AUTHENTICATE,
            header::RETRY_AFTER,
            header::REFERRER_POLICY,
        ] {
            for value in response.headers().get_all(&name) {
                preserved.push((name.clone(), value.clone()));
            }
        }
        response = if hosted_app_page {
            ui::hosted_error_page(response.status())
        } else {
            ui::error_page(response.status())
        };
        for (name, value) in preserved {
            response.headers_mut().append(name, value);
        }
    }
    response
        .headers_mut()
        .insert("x-richview", "v3-rust".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
        .headers_mut()
        .entry(header::REFERRER_POLICY)
        .or_insert(header::HeaderValue::from_static(
            "strict-origin-when-cross-origin",
        ));
    if sensitive_link {
        response.headers_mut().insert(
            header::REFERRER_POLICY,
            header::HeaderValue::from_static("no-referrer"),
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
    }
    if hosted_report {
        response
            .headers_mut()
            .insert("x-robots-tag", "noindex".parse().unwrap());
    }
    // Visibility and suspension can change even for immutable report versions. A shared cache
    // must not continue serving a previously public hosted report after that change.
    if hosted_report {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    }
    response
}

/// The body of a refused request: the T-CLI-JSON failure object (`ok`, `status`, `output`,
/// `diagnostics`) plus the one-sentence `error` TYP-70 gives every HTTP error.
pub(crate) fn error_body(
    error: impl Into<String>,
    diagnostics: impl serde::Serialize,
) -> serde_json::Value {
    json!({"ok": false, "status": "issues", "output": null, "error": error.into(), "diagnostics": diagnostics})
}

fn api_error(status: StatusCode, message: impl Into<String>) -> Response {
    let message = message.into();
    if let Ok(diagnostics) =
        serde_json::from_str::<Vec<richview_core::document::Diagnostic>>(&message)
    {
        return (status, Json(error_body("Report rejected", diagnostics))).into_response();
    }
    // TYP-70: every error body says `ok: false` and carries a `diagnostics` list.
    (status, Json(error_body(message, json!([])))).into_response()
}

async fn preview_revision(State(state): State<AppState>) -> Response {
    let Some(preview) = state.preview else {
        return api_error(StatusCode::NOT_FOUND, "No preview source");
    };
    match tokio::task::spawn_blocking(move || preview_source_revision(&preview)).await {
        Ok(Ok(revision)) => Json(json!({"revision": revision})).into_response(),
        Ok(Err(e)) => api_error(StatusCode::UNPROCESSABLE_ENTITY, e),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Preview worker failed"),
    }
}

// Polling tracks source, referenced data/media and cascading style metadata. It never renders
// charts or copies embedded Wasm on a revision probe. An atomic editor save is detected too.
#[derive(Clone)]
struct PreviewDependencies {
    stamp: (u64, Option<std::time::SystemTime>),
    references: Vec<PathBuf>,
}
static PREVIEW_DEPS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::BTreeMap<String, PreviewDependencies>>,
> = std::sync::OnceLock::new();

pub fn preview_source_revision(preview: &Preview) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    crate::resolver::report_folder(&preview.source)?;
    let key = format!("{}{:?}", preview.source.display(), preview.styles);
    let metadata = std::fs::metadata(&preview.source).map_err(|e| e.to_string())?;
    let stamp = (metadata.len(), metadata.modified().ok());
    let mut cache = PREVIEW_DEPS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Preview cache lock poisoned")?;
    if !cache.get(&key).is_some_and(|d| d.stamp == stamp) {
        let source = crate::cli::read_source(&preview.source)?;
        let document = richview_core::document::parse(&source)
            .map_err(|e| serde_json::to_string(&e).unwrap_or_default())?;
        let parent = preview.source.parent().ok_or("Source has no parent")?;
        let mut references: Vec<_> = richview_core::document::report_resources(&document)
            .iter()
            .map(|r| parent.join(&r.path))
            .collect();
        references.extend(
            preview
                .styles
                .iter()
                .filter(|s| s.contains('.') && !s.contains("://"))
                .map(|s| parent.join(s)),
        );
        references.sort();
        references.dedup();
        if cache.len() >= 32 {
            cache.clear();
        }
        cache.insert(key.clone(), PreviewDependencies { stamp, references });
    }
    let dependencies = cache.get(&key).unwrap();
    let mut hash = Sha256::new();
    hash.update(format!("{:?}{:?}", dependencies.stamp, preview.styles));
    for path in &dependencies.references {
        hash.update(path.to_string_lossy().as_bytes());
        match std::fs::metadata(path) {
            Ok(m) => hash.update(format!("{}{:?}", m.len(), m.modified().ok())),
            Err(e) => hash.update(e.to_string()),
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn preview_render(
    preview: &Preview,
) -> Result<(richview_core::document::RenderedReport, String), String> {
    let source =
        String::from_utf8(resolver::bounded_read(&preview.source)?).map_err(|e| e.to_string())?;
    let (document, options) = resolver::resolve_document_options_policy(
        &source,
        &preview.source,
        &preview.styles,
        preview.allow_network,
        true,
    )?;
    let revision = preview_source_revision(preview)?;
    let mut report = richview_core::document::render_document(document, &options)
        .map_err(|e| serde_json::to_string(&e).unwrap_or_default())?;
    // The preview page polls its own host for the next revision; nothing else may connect.
    // The policy sits in a meta attribute, so its quotes are escaped. Only that attribute counts:
    // a paragraph that mentions `connect-src` must not keep the reload poll blocked.
    const MARK: &str = "http-equiv=\"Content-Security-Policy\" content=\"";
    if let Some(start) = report.html.find(MARK).map(|i| i + MARK.len())
        && let Some(len) = report.html[start..].find('"')
        && !report.html[start..start + len].contains("connect-src")
    {
        let policy = &report.html[start..start + len];
        let widened = policy.replacen(
            "default-src &#39;none&#39;;",
            "default-src &#39;none&#39;; connect-src &#39;self&#39;;",
            1,
        );
        report.html = format!(
            "{}{widened}{}",
            &report.html[..start],
            &report.html[start + len..]
        );
    }
    report.html = report.html.replacen("</head>", &format!("<meta name=\"richview-preview\" content=\"/__revision\"><meta name=\"richview-revision\" content=\"{revision}\"></head>"), 1);
    Ok((report, revision))
}

/// The preview page of `richview serve`: the current render, or the last good one under a banner.
pub(crate) async fn preview_page(state: &AppState) -> Option<Response> {
    if let Some(preview) = state.preview.clone() {
        let kept = preview.last_good.clone();
        let for_revision = preview.clone();
        let rendered = tokio::task::spawn_blocking(move || preview_render(&preview)).await;
        let page = |html: String| {
            (
                [
                    (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                html,
            )
                .into_response()
        };
        let remembered = || kept.lock().ok().and_then(|html| html.clone());
        return Some(match rendered {
            Ok(Ok((report, _))) => {
                if let Ok(mut html) = kept.lock() {
                    *html = Some(report.html.clone());
                }
                page(crate::banner::inject(&report.html, &report.diagnostics))
            }
            Ok(Err(e)) => match remembered() {
                Some(html) => {
                    // The kept page carries the revision it was rendered at; moving it to the
                    // current one stops the page's poll from reloading in a loop.
                    let html = match preview_source_revision(&for_revision) {
                        Ok(revision) => with_revision(&html, &revision),
                        Err(_) => html,
                    };
                    page(crate::banner::inject(
                        &html,
                        &crate::banner::diagnostics_of(&e),
                    ))
                }
                // Nothing rendered yet (a broken source at startup): the banner on an empty page
                // that asks again every second, so the page recovers when the source is fixed.
                None => page(crate::banner::inject(
                    "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"1\"><title>RichView preview</title></head><body></body></html>",
                    &crate::banner::diagnostics_of(&e),
                )),
            },
            Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Preview worker failed"),
        });
    }
    None
}

/// `GET /thumbnail`: the 1200×630 link-preview card of the story as it would be published,
/// drawn on demand from the current source (ARC-69).
pub(crate) async fn preview_thumbnail(State(state): State<AppState>) -> Response {
    let Some(preview) = state.preview.clone() else {
        return not_found().await;
    };
    let card = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let source = crate::cli::read_source(&preview.source)?;
        let (report, _) = preview_render(&preview)?;
        let document = richview_core::document::parse(&source)
            .map_err(|e| serde_json::to_string(&e).unwrap_or_default())?;
        let text = |key: &str, cap: usize| {
            document.frontmatter[key]
                .as_str()
                .map(|t| t.chars().take(cap).collect::<String>())
        };
        let style = preview.styles.first().cloned().unwrap_or_else(|| {
            document.frontmatter["styles"][0]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        });
        let (_, options) = resolver::resolve_document_options_policy(
            &source,
            &preview.source,
            &preview.styles,
            preview.allow_network,
            false,
        )?;
        let company = richview_core::report_company_brand(&document.frontmatter, &options);
        let style = company.base.as_deref().unwrap_or(&style).to_owned();
        richview_share::card_png_with_company(
            &text("title", 200).unwrap_or_default(),
            &text("author", 120).unwrap_or_default(),
            &style,
            &report.charts,
            (company.logo.is_some() || company.accent.is_some()).then_some(company),
        )
    })
    .await;
    match card {
        Ok(Ok(png)) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            png,
        )
            .into_response(),
        Ok(Err(e)) => api_error(StatusCode::UNPROCESSABLE_ENTITY, e),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Thumbnail worker failed"),
    }
}

fn with_revision(html: &str, revision: &str) -> String {
    const MARK: &str = "<meta name=\"richview-revision\" content=\"";
    match html.find(MARK) {
        Some(at) => {
            let start = at + MARK.len();
            match html[start..].find('"') {
                Some(len) => format!("{}{revision}{}", &html[..start], &html[start + len..]),
                None => html.to_owned(),
            }
        }
        None => html.to_owned(),
    }
}

async fn bind_listener(
    bind: &str,
    port: u16,
    option: &str,
) -> Result<tokio::net::TcpListener, String> {
    tokio::net::TcpListener::bind((bind, port))
        .await
        .map_err(|error| {
            let mut message = format!("Cannot listen on {bind}:{port}: {error}");
            if error.kind() == std::io::ErrorKind::AddrInUse {
                message.push_str(&format!("; choose another port with {option}"));
            }
            message
        })
}

// A separate signal listener arms an OS-thread deadline even when app workers are blocked.
#[cfg(unix)]
pub(crate) fn start_process_shutdown_watchdog(deadline: std::time::Duration) -> Result<(), String> {
    let (ready, started) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("richview-shutdown".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready.send(Err(error.to_string()));
                    return;
                }
            };
            runtime.block_on(async move {
                let mut term =
                    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    {
                        Ok(signal) => signal,
                        Err(error) => {
                            let _ = ready.send(Err(error.to_string()));
                            return;
                        }
                    };
                let mut interrupt =
                    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                    {
                        Ok(signal) => signal,
                        Err(error) => {
                            let _ = ready.send(Err(error.to_string()));
                            return;
                        }
                    };
                let _ = ready.send(Ok(()));
                tokio::select! { _ = term.recv() => {}, _ = interrupt.recv() => {} }
                if std::thread::Builder::new()
                    .name("richview-shutdown-deadline".into())
                    .spawn(move || {
                        std::thread::sleep(deadline);
                        // SAFETY: terminate this process without waiting for blocking tasks or exit handlers.
                        unsafe { libc::_exit(143) };
                    })
                    .is_err()
                {
                    // SAFETY: fail closed if the process-level deadline cannot be armed.
                    unsafe { libc::_exit(143) };
                }
                tokio::select! { _ = term.recv() => {}, _ = interrupt.recv() => {} }
                // SAFETY: a second delivered signal requests an immediate process exit.
                unsafe { libc::_exit(143) };
            });
        })
        .map_err(|error| error.to_string())?;
    started
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "Cannot start shutdown watchdog".to_owned())?
}

#[cfg(not(unix))]
pub(crate) fn start_process_shutdown_watchdog(
    _deadline: std::time::Duration,
) -> Result<(), String> {
    Ok(())
}

// SIGTERM is how the service manager stops a hosted instance.
pub(crate) async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

pub async fn serve(state: AppState, bind: &str, port: u16) -> Result<(), String> {
    let listener = bind_listener(bind, port, "--port").await?;
    println!(
        "RichView at http://{} (data: {})",
        listener.local_addr().map_err(|e| e.to_string())?,
        state.storage.dir().display()
    );
    let queue = state.access.mail_queue().cloned();
    let (stop, wait) = tokio::sync::watch::channel(false);
    let server = axum::serve(listener, build_app(state)).with_graceful_shutdown(async move {
        let mut wait = wait;
        let _ = wait.changed().await;
    });
    let server = server.into_future();
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result.map_err(|e| e.to_string()),
        _ = shutdown_signal() => {
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(25);
            if let Some(queue) = &queue { queue.begin_shutdown(); }
            let _ = stop.send(true);
            let result = tokio::time::timeout_at(deadline, &mut server).await;
            if let Some(queue) = &queue { queue.finish_shutdown(deadline).await; }
            match result { Ok(result) => result.map_err(|e| e.to_string()), Err(_) => Ok(()) }
        }
    }
}

/// Serves the app and, on a second listener, the report origin (ARC-15). Without a configured
/// report origin it is this host on the report port: locally a second port, hosted a second
/// hostname that the deployment points here.
pub async fn serve_split(
    mut state: AppState,
    bind: &str,
    port: u16,
    report_port: u16,
    report_origin: Option<String>,
) -> Result<(), String> {
    let app_listener = bind_listener(bind, port, "--port").await?;
    let report_listener = bind_listener(bind, report_port, "--report-port").await?;
    let app_addr = app_listener.local_addr().map_err(|e| e.to_string())?;
    let report_addr = report_listener.local_addr().map_err(|e| e.to_string())?;
    let loopback = matches!(bind, "127.0.0.1" | "localhost" | "::1");
    if report_origin.is_none() && !loopback {
        return Err(
            "--report-origin is required when the server listens beyond loopback: readers are sent there with a ticket, so it must be the address they can reach".into(),
        );
    }
    if state.public_origin.is_none() {
        let host = if loopback { bind } else { "localhost" };
        state.public_origin = Some(format!("http://{host}:{}", app_addr.port()));
    }
    let origin = report_origin.unwrap_or_else(|| {
        let host = match bind {
            "0.0.0.0" | "::" => "localhost",
            other => other,
        };
        format!("http://{host}:{}", report_addr.port())
    });
    state.report_origin = Some(origin.clone());
    println!(
        "RichView at http://{app_addr} (data: {})",
        state.storage.dir().display()
    );
    println!("Reports at {origin} (listening on {report_addr})");
    let queue = state.access.mail_queue().cloned();
    let (stop, wait) = tokio::sync::watch::channel(false);
    let mut app_wait = wait.clone();
    let mut report_wait = wait;
    let app =
        axum::serve(app_listener, build_app(state.clone())).with_graceful_shutdown(async move {
            let _ = app_wait.changed().await;
        });
    let reports =
        axum::serve(report_listener, build_report_app(state)).with_graceful_shutdown(async move {
            let _ = report_wait.changed().await;
        });
    let servers = async {
        let (app, reports) = tokio::join!(app.into_future(), reports.into_future());
        app.and(reports).map_err(|e| e.to_string())
    };
    tokio::pin!(servers);
    tokio::select! {
        result = &mut servers => result,
        _ = shutdown_signal() => {
            // Give in-flight requests and the current delivery at most 25 seconds in total.
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(25);
            if let Some(queue) = &queue { queue.begin_shutdown(); }
            let _ = stop.send(true);
            let result = tokio::time::timeout_at(deadline, &mut servers).await;
            if let Some(queue) = &queue { queue.finish_shutdown(deadline).await; }
            match result { Ok(result) => result, Err(_) => Ok(()) }
        }
    }
}

#[cfg(test)]
mod header_tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    #[cfg(unix)]
    #[tokio::test]
    async fn watchdog_ends_a_process_with_blocked_work_at_deadline_or_second_signal() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        const MODE: &str = "RICHVIEW_TEST_WATCHDOG_MODE";
        if let Ok(mode) = std::env::var(MODE) {
            let bound = if mode == "deadline" {
                std::time::Duration::from_millis(300)
            } else {
                std::time::Duration::from_secs(5)
            };
            super::start_process_shutdown_watchdog(bound).unwrap();
            tokio::spawn(super::shutdown_signal());
            tokio::task::yield_now().await;
            let (started, ready) = std::sync::mpsc::sync_channel(1);
            tokio::task::spawn_blocking(move || {
                let _ = started.send(());
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
            });
            ready
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            println!("WATCHDOG_FIXTURE_READY");
            std::future::pending::<()>().await;
            return;
        }
        for mode in ["deadline", "second"] {
            let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "server::header_tests::watchdog_ends_a_process_with_blocked_work_at_deadline_or_second_signal", "--nocapture"])
                .env(MODE, mode).stdout(std::process::Stdio::piped()).kill_on_drop(true).spawn().unwrap();
            let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while let Some(line) = lines.next_line().await.unwrap() {
                    if line.contains("WATCHDOG_FIXTURE_READY") {
                        return;
                    }
                }
                panic!("watchdog fixture exited before ready");
            })
            .await
            .unwrap();
            let pid = child.id().unwrap() as libc::pid_t;
            // SAFETY: this PID belongs to the child spawned by this test.
            assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
            if mode == "second" {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                // SAFETY: the child is still waiting for its five-second deadline.
                assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
            }
            let status = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(status.code(), Some(143));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sigterm_is_observed() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        const CHILD: &str = "RICHVIEW_TEST_SIGTERM_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let task = tokio::spawn(super::shutdown_signal());
            tokio::task::yield_now().await;
            println!("SIGTERM_FIXTURE_READY");
            tokio::time::timeout(std::time::Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap();
            return;
        }
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "server::header_tests::sigterm_is_observed",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                if line.contains("SIGTERM_FIXTURE_READY") {
                    return;
                }
            }
            panic!("signal fixture exited before ready");
        })
        .await
        .unwrap();
        let pid = child.id().unwrap() as libc::pid_t;
        // SAFETY: the PID belongs to the child just spawned by this test.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
        let status = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success());
    }

    #[tokio::test]
    async fn an_error_page_keeps_a_stricter_referrer_policy() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState {
            storage: FsStorage::new(dir.path()).unwrap(),
            access: Access::open(dir.path(), crate::access::Visibility::Private, None).unwrap(),
            preview: None,
            public_origin: None,
            report_origin: None,
        };
        let app = finish(
            Router::new().route(
                "/test",
                get(|| async {
                    (
                        StatusCode::BAD_REQUEST,
                        [(header::REFERRER_POLICY, "no-referrer")],
                        "bad",
                    )
                }),
            ),
            state,
            false,
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(header::ACCEPT, "text/html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
    }

    #[tokio::test]
    async fn upload_deadline_expires_even_when_bytes_keep_arriving() {
        use futures_util::StreamExt;
        let source = futures_util::stream::unfold((), |_| async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Some((Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"x")), ()))
        });
        let mut stream = request_body_deadline(
            Body::from_stream(source),
            std::time::Duration::from_millis(55),
        )
        .into_data_stream();
        let mut chunks = 0;
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                match stream.next().await {
                    Some(Ok(_)) => chunks += 1,
                    Some(Err(_)) => break,
                    None => panic!("stream ended without deadline error"),
                }
            }
        })
        .await
        .expect("upload stream did not time out");
        assert!(chunks > 0);
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn hosted_auth_forms_enforce_the_total_body_deadline() {
        use futures_util::StreamExt;
        for path in ["/ui/sign-up", "/ui/forgot", "/ui/sign-in", "/api/session"] {
            let source = futures_util::stream::unfold((), |_| async {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                Some((Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"x")), ()))
            });
            let request = Request::builder()
                .method("POST")
                .uri(path)
                .body(Body::from_stream(source))
                .unwrap();
            let request = bound_hosted_request_body(request, std::time::Duration::from_millis(55));
            let mut stream = request.into_body().into_data_stream();
            let mut chunks = 0;
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    match stream.next().await {
                        Some(Ok(_)) => chunks += 1,
                        Some(Err(_)) => break,
                        None => panic!("{path} ended without a deadline error"),
                    }
                }
            })
            .await
            .expect("auth form did not time out");
            assert!(chunks > 0, "{path}");
            assert!(stream.next().await.is_none(), "{path}");
        }
    }

    #[tokio::test]
    async fn timed_out_forgot_bodies_release_link_request_workers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let state = AppState {
            storage: FsStorage::new(dir.path()).unwrap().with_hosted(true),
            access: access.clone(),
            preview: None,
            public_origin: None,
            report_origin: None,
        };
        let app = build_app(state);
        let started = Arc::new(AtomicUsize::new(0));
        let mut requests = tokio::task::JoinSet::new();
        for _ in 0..4 {
            let started = started.clone();
            let source =
                futures_util::stream::unfold((true, started), |(first, started)| async move {
                    if first {
                        started.fetch_add(1, Ordering::SeqCst);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    Some((
                        Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"x")),
                        (false, started),
                    ))
                });
            let request = Request::builder()
                .method("POST")
                .uri("/ui/forgot")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header("x-real-ip", "192.0.2.20")
                .body(Body::from_stream(source))
                .unwrap();
            let request = bound_hosted_request_body(request, std::time::Duration::from_millis(200));
            let app = app.clone();
            requests.spawn(async move { app.oneshot(request).await.unwrap() });
        }
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while started.load(Ordering::SeqCst) != 4 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            access
                .admit_link_request(Some("192.0.2.21".parse().unwrap()))
                .is_err()
        );
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while let Some(result) = requests.join_next().await {
                assert_eq!(result.unwrap().status(), StatusCode::BAD_REQUEST);
            }
        })
        .await
        .expect("forgot requests did not finish");
        assert!(
            access
                .admit_link_request(Some("192.0.2.22".parse().unwrap()))
                .is_ok()
        );
    }

    #[tokio::test]
    async fn hosted_publish_admission_refuses_before_parsing_the_body() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let link = access
            .issue_link("owner@example.test", crate::access::LinkPurpose::Signup)
            .unwrap()
            .unwrap();
        let user = access
            .complete_hosted_link(&link, "Owner", "long password here")
            .await
            .unwrap();
        let org = access.organizations_for(&user.id)[0].id.clone();
        let (_, token) = access
            .create_self_token_for_fixture(&user.id, &org, "test")
            .unwrap();
        let permits: Vec<_> = (0..4)
            .map(|_| access.admit_hosted_publish_worker().unwrap())
            .collect();
        let state = AppState {
            storage: FsStorage::new(dir.path()).unwrap().with_hosted(true),
            access,
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let app = build_app(state);
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/api/reports")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("not-json"))
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        drop(permits);
        assert_eq!(
            app.oneshot(request()).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn hosted_compute_permits_survive_disconnected_requests() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(1)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let access = Access::open_hosted(dir.path()).unwrap();
            let link = access
                .issue_link("compute@example.test", crate::access::LinkPurpose::Signup)
                .unwrap()
                .unwrap();
            let user = access
                .complete_hosted_link(&link, "Compute", "long password here")
                .await
                .unwrap();
            let org = access.organizations_for(&user.id)[0].id.clone();
            let (_, token) = access
                .create_self_token_for_fixture(&user.id, &org, "test")
                .unwrap();
            let app = build_app(AppState {
                storage: FsStorage::new(dir.path()).unwrap().with_hosted(true),
                access: access.clone(),
                preview: None,
                public_origin: None,
                report_origin: None,
            });
            let (release, wait) = std::sync::mpsc::channel();
            let (started, ready) = tokio::sync::oneshot::channel();
            let blocker = tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                wait.recv().unwrap();
            });
            ready.await.unwrap();
            let consumed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut tasks = Vec::new();
            for _ in 0..4 {
                let app = app.clone();
                let token = token.clone();
                let consumed = consumed.clone();
                tasks.push(tokio::spawn(async move {
                    let body = Body::from_stream(futures_util::stream::once(async move {
                        consumed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok::<_, std::io::Error>(bytes::Bytes::from_static(
                            br##"{"source":"# Test"}"##,
                        ))
                    }));
                    app.oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/api/check")
                            .header(header::AUTHORIZATION, format!("Bearer {token}"))
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(body)
                            .unwrap(),
                    )
                    .await
                    .unwrap()
                }));
            }
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                while consumed.load(std::sync::atomic::Ordering::SeqCst) != 4 {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            for task in tasks {
                task.abort();
            }
            let request = || {
                Request::builder()
                    .method("POST")
                    .uri("/api/validate")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap()
            };
            assert_eq!(
                app.clone().oneshot(request()).await.unwrap().status(),
                StatusCode::TOO_MANY_REQUESTS
            );
            release.send(()).unwrap();
            blocker.await.unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                loop {
                    if app.clone().oneshot(request()).await.unwrap().status()
                        != StatusCode::TOO_MANY_REQUESTS
                    {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
        });
    }

    #[tokio::test]
    async fn hosted_link_admission_refuses_before_parsing_the_body() {
        let dir = tempfile::tempdir().unwrap();
        let access = Access::open_hosted(dir.path()).unwrap();
        let permits: Vec<_> = (0..4)
            .map(|_| access.admit_link_request(None).unwrap())
            .collect();
        let state = AppState {
            storage: FsStorage::new(dir.path()).unwrap().with_hosted(true),
            access,
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        };
        let app = build_app(state);
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/ui/forgot")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("not-an-email-field=1"))
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            StatusCode::OK
        );
        drop(permits);
        assert_eq!(
            app.oneshot(request()).await.unwrap().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
