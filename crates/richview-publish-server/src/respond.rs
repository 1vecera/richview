//! Response builders: every answer is `private, no-store` and carries the same hardening headers.
use crate::{host::Response, model::escape};
use serde::Serialize;

const BASE: [(&str, &str); 5] = [
    ("Cache-Control", "private, no-store, max-age=0"),
    ("Pragma", "no-cache"),
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "no-referrer"),
    ("X-Frame-Options", "SAMEORIGIN"),
];
/// The page policy: same-origin scripts, fonts and frames; the one remote image origin is Unsplash.
pub const PAGE_CSP: &str = "default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; img-src 'self' data: https://images.unsplash.com; font-src 'self'; frame-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'self'";

pub fn with(status: u16, content_type: &str, body: Vec<u8>, extra: &[(&str, String)]) -> Response {
    let mut headers: Vec<(String, String)> = BASE
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect();
    headers.push(("Content-Type".into(), content_type.into()));
    for (name, value) in extra {
        headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        headers.push(((*name).into(), value.clone()));
    }
    Response {
        status,
        headers,
        body,
    }
}
pub fn json<T: Serialize>(value: &T, status: u16) -> Response {
    with(
        status,
        "application/json; charset=utf-8",
        serde_json::to_vec(value).unwrap_or_default(),
        &[("X-Robots-Tag", "noindex".into())],
    )
}
pub fn html(body: String, status: u16, extra: &[(&str, String)]) -> Response {
    let mut all = vec![("Content-Security-Policy", PAGE_CSP.to_string())];
    all.extend(extra.iter().cloned());
    with(status, "text/html; charset=utf-8", body.into_bytes(), &all)
}
pub fn text(content_type: &str, body: String) -> Response {
    with(200, content_type, body.into_bytes(), &[])
}
pub fn redirect(status: u16, location: &str) -> Response {
    with(
        status,
        "text/plain; charset=utf-8",
        vec![],
        &[("Location", location.into())],
    )
}
/// The failure page; it names no item and says nothing about why access was refused.
pub fn failure(status: u16, message: &str) -> Response {
    let heading = if status == 404 || status == 410 {
        "This item is unavailable"
    } else {
        "RichView Publish"
    };
    html(
        format!(
            "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>RichView Publish</title><main><h1>{heading}</h1><p>{}</p><p><a href=\"/\">Publication home</a> · <a href=\"/_auth/login\">Member sign-in</a></p></main></html>",
            escape(message)
        ),
        status,
        &[("X-Robots-Tag", "noindex".into())],
    )
}
