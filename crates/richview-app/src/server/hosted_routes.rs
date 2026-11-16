//! Adapt hosted organization/slug path segments to the shared report handlers.
use super::{AppState, auth, governance, reports};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::Value;

fn docs_asset(bytes: Option<&'static [u8]>, mime: &'static str) -> Response {
    match bytes {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            bytes,
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Hosted docs runtime unavailable",
        )
            .into_response(),
    }
}

pub async fn docs_js() -> Response {
    docs_asset(
        crate::runtime::DOCS_JS,
        "application/javascript; charset=utf-8",
    )
}
pub async fn docs_wasm() -> Response {
    docs_asset(crate::runtime::DOCS_WASM, "application/wasm")
}
pub async fn docs_bootstrap() -> Response {
    docs_asset(
        crate::runtime::DOCS_BOOTSTRAP,
        "application/javascript; charset=utf-8",
    )
}

fn key(state: &AppState, org: &str, slug: &str) -> Result<String, Response> {
    let key = crate::storage::ReportKey::hosted(org, slug).map_err(|_| auth::not_found())?;
    let key = key.as_str().to_owned();
    state
        .storage
        .validate_key(&key)
        .map_err(|_| auth::not_found())?;
    Ok(key)
}
macro_rules! one {
    ($name:ident, $target:path) => {
        pub async fn $name(
            State(state): State<AppState>,
            headers: HeaderMap,
            Path((org, slug)): Path<(String, String)>,
        ) -> Response {
            let key = match key(&state, &org, &slug) {
                Ok(key) => key,
                Err(response) => return response,
            };
            $target(State(state), headers, Path(key)).await
        }
    };
}
macro_rules! versioned {
    ($name:ident, $target:path) => {
        pub async fn $name(
            State(state): State<AppState>,
            headers: HeaderMap,
            Path((org, slug, version)): Path<(String, String, String)>,
        ) -> Response {
            let key = match key(&state, &org, &slug) {
                Ok(key) => key,
                Err(response) => return response,
            };
            $target(State(state), headers, Path((key, version))).await
        }
    };
}
one!(report_latest, reports::report_latest);
one!(raw_latest, reports::raw_latest);
one!(share_latest, reports::share_latest);
one!(report_download_latest, reports::report_download_latest);
one!(delete_report, reports::delete_report);
one!(report_info, reports::report_info);
one!(report_versions, reports::report_versions);
one!(unpin_latest, reports::unpin_latest);
one!(get_access, governance::get_access);
versioned!(report_version, reports::report_version);
versioned!(raw_version, reports::raw_version);
versioned!(share_version, reports::share_version);
versioned!(report_download, reports::report_download);
versioned!(delete_version, reports::delete_version);
versioned!(report_bundle, reports::report_bundle);
pub async fn regenerate(
    State(state): State<AppState>,
    headers: HeaderMap,
    admission: Option<Extension<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>>,
    Path((org, slug, version)): Path<(String, String, String)>,
) -> Response {
    let key = match key(&state, &org, &slug) {
        Ok(key) => key,
        Err(response) => return response,
    };
    reports::regenerate(State(state), headers, admission, Path((key, version))).await
}

pub async fn report_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((org, slug, version, path)): Path<(String, String, String, String)>,
) -> Response {
    let key = match key(&state, &org, &slug) {
        Ok(key) => key,
        Err(response) => return response,
    };
    reports::report_file(State(state), headers, Path((key, version, path))).await
}

pub async fn pin_latest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((org, slug)): Path<(String, String)>,
    payload: Result<Json<Value>, JsonRejection>,
) -> Response {
    let key = match key(&state, &org, &slug) {
        Ok(key) => key,
        Err(response) => return response,
    };
    reports::pin_latest(State(state), headers, Path(key), payload).await
}

pub async fn put_access(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((org, slug)): Path<(String, String)>,
    payload: Result<Json<governance::AccessChange>, JsonRejection>,
) -> Response {
    let key = match key(&state, &org, &slug) {
        Ok(key) => key,
        Err(response) => return response,
    };
    governance::put_access(State(state), headers, Path(key), payload).await
}
