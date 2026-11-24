//! Publishing, reading and owner actions on stored reports. Every handler asks `auth` first, so a
//! route's access rule is visible at its top (TYP-70, FEA-SF-64).
use super::{
    AppState, api_error,
    auth::{self, Caller, authorize, mutating},
};
use crate::{
    access::{Principal, Right, Visibility},
    resolver,
    storage::{PublishError, StoredReport, validate_slug, validate_version},
};
use axum::{
    Json,
    extract::{Extension, Path as AxumPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use richview_core::document::RenderOptions;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RenderRequest {
    pub source: String,
    #[serde(default)]
    pub report_folder: Option<richview_core::document::ReportFolder>,
    pub styles: Vec<String>,
    pub data: BTreeMap<String, String>,
    pub width: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishRequest {
    pub slug: String,
    /// Selected organization for a browser session; token callers use their token's organization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    pub source: String,
    #[serde(default)]
    pub report_folder: Option<richview_core::document::ReportFolder>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub data: BTreeMap<String, String>,
    /// A client revising a previously published file sends false to require an existing address.
    #[serde(default = "allow_new_address")]
    pub new: bool,
    /// The files the source links to, base64 by report-relative path (FEA-SF-52).
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    /// Who can open the report: `private`, `org` or `public`. Left as it is when absent; a new
    /// report starts with the server's default. Changing it needs the `share` right.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
}

fn allow_new_address() -> bool {
    true
}

impl PublishRequest {
    /// Drops every macOS sidecar (`._<name>`) the publication lists or carries. It is another
    /// file's attributes, not a file of the report (TYP-39), so this server stores and serves
    /// none, whatever client built the request.
    fn drop_sidecars(&mut self) {
        use richview_core::document::apple_double_sidecar as sidecar;
        if let Some(folder) = self.report_folder.as_mut() {
            folder.files.retain(|path| !sidecar(path));
        }
        self.data.retain(|path, _| !sidecar(path));
        self.files.retain(|path, _| !sidecar(path));
    }
}

/// What `validate` and `check` receive over HTTP: the report as the folder holds it, nothing more.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckRequest {
    pub source: String,
    #[serde(default)]
    pub report_folder: Option<richview_core::document::ReportFolder>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub data: BTreeMap<String, String>,
    /// `validate` only: also apply the publish rules.
    #[serde(default)]
    pub publish: bool,
    /// `check` only: the widths figures occupy for readers.
    #[serde(default)]
    pub widths: Vec<f64>,
}

fn outcome_response(outcome: crate::ops::Outcome) -> Response {
    match outcome {
        Ok(value) => Json(value).into_response(),
        Err(failure) => {
            let status = match failure {
                crate::ops::Failure::Usage(_) => StatusCode::BAD_REQUEST,
                crate::ops::Failure::Diagnostics(_) => StatusCode::UNPROCESSABLE_ENTITY,
                crate::ops::Failure::Host { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, Json(failure.to_json())).into_response()
        }
    }
}

/// The caller of a stateless operation: someone who may publish, so the work is not offered to
/// the open web. Nothing is stored.
fn operator(state: &AppState, headers: &HeaderMap) -> Result<Caller, Response> {
    let caller = mutating(state, headers)?;
    if !state.access.can_create(&caller.principal) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "Your role does not include the right to publish.",
        ));
    }
    Ok(caller)
}

/// `POST /api/validate`: the same result as `richview validate --format json`, for a report sent
/// as text with its folder inventory (ARC-08). Stores nothing.
pub async fn validate_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    admission: Option<Extension<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>>,
    payload: Result<Json<CheckRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Err(response) = operator(&state, &headers) {
        return response;
    }
    let Json(request) = match payload {
        Ok(p) => p,
        Err(e) => return api_error(e.status(), "Invalid validate JSON"),
    };
    if request.source.len() > richview_core::document::MAX_SOURCE_BYTES {
        return api_error(StatusCode::PAYLOAD_TOO_LARGE, "Source exceeds 16 MiB");
    }
    match tokio::task::spawn_blocking(move || {
        let _admission = admission;
        crate::ops::validate_text(
            &request.source,
            request.report_folder.as_ref(),
            request.publish,
        )
    })
    .await
    {
        Ok(outcome) => outcome_response(outcome),
        Err(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Validation worker failed",
        ),
    }
}

/// `POST /api/check`: the same result as `richview check --format json`. Stores nothing.
pub async fn check_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    admission: Option<Extension<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>>,
    payload: Result<Json<CheckRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if let Err(response) = operator(&state, &headers) {
        return response;
    }
    let Json(request) = match payload {
        Ok(p) => p,
        Err(e) => return api_error(e.status(), "Invalid check JSON"),
    };
    let render = RenderRequest {
        source: request.source.clone(),
        report_folder: request.report_folder.clone(),
        styles: request.styles.clone(),
        data: request.data.clone(),
        width: None,
    };
    let widths = request.widths.clone();
    let folder = request.report_folder.clone();
    match tokio::task::spawn_blocking(move || {
        let _admission = admission;
        let options = options_for(&state, &render).map_err(|message| {
            // Whatever is wrong with the request's own data or styles is the caller's to fix.
            match crate::ops::Failure::from(message) {
                crate::ops::Failure::Host { message, .. } => crate::ops::Failure::usage(message),
                other => other,
            }
        })?;
        crate::ops::check_text(&request.source, options, folder.as_ref(), &widths)
    })
    .await
    {
        Ok(outcome) => outcome_response(outcome),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Check worker failed"),
    }
}

/// A font travels as a `data:font/woff2;base64,` URI that reaches the page's style sheet, so the
/// server accepts only that exact form: the base64 alphabet and the WOFF2 magic bytes. Anything
/// else could close the style sheet and carry markup or a redirect to every reader.
fn validate_data(data: &BTreeMap<String, String>) -> Result<(), String> {
    use base64::Engine as _;
    for (name, value) in data {
        if !value.starts_with("data:font/") {
            continue;
        }
        let ok = value
            .strip_prefix("data:font/woff2;base64,")
            .is_some_and(|body| {
                body.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
                    && base64::engine::general_purpose::STANDARD
                        .decode(body)
                        .is_ok_and(|bytes| bytes.starts_with(b"wOF2"))
            });
        if !ok {
            return Err(format!(
                "Data '{name}' is not a WOFF2 font: fonts must be data:font/woff2;base64, followed by a WOFF2 file"
            ));
        }
    }
    Ok(())
}

pub(super) fn options_for(
    _state: &AppState,
    request: &RenderRequest,
) -> Result<RenderOptions, String> {
    if request.source.len() > richview_core::document::MAX_SOURCE_BYTES {
        return Err("Source exceeds 16 MiB".into());
    }
    validate_data(&request.data)?;
    let width = request.width.unwrap_or(720.0);
    if !width.is_finite() || !(240.0..=4096.0).contains(&width) {
        return Err("Chart width must be finite and between 240 and 4096 pixels".into());
    }
    let options = RenderOptions {
        styles: request.styles.clone(),
        report_folder: request.report_folder.clone(),
        data: request.data.clone(),
        width,
        ..Default::default()
    };
    Ok(options)
}

/// Stored reports link the runtime from this server instead of inlining it, so each one stays
/// small; the runtime is served once from a content-hashed, immutable route (INT-04, FEA-SF-20).
fn link_runtime(state: &AppState, options: &mut RenderOptions) -> Result<(), String> {
    let runtime = crate::runtime::require()?;
    if runtime.js.is_empty() {
        return Err("The report runtime bindings are empty".into());
    }
    let id = &runtime.sha256[..16];
    let wasm = crate::runtime::wasm_bytes()?;
    state.storage.keep_runtime(id, &runtime.js, &wasm)?;
    // The renderer hashes the same bytes that the immutable route serves.
    options.runtime_js = Some(runtime.js.clone());
    options.runtime_wasm = None;
    options.runtime_url = Some(format!("/_rv/{id}"));
    Ok(())
}

pub(super) fn diagnostic_response(
    status: StatusCode,
    diagnostic: richview_core::document::Diagnostic,
) -> Response {
    (
        status,
        Json(super::error_body(diagnostic.message.clone(), [diagnostic])),
    )
        .into_response()
}

/// The next patch release after a release version (`1.4.2` → `1.4.3`), for a fix that names
/// the version to write; a pre-release or an unparsable version gets none.
fn next_patch(version: &str) -> Option<String> {
    let version = semver::Version::parse(version.trim()).ok()?;
    (version.pre.is_empty() && version.build.is_empty())
        .then(|| format!("{}.{}.{}", version.major, version.minor, version.patch + 1))
}

/// Map a refused publication to its status and diagnostic (TYP-63, TYP-64).
fn publish_error_response(error: &PublishError, slug: &str, version: &str) -> Response {
    let path = "frontmatter.version";
    match error {
        PublishError::Conflict { stored_sha256 } => diagnostic_response(
            StatusCode::CONFLICT,
            richview_core::document::Diagnostic::new(
                "error",
                "publish.version-exists",
                format!("{slug} {version} is already published with different content. Bump `version` in the frontmatter and publish again."),
                path,
            )
            .fix(next_patch(version).map_or_else(
                || "Raise the version, for example to the next patch number".to_owned(),
                |next| format!("Raise the version, for example to `version: {next}`"),
            ))
            .with_details(json!({"storedContentSha256": stored_sha256})),
        ),
        PublishError::Quota { limit, used, needed } | PublishError::AccountQuota { limit, used, needed } => diagnostic_response(
            StatusCode::INSUFFICIENT_STORAGE,
            richview_core::document::Diagnostic::new("error", "publish.quota", error.to_string(), "source")
                .with_details(json!({"quotaBytes": limit, "usedBytes": used, "neededBytes": needed})),
        ),
        PublishError::OrganizationQuota { limit, used, needed } => diagnostic_response(
            StatusCode::INSUFFICIENT_STORAGE,
            richview_core::document::Diagnostic::new("error", "publish.quota", error.to_string(), "source")
                .with_details(json!({"scope":"organization", "quotaBytes":limit, "usedBytes":used, "neededBytes":needed})),
        ),
        PublishError::ReportLimit { limit } => diagnostic_response(
            StatusCode::INSUFFICIENT_STORAGE,
            richview_core::document::Diagnostic::new("error", "publish.quota", error.to_string(), "slug")
                .fix("Delete a report to free a report slot.")
                .with_details(json!({"scope":"organization-reports", "limit":limit})),
        ),
        PublishError::Deleted => diagnostic_response(
            StatusCode::CONFLICT,
            richview_core::document::Diagnostic::new(
                "error",
                "publish.version-deleted",
                format!("{slug} {version} was deleted and its number can never be published again. Publish the content under a new version."),
                path,
            ),
        ),
        PublishError::ReportDeleted => diagnostic_response(
            StatusCode::CONFLICT,
            richview_core::document::Diagnostic::new("error", "publish.version-deleted", error.to_string(), "slug"),
        ),
        PublishError::Other(message) => api_error(StatusCode::BAD_REQUEST, message.clone()),
    }
}

fn request_origin(state: &AppState, headers: &HeaderMap) -> String {
    if let Some(origin) = &state.public_origin {
        return origin.trim_end_matches('/').to_string();
    }
    // Proxy headers are not trusted; this host is already the current request authority.
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .filter(|s| {
            s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".:-[]".contains(&b))
        })
        .unwrap_or("localhost");
    format!("http://{host}")
}

fn audit(state: &AppState, caller: &Caller, action: &str, target: &str) {
    let _ = state.access.audit(&caller.principal, action, target, true);
}

/// `POST /api/reports`: publish a version. A new address needs `create`; a stored
/// address needs `edit` (FEA-SF-51, FEA-SF-19).
pub async fn publish_api(
    State(state): State<AppState>,
    headers: HeaderMap,
    admission: Option<Extension<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>>,
    payload: Result<Json<PublishRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let admission = admission.map(|Extension(permit)| permit);
    if state.access.is_hosted() && admission.is_none() {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Publish admission is unavailable.",
        );
    }
    let caller = match mutating(&state, &headers) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    // The right to publish is checked before the body is read: a refusal never depends on it.
    if !state.access.may_publish_anywhere(&caller.principal) {
        return api_error(
            StatusCode::FORBIDDEN,
            "Your role does not include the right to publish.",
        );
    }
    let Json(mut request) = match payload {
        Ok(p) => p,
        Err(e) => return api_error(e.status(), "Invalid publish JSON"),
    };
    if request.source.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "Missing slug or source");
    }
    request.drop_sidecars();
    if let Err(problem) = validate_slug(&request.slug) {
        return diagnostic_response(
            StatusCode::BAD_REQUEST,
            richview_core::document::Diagnostic::new("error", "publish.slug", problem, "slug")
                .expected(richview_core::SLUG_GRAMMAR)
                .fix("Send a slug that matches the grammar, or omit it from the client so it is derived from the title"),
        );
    }
    if state.access.is_hosted() {
        let Principal::User(user) = &caller.principal else {
            return auth::not_found();
        };
        let org = match (&user.token_org, &request.org) {
            (Some(bound), Some(requested)) if bound != requested => return auth::not_found(),
            (Some(bound), _) => bound.as_str(),
            (None, Some(selected)) => selected.as_str(),
            (None, None) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    "Select an organization to publish.",
                );
            }
        };
        let key = match crate::storage::ReportKey::hosted(org, &request.slug) {
            Ok(key) => key.as_str().to_owned(),
            Err(_) => return auth::not_found(),
        };
        if !state.access.report_org_active(&key)
            || (!matches!(
                state.access.role_in(&user.id, org),
                Some(
                    crate::access::OrgRole::Owner
                        | crate::access::OrgRole::Admin
                        | crate::access::OrgRole::Author
                )
            ) && !state.access.can(&caller.principal, &key, Right::Edit))
        {
            return auth::not_found();
        }
        request.slug = key;
    }
    if let Err(error) = state.access.admit_hosted_publish(&request.slug) {
        return match error {
            crate::access::AccessError::Throttled => api_error(
                StatusCode::TOO_MANY_REQUESTS,
                "This organization has reached its 60 publish attempts per hour limit.",
            ),
            _ => api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not check the publish limit.",
            ),
        };
    }
    let parsed = match richview_core::document::parse(&request.source) {
        Ok(p) => p,
        Err(d) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(super::error_body("Invalid source", d)),
            )
                .into_response();
        }
    };
    // The slug was chosen by the client; only the metadata rules are checked again here.
    let problems: Vec<_> = richview_core::document::validate_publish(&request.source)
        .into_iter()
        .filter(|d| d.level == "error" && d.code != "publish.slug")
        .collect();
    if !problems.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(super::error_body(problems[0].message.clone(), problems)),
        )
            .into_response();
    }
    let version = parsed
        .frontmatter
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if validate_version(&version).is_err() {
        return api_error(
            StatusCode::BAD_REQUEST,
            "Publish requires frontmatter version (semver, e.g. 0.0.1)",
        );
    }
    // A new slug's authorization, storage and ownership claim must be one operation with
    // respect to other publishers. Otherwise two authors can both create the same address.
    let publication = state.access.publication.clone().lock_owned().await;
    // A token may have been revoked or its organization suspended while this request waited.
    let caller = if state.access.is_hosted() {
        let caller = auth::caller(&state, &headers);
        if matches!(caller.principal, Principal::Anonymous) {
            return auth::unauthorized();
        }
        if !state.access.report_org_active(&request.slug) {
            return auth::not_found();
        }
        caller
    } else {
        caller
    };
    let known = match state.storage.has_slug(&request.slug) {
        Ok(known) => known,
        Err(e) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    if known {
        if let Err(response) = authorize(&state, &caller, &request.slug, Right::Edit) {
            return response;
        }
        // An editor publishes versions; who can open the report stays with those who may share it.
        if request
            .visibility
            .is_some_and(|wanted| wanted != state.access.acl(&request.slug).visibility)
            && let Err(response) = authorize(&state, &caller, &request.slug, Right::Share)
        {
            return response;
        }
    } else {
        let may_create = if state.access.is_hosted() {
            match &caller.principal {
                Principal::User(user) => request.slug.split_once('/').is_some_and(|(org, _)| {
                    matches!(
                        state.access.role_in(&user.id, org),
                        Some(
                            crate::access::OrgRole::Owner
                                | crate::access::OrgRole::Admin
                                | crate::access::OrgRole::Author
                        )
                    )
                }),
                _ => false,
            }
        } else {
            state.access.can_create(&caller.principal)
        };
        if !may_create {
            let _ = state
                .access
                .audit(&caller.principal, "denied.create", &request.slug, false);
            return api_error(
                StatusCode::FORBIDDEN,
                "Your role cannot publish new reports.",
            );
        }
        if !request.new {
            return diagnostic_response(
                StatusCode::CONFLICT,
                richview_core::document::Diagnostic::new(
                    "error",
                    "publish.new-slug",
                    format!(
                        "`{}` does not exist on this server and this request expected an existing report. Nothing was stored.",
                        request.slug
                    ),
                    "slug",
                )
                .fix("Use the previous slug, or pass --new (MCP: new: true) to create another address intentionally"),
            );
        }
        // Reserve room for the retained access record before writing a first version. Publishing
        // and claims share the publication lock, so another publisher cannot take this capacity.
        if let Err(error) = state.access.check_claim_capacity(&request.slug) {
            return auth::access_error(error);
        }
    }
    // Only files the source links become public; each one must have been sent.
    let links: Vec<String> = resolver::linked_paths(&parsed)
        .into_iter()
        .filter(|p| !p.to_lowercase().ends_with(".rmd"))
        .collect();
    if let Some(missing) = links.iter().find(|p| !request.files.contains_key(*p)) {
        return diagnostic_response(
            StatusCode::BAD_REQUEST,
            richview_core::document::Diagnostic::new(
                "error",
                "report.asset.missing",
                format!(
                    "The report links to {missing}, but the publication did not carry that file."
                ),
                missing.as_str(),
            )
            .fix("Add the linked file to the report folder and publish again"),
        );
    }
    request.files.retain(|path, _| links.contains(path));
    let origin = request_origin(&state, &headers);
    let slug = request.slug.clone();
    let storage = state.storage.clone();
    let stored_version = version.clone();
    let worker_state = state.clone();
    let principal = caller.principal.clone();
    let wanted_visibility = request.visibility;
    let result = tokio::task::spawn_blocking(move || -> Result<_, Refused> {
        // A disconnected client drops its handler, not this blocking task. Keep the lock
        // and the ownership claim with the write until that task actually finishes.
        let _publication = publication;
        let _admission = admission;
        let mut options = options_for(
            &worker_state,
            &RenderRequest {
                source: request.source.clone(),
                report_folder: request.report_folder.clone(),
                styles: request.styles.clone(),
                data: request.data.clone(),
                ..Default::default()
            },
        )?;
        link_runtime(&worker_state, &mut options)?;
        // HTML/frontmatter supplied by a client is deliberately not trusted or stored.
        let frontmatter = parsed.frontmatter.clone();
        let rendered = richview_core::document::render_document(parsed, &options)
            .map_err(|d| PublishError::from(serde_json::to_string(&d).unwrap_or_default()))?;
        if rendered.diagnostics.iter().any(|d| d.level == "error") {
            return Err(PublishError::from(
                serde_json::to_string(&rendered.diagnostics).unwrap_or_default(),
            )
            .into());
        }
        // Links to report files point at the version's own file route, so they work from the
        // latest address and the permalink alike (FEA-SF-52).
        let html = link_files(&rendered.html, &request.slug, &stored_version, &links);
        let charts = rendered.charts.clone();
        let findings = rendered.diagnostics.clone();
        let owner = if known {
            worker_state.access.acl(&request.slug).owner
        } else {
            match &principal {
                Principal::User(user) => user.id.clone(),
                Principal::Service => "service".to_owned(),
                Principal::Anonymous => {
                    return Err(
                        PublishError::from("Anonymous publishing is refused".to_owned()).into(),
                    );
                }
            }
        };
        #[cfg(test)]
        crate::test_points::reach("publish.rendered", &request.slug);
        let owned_slugs = worker_state.access.owned_slugs(&owner);
        let slug = request.slug.clone();
        let card_storage = storage.clone();
        let commit_headers = headers.clone();
        // Rendering took time. The commit reads the publisher's rights again, puts the
        // visibility or the new report's ownership in force and stores the version as one
        // step that no access change can fall into; a refusal stores nothing.
        let (stored, created) = worker_state.access.commit_publication(
            || auth::caller(&worker_state, &commit_headers).principal,
            &slug,
            known,
            wanted_visibility,
            || {
                storage.put_version_for_owner(
                    StoredReport {
                        slug: request.slug,
                        version: stored_version,
                        source: request.source,
                        // Only the folder's name is kept: the author's local path stays with
                        // the author (TYP-65).
                        report_folder: request.report_folder.map(|mut folder| {
                            folder.root =
                                std::path::Path::new(&folder.root).file_name().map_or_else(
                                    || "report".to_owned(),
                                    |n| n.to_string_lossy().into_owned(),
                                );
                            folder
                        }),
                        html,
                        frontmatter,
                        created_at: crate::time::Instant::now().millis(),
                        styles: request.styles,
                        data: request.data,
                        files: request.files,
                        content_sha256: String::new(),
                    },
                    &owned_slugs,
                )
            },
        )??;
        // The card is drawn once, when the version is stored, so a link preview never renders
        // anything (ARC-69); after the commit, so no access change waits for a drawing. A
        // failure leaves it to be drawn on first request.
        if created && let Ok(png) = draw_share_card(&stored, &charts) {
            let _ = card_storage.put_share_card(&stored.slug, &stored.version, &png);
        }
        let _ = worker_state.access.audit(
            &principal,
            "report.publish",
            &format!("{}@{}", stored.slug, stored.version),
            true,
        );
        let claim: Result<(), crate::access::AccessError> = Ok(());
        Ok((stored, created, findings, claim))
    })
    .await;
    match result {
        Ok(Ok((report, created, findings, claim))) => {
            if let Err(error) = claim {
                return auth::access_error(error);
            }
            let visibility = state.access.acl(&slug).visibility;
            // The server's own render can warn even when the client's check was clean: say so
            // (TYP-54), in the same result object as every other operation.
            Json(richview_core::document::operation_result(findings, json!({"slug": slug, "version": report.version, "url": format!("{origin}/r/{slug}"), "created": created, "permalink": format!("{origin}/r/{slug}/v/{}", report.version), "contentSha256": report.content_sha256, "visibility": visibility})))
            .into_response()
        }
        Ok(Err(Refused::Access(error))) => auth::access_error(error),
        Ok(Err(Refused::Publish(PublishError::Other(e)))) => api_error(StatusCode::BAD_REQUEST, e),
        Ok(Err(Refused::Publish(e))) => publish_error_response(&e, &slug, &version),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Publish worker failed"),
    }
}

/// Why a publication stored nothing: the store refused it, or the publisher's rights changed
/// while it was rendered.
enum Refused {
    Publish(PublishError),
    Access(crate::access::AccessError),
}

impl From<PublishError> for Refused {
    fn from(error: PublishError) -> Self {
        Self::Publish(error)
    }
}

impl From<String> for Refused {
    fn from(message: String) -> Self {
        Self::Publish(message.into())
    }
}

impl From<crate::access::AccessError> for Refused {
    fn from(error: crate::access::AccessError) -> Self {
        Self::Access(error)
    }
}

/// `DELETE /api/reports/{slug}/v/{version}`: needs `delete`; leaves a tombstone.
/// Deletes every version of a report while preserving its retired address. The caller is the
/// owner or an authorized administrator of that report's organization; editors cannot delete it.
pub async fn delete_owned_report(
    state: &AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<(), Response> {
    #[cfg(test)]
    crate::test_points::reach("delete.waiting", slug);
    let publication = state.access.publication.clone().lock_owned().await;
    // The wait can be long. Who is deleting is read from the request once it is over: an
    // account disabled or a session ended meanwhile deletes nothing.
    let caller = &auth::caller(state, headers);
    if !state.access.can(&caller.principal, slug, Right::Delete) {
        return Err(auth::not_found());
    }
    let storage = state.storage.clone();
    let target = slug.to_owned();
    match tokio::task::spawn_blocking(move || {
        let _publication = publication;
        storage.delete_report(&target)
    })
    .await
    {
        Ok(Ok(true)) => {
            audit(state, caller, "report.delete", slug);
            Ok(())
        }
        Ok(Ok(false)) => Err(auth::not_found()),
        _ => Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Report deletion failed",
        )),
    }
}

/// `DELETE /api/reports/{slug}`: remove the whole report and retire its address.
pub async fn delete_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let caller = match mutating(&state, &headers) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    drop(caller);
    match delete_owned_report(&state, &headers, &slug).await {
        Ok(()) => Json(json!({"ok": true, "deleted": true})).into_response(),
        Err(response) => response,
    }
}

/// `DELETE /api/reports/{slug}/v/{version}`: needs `delete`; leaves a tombstone.
pub async fn delete_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((slug, version)): AxumPath<(String, String)>,
) -> Response {
    let caller = match mutating(&state, &headers) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Err(response) = authorize(&state, &caller, &slug, Right::Delete) {
        return response;
    }
    let target = format!("{slug}@{version}");
    let storage = state.storage.clone();
    match tokio::task::spawn_blocking(move || storage.delete_version(&slug, &version)).await {
        Ok(Ok(true)) => {
            audit(&state, &caller, "report.delete", &target);
            Json(json!({"ok": true, "deleted": true})).into_response()
        }
        Ok(Ok(false)) => api_error(StatusCode::NOT_FOUND, "Not found"),
        Ok(Err(e)) => api_error(StatusCode::BAD_REQUEST, e),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage worker failed"),
    }
}

/// `PUT /api/reports/{slug}/latest`: needs `manage`.
pub async fn pin_latest(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    payload: Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let caller = match mutating(&state, &headers) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Err(response) = authorize(&state, &caller, &slug, Right::Manage) {
        return response;
    }
    let version = match payload {
        Ok(Json(body)) => body["version"].as_str().unwrap_or_default().to_owned(),
        Err(e) => return api_error(e.status(), "Invalid pin JSON"),
    };
    let pinned = version.clone();
    let target = format!("{slug}@{version}");
    let storage = state.storage.clone();
    match tokio::task::spawn_blocking(move || storage.set_pin(&slug, &version)).await {
        Ok(Ok(true)) => {
            audit(&state, &caller, "report.pin", &target);
            Json(json!({"ok": true, "pinned": pinned})).into_response()
        }
        Ok(Ok(false)) => diagnostic_response(
            StatusCode::NOT_FOUND,
            richview_core::document::Diagnostic::new(
                "error",
                "publish.version-missing",
                format!(
                    "Version {pinned} is not published for this report, so it cannot be pinned. Pin one of the listed versions."
                ),
                "version",
            ),
        ),
        Ok(Err(e)) => api_error(StatusCode::BAD_REQUEST, e),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage worker failed"),
    }
}

/// `DELETE /api/reports/{slug}/latest`: needs `manage`.
pub async fn unpin_latest(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let caller = match mutating(&state, &headers) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Err(response) = authorize(&state, &caller, &slug, Right::Manage) {
        return response;
    }
    let target = slug.clone();
    let storage = state.storage.clone();
    match tokio::task::spawn_blocking(move || storage.clear_pin(&slug)).await {
        Ok(Ok(())) => {
            audit(&state, &caller, "report.unpin", &target);
            Json(json!({"ok": true, "pinned": null})).into_response()
        }
        Ok(Err(e)) => api_error(StatusCode::BAD_REQUEST, e),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage worker failed"),
    }
}

/// `GET /api/reports`: only the reports the caller may see (FEA-SF-15).
pub async fn list_reports(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let caller = auth::caller(&state, &headers);
    // Anonymous callers cannot be named on a report, and public links are unlisted.
    if matches!(caller.principal, Principal::Anonymous) {
        return Json(Vec::<Value>::new()).into_response();
    }
    let storage = state.storage.clone();
    match tokio::task::spawn_blocking(move || storage.list()).await {
        Ok(Ok(rows)) => Json(
            rows.into_iter()
                .filter(|row| {
                    report_row_key(&state, row).is_some_and(|slug| {
                        // "Anyone with the link" is not "listed": anonymous callers get the
                        // reports they were named on, never the public ones (FEA-SF-15).
                        let same_org = state.access.is_hosted()
                            && row["org"]
                                .as_str()
                                .is_some_and(|org| state.access.member_of(&caller.principal, org));
                        let unlisted = state.access.acl(&slug).visibility == Visibility::Public
                            && !state.access.named_on(&caller.principal, &slug)
                            && !same_org;
                        !unlisted && state.access.can(&caller.principal, &slug, Right::View)
                    })
                })
                .map(|mut row| {
                    if let Some(slug) = report_row_key(&state, &row) {
                        let acl = state.access.acl(&slug);
                        row["visibility"] = json!(acl.visibility);
                        row["rights"] = json!(state.access.rights(&caller.principal, &slug));
                    }
                    row
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        _ => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage read failed"),
    }
}

fn report_row_key(state: &AppState, row: &Value) -> Option<String> {
    let slug = row["slug"].as_str()?;
    if state.access.is_hosted() {
        Some(format!("{}/{}", row["org"].as_str()?, slug))
    } else {
        Some(slug.to_owned())
    }
}

/// Hosted page data for a signed-in person; storage failures are returned, never shown as empty.
pub async fn owned_report_rows(state: &AppState, caller: &Caller) -> Result<Vec<Value>, Response> {
    let Principal::User(user) = &caller.principal else {
        return Err(auth::not_found());
    };
    if caller.session.is_none() {
        return Err(auth::not_found());
    }
    let person = user.id.clone();
    let storage = state.storage.clone();
    let rows = tokio::task::spawn_blocking(move || storage.list())
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage read failed"))?
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage read failed"))?;
    let mut owned = Vec::new();
    for mut row in rows {
        let Some(key) = report_row_key(state, &row) else {
            continue;
        };
        let acl = state.access.acl(&key);
        if acl.owner != person || !state.access.can(&caller.principal, &key, Right::View) {
            continue;
        }
        let Some(origin) = state.report_origin.as_deref() else {
            return Err(api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Report origin is not configured",
            ));
        };
        row["visibility"] = json!(acl.visibility);
        row["url"] = json!(format!("{}/r/{key}", origin.trim_end_matches('/')));
        owned.push(row);
    }
    Ok(owned)
}

/// `GET /api/reports/{slug}`: needs `view`.
pub async fn report_info(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let caller = auth::caller(&state, &headers);
    if let Err(response) = authorize(&state, &caller, &slug, Right::View) {
        return response;
    }
    let storage = state.storage.clone();
    match tokio::task::spawn_blocking(move || {
        let report = storage.summary(&slug, None)?;
        let versions = storage.list_versions(&slug)?;
        let pinned = storage.pinned(&slug)?;
        Ok::<_, String>((report, versions, pinned))
    })
    .await
    {
        Ok(Ok((Some(mut report), versions, pinned))) => {
            report["versions"] = json!(versions);
            report["pinned"] = json!(pinned);
            Json(report).into_response()
        }
        Ok(Ok(_)) => api_error(StatusCode::NOT_FOUND, "Not found"),
        _ => api_error(
            StatusCode::BAD_REQUEST,
            "Invalid slug or unavailable storage",
        ),
    }
}

/// `GET /api/reports/{slug}/versions`: needs `view`.
pub async fn report_versions(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let caller = auth::caller(&state, &headers);
    if let Err(response) = authorize(&state, &caller, &slug, Right::View) {
        return response;
    }
    let storage = state.storage.clone();
    match tokio::task::spawn_blocking(move || storage.list_versions(&slug)).await {
        Ok(Ok(v)) if state.access.is_hosted() && v.is_empty() => auth::not_found(),
        Ok(Ok(v)) => Json(v).into_response(),
        _ => api_error(
            StatusCode::BAD_REQUEST,
            "Invalid slug or unavailable storage",
        ),
    }
}

/// `GET /api/reports/{slug}/v/{version}`: the stored record, with its data files; needs `edit`.
pub async fn report_bundle(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((slug, version)): AxumPath<(String, String)>,
) -> Response {
    let caller = auth::caller(&state, &headers);
    if let Err(response) = authorize(&state, &caller, &slug, Right::Edit) {
        return response;
    }
    let storage = state.storage.clone();
    let (gone_slug, gone_version) = (slug.clone(), version.clone());
    match tokio::task::spawn_blocking(move || storage.get(&slug, Some(&version))).await {
        Ok(Ok(Some(r))) => {
            // The author's absolute folder is not part of what a client needs.
            let mut bundle = serde_json::to_value(&r).unwrap_or(Value::Null);
            if let Some(folder) = bundle
                .get_mut("reportFolder")
                .and_then(Value::as_object_mut)
            {
                folder.insert("root".into(), Value::String(String::new()));
            }
            Json(bundle).into_response()
        }
        Ok(Ok(None))
            if state
                .storage
                .is_deleted(&gone_slug, &gone_version)
                .unwrap_or(false) =>
        {
            api_error(StatusCode::GONE, "This version was deleted")
        }
        Ok(Ok(None)) => api_error(StatusCode::NOT_FOUND, "Not found"),
        _ => api_error(StatusCode::BAD_REQUEST, "Invalid report path"),
    }
}

/// Re-renders a stored version from its stored source and resources with the running engine and
/// says whether the result differs from the stored artifact. Nothing is stored (FEA-SF-16).
/// Needs `manage`.
pub async fn regenerate(
    State(state): State<AppState>,
    headers: HeaderMap,
    admission: Option<Extension<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>>,
    AxumPath((slug, version)): AxumPath<(String, String)>,
) -> Response {
    let caller = match mutating(&state, &headers) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Err(response) = authorize(&state, &caller, &slug, Right::Manage) {
        return response;
    }
    audit(
        &state,
        &caller,
        "report.regenerate",
        &format!("{slug}@{version}"),
    );
    let storage = state.storage.clone();
    let (named_slug, named_version) = (slug.clone(), version.clone());
    let load_admission = admission.clone();
    let report = match tokio::task::spawn_blocking(move || {
        let _admission = load_admission;
        storage.get(&slug, Some(&version))
    })
    .await
    {
        Ok(Ok(Some(r))) => r,
        Ok(Ok(None)) => return api_error(StatusCode::NOT_FOUND, "Not found"),
        Ok(Err(e)) => return api_error(StatusCode::BAD_REQUEST, e),
        Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage worker failed"),
    };
    // This is a POST, so it is not repeated like a read. The version was loaded after the
    // first check, so the request is authenticated again from its own headers: whoever lost
    // the right, the token or the account in between gets nothing of it, and the refusal
    // writes no second audit line and spends nothing more.
    if !state.access.can(
        &auth::caller(&state, &headers).principal,
        &named_slug,
        Right::Manage,
    ) {
        return auth::not_found();
    }
    let stored_html = report.html.clone();
    let rendered = tokio::task::spawn_blocking(move || {
        let _admission = admission;
        let mut options = options_for(
            &state,
            &RenderRequest {
                source: report.source.clone(),
                report_folder: report.report_folder,
                styles: report.styles,
                data: report.data,
                ..Default::default()
            },
        )?;
        link_runtime(&state, &mut options)?;
        let mut rendered = richview_core::document::render(&report.source, &options)
            .map_err(|d| serde_json::to_string(&d).unwrap_or_default())?;
        // The same steps as publishing, so `changed` compares like with like.
        if let Ok(document) = richview_core::document::parse(&report.source) {
            let links: Vec<String> = resolver::linked_paths(&document)
                .into_iter()
                .filter(|p| !p.to_lowercase().ends_with(".rmd"))
                .collect();
            rendered.html = link_files(&rendered.html, &named_slug, &named_version, &links);
        }
        Ok::<_, String>(rendered)
    })
    .await;
    match rendered {
        Ok(Ok(report)) => Json(richview_core::document::operation_result(
            report.diagnostics,
            json!({"changed": report.html != stored_html, "html": report.html, "metadata": report.metadata}),
        ))
        .into_response(),
        Ok(Err(e)) => api_error(StatusCode::UNPROCESSABLE_ENTITY, e),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Render worker failed"),
    }
}

/// Points every link to a report file at that version's file route.
fn link_files(html: &str, slug: &str, version: &str, links: &[String]) -> String {
    let mut html = html.to_owned();
    for path in links {
        // The rendered href is the file name percent-encoded; a hand-written one may be raw.
        let encoded: String = path
            .bytes()
            .map(|b| match b {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect();
        let raw = path
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");
        for spelling in [raw, encoded.clone()] {
            html = html.replace(
                &format!("href=\"{spelling}\""),
                &format!("href=\"/r/{slug}/v/{version}/files/{encoded}\""),
            );
        }
    }
    html
}

/// Cache headers that never let a shared cache keep a report that is not public.
fn cache_control(public: bool, immutable: bool) -> &'static str {
    match (public, immutable) {
        (true, true) => "public, max-age=31536000, immutable",
        (true, false) => "no-cache",
        (false, true) | (false, false) => "private, no-store",
    }
}

fn is_public(state: &AppState, slug: &str) -> bool {
    state.access.acl(slug).visibility == Visibility::Public
}

/// `GET /r/{slug}/v/{version}/files/{path}`: a file the report links to, read-only and never run:
/// offered as a download, with a type only for formats that cannot carry script (FEA-SF-52).
pub async fn report_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((slug, version, path)): AxumPath<(String, String, String)>,
) -> Response {
    if state.access.is_hosted() && !state.access.report_org_active(&slug) {
        return auth::not_found();
    }
    let caller = auth::reader(&state, &headers, &slug);
    if let Err(response) = authorize(&state, &caller, &slug, Right::View) {
        return response;
    }
    let permit = if state.access.is_hosted() {
        match hosted_download_workers().clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => {
                return api_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Report download is busy; try again shortly",
                );
            }
        }
    } else {
        None
    };
    let public = is_public(&state, &slug);
    let storage = state.storage.clone();
    let gone = (slug.clone(), version.clone());
    let wanted = path.clone();
    match tokio::task::spawn_blocking(move || (storage.read_file(&slug, &version, &wanted), permit))
        .await
    {
        Ok((Ok(Some(bytes)), permit)) => {
            let mime = match path.rsplit('.').next().map(str::to_lowercase).as_deref() {
                Some("csv") => "text/csv; charset=utf-8",
                Some("json") => "application/json",
                Some("txt" | "md") => "text/plain; charset=utf-8",
                Some("pdf") => "application/pdf",
                Some("png") => "image/png",
                Some("jpg" | "jpeg") => "image/jpeg",
                Some("gif") => "image/gif",
                Some("webp") => "image/webp",
                _ => "application/octet-stream",
            };
            let name = path.rsplit('/').next().unwrap_or("file").replace('"', "");
            (
                [
                    (header::CONTENT_TYPE, mime.to_owned()),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{name}\""),
                    ),
                    (
                        header::CACHE_CONTROL,
                        cache_control(public, true).to_owned(),
                    ),
                    (
                        header::CONTENT_SECURITY_POLICY,
                        "sandbox; default-src 'none'".to_owned(),
                    ),
                ],
                bytes::Bytes::from_owner(GuardedBytes {
                    body: bytes,
                    _permit: permit,
                }),
            )
                .into_response()
        }
        Ok((Ok(None), _)) => match state.storage.is_deleted(&gone.0, &gone.1) {
            Ok(true) => api_error(StatusCode::GONE, "This version was deleted"),
            _ => api_error(StatusCode::NOT_FOUND, "Not found"),
        },
        Ok((Err(e), _)) => api_error(StatusCode::BAD_REQUEST, e),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage worker failed"),
    }
}

/// `GET /_rv/{name}`: a kept runtime file, immutable for ever under its content id. Public.
pub async fn runtime_file(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Response {
    let mime = if name.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else {
        "application/wasm"
    };
    match tokio::task::spawn_blocking(move || state.storage.runtime_file(&name)).await {
        Ok(Ok(Some(bytes))) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                // Reports may run in an opaque origin, which loads the runtime across origins.
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            bytes,
        )
            .into_response(),
        Ok(Ok(None)) => api_error(StatusCode::NOT_FOUND, "Not found"),
        Ok(Err(e)) => api_error(StatusCode::BAD_REQUEST, e),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage worker failed"),
    }
}

/// `GET /r/{slug}/download`: the latest version's self-contained file; needs `view`.
pub async fn report_download_latest(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    download(state, headers, slug, None).await
}

/// `GET /r/{slug}/v/{version}/download`: the report as one self-contained file; needs `view`.
pub async fn report_download(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((slug, version)): AxumPath<(String, String)>,
) -> Response {
    download(state, headers, slug, Some(version)).await
}

fn hosted_download_workers() -> &'static std::sync::Arc<tokio::sync::Semaphore> {
    static WORKERS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    WORKERS.get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(2)))
}

enum DownloadFailure {
    NotFound,
    Storage(String),
    Render(String),
}

async fn download(
    state: AppState,
    headers: HeaderMap,
    slug: String,
    version: Option<String>,
) -> Response {
    let caller = auth::reader(&state, &headers, &slug);
    if let Err(response) = authorize(&state, &caller, &slug, Right::View) {
        return response;
    }
    // A disconnected request cannot release its permit until its blocking load and render finish.
    let permit = if state.access.is_hosted() {
        match hosted_download_workers().clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => {
                return api_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Report download is busy; try again shortly",
                );
            }
        }
    } else {
        None
    };
    let public = is_public(&state, &slug);
    let name_slug = slug.rsplit('/').next().unwrap_or(&slug).to_owned();
    let result = tokio::task::spawn_blocking(move || {
        let result = (|| -> Result<(String, String), DownloadFailure> {
            let version = match version {
                Some(version) => version,
                None => state
                    .storage
                    .latest_version(&slug)
                    .map_err(DownloadFailure::Storage)?
                    .ok_or(DownloadFailure::NotFound)?,
            };
            let report = state
                .storage
                .get(&slug, Some(&version))
                .map_err(DownloadFailure::Storage)?
                .ok_or(DownloadFailure::NotFound)?;
            let mut options = options_for(
                &state,
                &RenderRequest {
                    source: report.source.clone(),
                    report_folder: report.report_folder,
                    styles: report.styles,
                    data: report.data,
                    ..Default::default()
                },
            )
            .map_err(DownloadFailure::Render)?;
            resolver::load_runtime(&mut options).map_err(DownloadFailure::Render)?;
            let rendered =
                richview_core::document::render(&report.source, &options).map_err(|d| {
                    DownloadFailure::Render(serde_json::to_string(&d).unwrap_or_default())
                })?;
            Ok((version, rendered.html))
        })();
        (result, permit)
    })
    .await;
    match result {
        Ok((Ok((version, html)), permit)) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8".to_owned()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{name_slug}-{version}.html\""),
                ),
                (
                    header::CACHE_CONTROL,
                    cache_control(public, true).to_owned(),
                ),
            ],
            bytes::Bytes::from_owner(GuardedBytes {
                body: html.into_bytes(),
                _permit: permit,
            }),
        )
            .into_response(),
        Ok((Err(DownloadFailure::NotFound), _)) => api_error(StatusCode::NOT_FOUND, "Not found"),
        Ok((Err(DownloadFailure::Storage(error)), _)) => api_error(StatusCode::BAD_REQUEST, error),
        Ok((Err(DownloadFailure::Render(error)), _)) => {
            api_error(StatusCode::UNPROCESSABLE_ENTITY, error)
        }
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Download worker failed"),
    }
}

// Keep the permit with the bytes until the response body (and any clones) is released.
struct GuardedBytes {
    body: Vec<u8>,
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl AsRef<[u8]> for GuardedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.body
    }
}

fn hosted_page_workers() -> &'static std::sync::Arc<tokio::sync::Semaphore> {
    static WORKERS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    WORKERS.get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(4)))
}

fn hosted_page_waiters() -> &'static std::sync::Arc<tokio::sync::Semaphore> {
    static WAITERS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    WAITERS.get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(32)))
}

async fn admit_hosted_page() -> Option<tokio::sync::OwnedSemaphorePermit> {
    // Waiting requests have not read a page. The permit bounds that queue and is released on
    // admission, timeout, or cancellation, including when a reader disconnects while waiting.
    let _waiting = hosted_page_waiters().clone().try_acquire_owned().ok()?;
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        hosted_page_workers().clone().acquire_owned(),
    )
    .await
    .ok()?
    .ok()
}

async fn fetch_report(
    state: AppState,
    headers: HeaderMap,
    slug: String,
    version: Option<String>,
    raw: bool,
) -> Response {
    let caller = auth::reader(&state, &headers, &slug);
    if let Err(response) = authorize(&state, &caller, &slug, Right::View) {
        // On the report origin, a reader with no session there is sent once to the app, which
        // signs them in or hands them over with a ticket; a report that is private, missing or
        // deleted answers the same (ARC-15, FEA-SF-15).
        if !raw
            && matches!(caller.principal, Principal::Anonymous)
            && super::origin::on_report_origin(&state, &headers)
            && let Some(app) = state.public_origin.as_deref()
        {
            let path = match &version {
                Some(version) => format!("/r/{slug}/v/{version}"),
                None => format!("/r/{slug}"),
            };
            return (
                StatusCode::SEE_OTHER,
                [
                    (
                        header::LOCATION,
                        format!("{}{path}?rv=signin", app.trim_end_matches('/')),
                    ),
                    (header::CACHE_CONTROL, "no-store".to_owned()),
                ],
            )
                .into_response();
        }
        return response;
    }
    let permit = if state.access.is_hosted() {
        match admit_hosted_page().await {
            Some(permit) => Some(permit),
            None if !raw => return super::ui::report_busy_page(),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    [(header::RETRY_AFTER, "1")],
                    "Report service is busy; try again shortly",
                )
                    .into_response();
            }
        }
    } else {
        None
    };
    let public = is_public(&state, &slug);
    let immutable = version.is_some();
    let storage = state.storage.clone();
    let origin = state
        .report_origin
        .clone()
        .or_else(|| state.public_origin.clone());
    let (card_slug, card_version) = (
        slug.clone(),
        match &version {
            Some(version) => Some(version.clone()),
            // The card of the version served now, so a preview never shows a later one.
            None if !raw => state.storage.latest_version(&slug).ok().flatten(),
            None => None,
        },
    );
    let gone = version.clone().map(|v| (slug.clone(), v));
    // The preview image's alt text is the story's title and byline.
    let alt = if public && !raw {
        let summary = storage
            .summary(&card_slug, card_version.as_deref())
            .ok()
            .flatten();
        let text = |key: &str| {
            summary
                .as_ref()
                .and_then(|s| s["frontmatter"][key].as_str())
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>()
        };
        let title = Some(text("title")).filter(|t| !t.is_empty());
        richview_share::alt_text(title.as_deref().unwrap_or(&card_slug), &text("author"))
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    } else {
        String::new()
    };
    // The bar above a hosted report, on the report host only; a public report's is everyone's.
    let bar = if raw || !super::origin::on_report_origin(&state, &headers) {
        None
    } else {
        super::host_bar::bar(&state, &caller.principal, &slug, public)
    };
    match tokio::task::spawn_blocking(move || {
        let result = state.storage.read_content(&slug, version.as_deref(), raw);
        (result, permit)
    })
    .await
    {
        Ok((Ok(Some(body)), permit)) => {
            let mime = if raw {
                "text/markdown; charset=utf-8"
            } else {
                "text/html; charset=utf-8"
            };
            // Link previews fetch the page anonymously, so only a public report names its card.
            let body = match (&origin, raw, public) {
                (Some(origin), false, true) => {
                    with_share_tags(&body, origin, &card_slug, card_version.as_deref(), &alt)
                }
                _ => body,
            };
            let body = match &bar {
                Some(bar) => super::host_bar::insert(&body, bar),
                None => body,
            };
            (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, cache_control(public, immutable)),
                    (header::VARY, "Cookie, Authorization"),
                ],
                bytes::Bytes::from_owner(GuardedBytes {
                    body: body.into_bytes(),
                    _permit: permit,
                }),
            )
                .into_response()
        }
        Ok((Ok(None), _))
            if gone.is_some_and(|(s, v)| storage.is_deleted(&s, &v).unwrap_or(false)) =>
        {
            (StatusCode::GONE, "This version was deleted").into_response()
        }
        Ok((Ok(None), _)) => (StatusCode::NOT_FOUND, "Not found").into_response(),
        _ => api_error(StatusCode::BAD_REQUEST, "Invalid report path"),
    }
}

/// The link-preview tags of a public report: its share card as `og:image` and a large Twitter card
/// (FEA-RP-51). The slug and version are grammar-checked, so the URL needs no escaping.
fn with_share_tags(
    html: &str,
    origin: &str,
    slug: &str,
    version: Option<&str>,
    alt: &str,
) -> String {
    let path = match version {
        Some(version) => format!("/r/{slug}/v/{version}/share.png"),
        None => format!("/r/{slug}/share.png"),
    };
    let url = format!("{}{path}", origin.trim_end_matches('/'));
    html.replacen(
        "</head>",
        &format!(
            "<meta property=\"og:image\" content=\"{url}\"><meta property=\"og:image:width\" content=\"1200\"><meta property=\"og:image:height\" content=\"630\"><meta property=\"og:image:alt\" content=\"{alt}\"><meta name=\"twitter:card\" content=\"summary_large_image\"><meta name=\"twitter:image\" content=\"{url}\"></head>"
        ),
        1,
    )
}

/// Draws a stored version's card from its title and byline in its own style (STY-70), over its
/// first chart when the render's chart payloads are at hand.
pub(super) fn draw_share_card(
    report: &StoredReport,
    charts: &[serde_json::Value],
) -> Result<Vec<u8>, String> {
    let (title, byline, style, company) = card_text(report);
    richview_share::card_png_with_company(&title, &byline, &style, charts, company)
}

/// The same card as [`draw_share_card`], as SVG, for drawing small.
pub(super) fn share_card_svg(
    report: &StoredReport,
    charts: &[serde_json::Value],
) -> Result<String, String> {
    let (title, byline, style, company) = card_text(report);
    richview_share::card_svg_with_company(&title, &byline, &style, charts, company)
}

/// A stored version's card text, style and company brand.
fn card_text(
    report: &StoredReport,
) -> (String, String, String, Option<richview_core::CompanyBrand>) {
    // The text is bounded before it is laid out: a card is 1200 x 630 and holds a few lines.
    let text = |key: &str, cap: usize| {
        report.frontmatter[key]
            .as_str()
            .map(|t| t.chars().take(cap).collect::<String>())
    };
    // The story's own style; a missing or custom one draws Sleek.
    let style = report.frontmatter["styles"][0]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let options = RenderOptions {
        styles: report.styles.clone(),
        report_folder: report.report_folder.clone(),
        data: report.data.clone(),
        ..Default::default()
    };
    let company = richview_core::report_company_brand(&report.frontmatter, &options);
    let style = company.base.as_deref().unwrap_or(&style).to_owned();
    (
        text("title", 200).unwrap_or_else(|| report.slug.clone()),
        text("author", 120).unwrap_or_default(),
        style,
        (company.logo.is_some() || company.accent.is_some()).then_some(company),
    )
}

/// The chart payloads of a stored version, from its stored source, folder inventory, data and
/// styles; none when it does not render.
pub(super) fn charts_of(report: &StoredReport) -> Vec<serde_json::Value> {
    let options = RenderOptions {
        styles: report.styles.clone(),
        report_folder: report.report_folder.clone(),
        data: report.data.clone(),
        ..Default::default()
    };
    richview_core::document::render(&report.source, &options)
        .map(|rendered| rendered.charts)
        .unwrap_or_default()
}

/// The most recent values by key, up to a capacity; past it the oldest go first.
pub(super) struct Recent<K, V> {
    capacity: usize,
    entries: std::collections::HashMap<K, V>,
    order: std::collections::VecDeque<K>,
}

impl<K: Eq + std::hash::Hash + Clone, V: Clone> Recent<K, V> {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: std::collections::HashMap::new(),
            order: std::collections::VecDeque::new(),
        }
    }

    pub(super) fn get(&self, key: &K) -> Option<V> {
        self.entries.get(key).cloned()
    }

    pub(super) fn put(&mut self, key: K, value: V) {
        if self.entries.insert(key.clone(), value).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            }
        }
    }
}

/// Full-size cards for reports' latest addresses, by report, version and card design. Each is a
/// whole picture, so only the most recent few are kept.
const LATEST_CARDS: usize = 64;

type CardKey = (String, String, u32);

fn latest_cards() -> &'static std::sync::Mutex<Recent<CardKey, std::sync::Arc<Vec<u8>>>> {
    static CARDS: std::sync::OnceLock<std::sync::Mutex<Recent<CardKey, std::sync::Arc<Vec<u8>>>>> =
        std::sync::OnceLock::new();
    CARDS.get_or_init(|| std::sync::Mutex::new(Recent::new(LATEST_CARDS)))
}

/// The 1200×630 card of a stored version, drawn natively from its title and byline (ARC-69).
/// It is served under the report's own access rule and never from a hotlinked cover.
///
/// A version's immutable address serves the card kept beside it when it was published, as it
/// promised. The latest address serves the card in today's design
/// ([`richview_share::CARD_DESIGN`]), drawn again from the stored report and held in memory, so a
/// report published before a redesign unfurls like one published after it, and nothing is written
/// into the store.
async fn share_card(
    state: AppState,
    headers: HeaderMap,
    slug: String,
    version: Option<String>,
) -> Response {
    let caller = auth::reader(&state, &headers, &slug);
    if let Err(response) = authorize(&state, &caller, &slug, Right::View) {
        return response;
    }
    let public = is_public(&state, &slug);
    let immutable = version.is_some();
    let storage = state.storage.clone();
    // An uncached card can be requested anonymously. Bound work before spawning it so a full
    // organization quota cannot turn repeated card generation into an unbounded worker flood.
    static HOSTED_CARD_WORKERS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    let permit = if state.access.is_hosted() {
        match HOSTED_CARD_WORKERS
            .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(2)))
            .clone()
            .try_acquire_owned()
        {
            Ok(permit) => Some(permit),
            Err(_) => {
                return api_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Report preview is busy; try again shortly",
                );
            }
        }
    } else {
        None
    };
    let card = tokio::task::spawn_blocking(move || -> Result<std::sync::Arc<Vec<u8>>, String> {
        let _permit = permit;
        let report = storage
            .get(&slug, version.as_deref())?
            .ok_or_else(|| "Not found".to_owned())?;
        if !immutable {
            let key = (
                report.slug.clone(),
                report.version.clone(),
                richview_share::CARD_DESIGN,
            );
            let cached = latest_cards()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&key);
            if let Some(png) = cached {
                return Ok(png);
            }
            let png = std::sync::Arc::new(draw_share_card(&report, &charts_of(&report))?);
            latest_cards()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .put(key, png.clone());
            return Ok(png);
        }
        if let Some(kept) = storage.share_card(&report.slug, &report.version)? {
            return Ok(std::sync::Arc::new(kept));
        }
        // Charts come from the stored source, so a card asked for before the publish-time draw
        // finished is the same card, and the one kept is never a title-only stand-in.
        let png = draw_share_card(&report, &charts_of(&report))?;
        let _ = storage.put_share_card(&report.slug, &report.version, &png);
        Ok(std::sync::Arc::new(png))
    })
    .await;
    match card {
        Ok(Ok(png)) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, cache_control(public, immutable)),
                (header::VARY, "Cookie, Authorization"),
            ],
            png.as_ref().clone(),
        )
            .into_response(),
        Ok(Err(e)) if e == "Not found" => api_error(StatusCode::NOT_FOUND, "Not found"),
        _ => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The share card could not be drawn",
        ),
    }
}

pub async fn share_latest(
    State(s): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    share_card(s, headers, slug, None).await
}
pub async fn share_version(
    State(s): State<AppState>,
    headers: HeaderMap,
    AxumPath((slug, v)): AxumPath<(String, String)>,
) -> Response {
    share_card(s, headers, slug, Some(v)).await
}

pub async fn report_latest(
    State(s): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    fetch_report(s, headers, slug, None, false).await
}
pub async fn report_version(
    State(s): State<AppState>,
    headers: HeaderMap,
    AxumPath((slug, v)): AxumPath<(String, String)>,
) -> Response {
    fetch_report(s, headers, slug, Some(v), false).await
}
pub async fn raw_latest(
    State(s): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    fetch_report(s, headers, slug, None, true).await
}
pub async fn raw_version(
    State(s): State<AppState>,
    headers: HeaderMap,
    AxumPath((slug, v)): AxumPath<(String, String)>,
) -> Response {
    fetch_report(s, headers, slug, Some(v), true).await
}

#[cfg(all(test, feature = "embedded-runtime"))]
#[path = "publish_refusal_tests.rs"]
mod publish_refusal_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hosted_hotfix_report_count_remedy() {
        let response =
            publish_error_response(&PublishError::ReportLimit { limit: 3 }, "fourth", "1.0.0");
        assert_eq!(response.status(), StatusCode::INSUFFICIENT_STORAGE);
        let data = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&data).unwrap();
        let diagnostic = &json["diagnostics"][0];
        assert_eq!(diagnostic["code"], "publish.quota");
        assert_eq!(diagnostic["fix"], "Delete a report to free a report slot.");
    }

    #[test]
    fn a_publication_keeps_no_macos_sidecar_and_every_other_file() {
        let mut request: PublishRequest = serde_json::from_value(json!({
            "slug": "q3",
            "source": "# Q3",
            "reportFolder": {"root": "q3", "source": "report.rmd", "files": [
                "report.rmd", "._report.rmd", "data/sales.csv", "data/._sales.csv",
                "__MACOSX/._report.rmd", ".DS_Store"
            ]},
            "data": {"data/sales.csv": "region,sales\nNorth,4\n", "data/._sales.csv": "x"},
            "files": {"notes.txt": "bm90ZXM=", "._notes.txt": "AAUWBw=="}
        }))
        .unwrap();
        request.drop_sidecars();
        // Only the sidecar's own name is dropped: another hidden file stays a file of the folder.
        assert_eq!(
            request.report_folder.unwrap().files,
            ["report.rmd", "data/sales.csv", ".DS_Store"]
        );
        assert_eq!(request.data.keys().collect::<Vec<_>>(), ["data/sales.csv"]);
        assert_eq!(request.files.keys().collect::<Vec<_>>(), ["notes.txt"]);
    }

    #[test]
    fn a_version_that_exists_is_answered_with_the_next_patch_to_write() {
        assert_eq!(next_patch("0.0.1").as_deref(), Some("0.0.2"));
        assert_eq!(next_patch(" 1.4.9 ").as_deref(), Some("1.4.10"));
        assert_eq!(next_patch("1.0.0-beta.1"), None);
        assert_eq!(next_patch("draft"), None);
        let response = publish_error_response(
            &PublishError::Conflict {
                stored_sha256: "x".into(),
            },
            "ab123456/q3",
            "0.0.1",
        );
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn hosted_page_limit_refuses_until_a_response_is_released() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".access")).unwrap();
        let access_state = json!({"schema":2,"tenancy":"hosted",
            "organizations":[{"id":"aaaa0001","name":"A","created":"now","limits":{}}],
            "memberships":[], "users":[], "sessions":[],
            "acls":{"aaaa0001/page":{"org":"aaaa0001","owner":"a","visibility":"public","grants":[]}}});
        std::fs::write(
            dir.path().join(".access/state.json"),
            serde_json::to_vec(&access_state).unwrap(),
        )
        .unwrap();
        let storage = crate::storage::FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true);
        storage
            .put_version(StoredReport {
                slug: "aaaa0001/page".into(),
                version: "1.0.0".into(),
                source: "# Page".into(),
                report_folder: None,
                html: "<html>page</html>".into(),
                frontmatter: json!({}),
                created_at: "now".into(),
                styles: vec![],
                data: BTreeMap::new(),
                files: BTreeMap::new(),
                content_sha256: String::new(),
            })
            .unwrap();
        let state = AppState {
            storage,
            access: crate::access::Access::open_hosted(dir.path()).unwrap(),
            preview: None,
            public_origin: None,
            report_origin: None,
        };
        let mut responses = Vec::new();
        for _ in 0..4 {
            let response = fetch_report(
                state.clone(),
                HeaderMap::new(),
                "aaaa0001/page".into(),
                None,
                false,
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            responses.push(response);
        }
        // Ten readers, including six who wait for the first four responses to drain.
        let mut readers = tokio::task::JoinSet::new();
        for _ in 0..6 {
            let state = state.clone();
            readers.spawn(async move {
                fetch_report(state, HeaderMap::new(), "aaaa0001/page".into(), None, false).await
            });
        }
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while hosted_page_waiters().available_permits() != 26 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        drop(responses);
        while let Some(result) = readers.join_next().await {
            assert_eq!(result.unwrap().status(), StatusCode::OK);
        }

        let mut responses = Vec::new();
        for _ in 0..4 {
            let response = fetch_report(
                state.clone(),
                HeaderMap::new(),
                "aaaa0001/page".into(),
                None,
                false,
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            responses.push(response);
        }
        // No more than 32 readers may wait, and cancellation must return their queue slots.
        for _ in 0..32 {
            let state = state.clone();
            readers.spawn(async move {
                fetch_report(state, HeaderMap::new(), "aaaa0001/page".into(), None, false).await
            });
        }
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while hosted_page_waiters().available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            fetch_report(
                state.clone(),
                HeaderMap::new(),
                "aaaa0001/page".into(),
                None,
                false,
            ),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::RETRY_AFTER], "1");
        assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
        assert!(
            response.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        let body = axum::body::to_bytes(response.into_body(), 2_000_000)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            html.contains("The report service is busy. Wait a moment, then load the page again.")
        );
        assert!(html.contains("href=\"\">Try again</a>"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("rv-host-bar"));
        assert!(!html.contains("http-equiv=\"refresh\""));
        let raw = fetch_report(
            state.clone(),
            HeaderMap::new(),
            "aaaa0001/page".into(),
            None,
            true,
        )
        .await;
        assert_eq!(raw.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(raw.headers()[header::RETRY_AFTER], "1");
        let raw = axum::body::to_bytes(raw.into_body(), 100_000)
            .await
            .unwrap();
        assert_eq!(&raw[..], b"Report service is busy; try again shortly");
        readers.abort_all();
        while readers.join_next().await.is_some() {}
        assert_eq!(hosted_page_waiters().available_permits(), 32);

        let started = tokio::time::Instant::now();
        let response = fetch_report(
            state.clone(),
            HeaderMap::new(),
            "aaaa0001/page".into(),
            None,
            false,
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::RETRY_AFTER], "1");
        assert!(started.elapsed() >= std::time::Duration::from_millis(1900));
        drop(responses.pop());
        let response =
            fetch_report(state, HeaderMap::new(), "aaaa0001/page".into(), None, false).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn hosted_download_refuses_excess_before_loading_a_record() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".access")).unwrap();
        let state = json!({"schema":2,"tenancy":"hosted",
            "organizations":[{"id":"aaaa0001","name":"A","created":"now","limits":{}}],
            "memberships":[], "users":[], "sessions":[],
            "acls":{"aaaa0001/missing":{"org":"aaaa0001","owner":"a","visibility":"public","grants":[]}}});
        std::fs::write(
            dir.path().join(".access/state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let access = crate::access::Access::open_hosted(dir.path()).unwrap();
        let state = AppState {
            storage: crate::storage::FsStorage::new(dir.path())
                .unwrap()
                .with_hosted(true),
            access,
            preview: None,
            public_origin: None,
            report_origin: None,
        };
        let first = hosted_download_workers()
            .clone()
            .try_acquire_owned()
            .unwrap();
        let second = hosted_download_workers()
            .clone()
            .try_acquire_owned()
            .unwrap();
        let response = report_download(
            State(state.clone()),
            HeaderMap::new(),
            AxumPath(("aaaa0001/missing".into(), "1.0.0".into())),
        )
        .await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let latest = report_download_latest(
            State(state.clone()),
            HeaderMap::new(),
            AxumPath("aaaa0001/missing".into()),
        )
        .await;
        assert_eq!(latest.status(), StatusCode::TOO_MANY_REQUESTS);
        let file = report_file(
            State(state.clone()),
            HeaderMap::new(),
            AxumPath((
                "aaaa0001/missing".into(),
                "1.0.0".into(),
                "missing.csv".into(),
            )),
        )
        .await;
        assert_eq!(file.status(), StatusCode::TOO_MANY_REQUESTS);
        drop((first, second));
        let response = report_download(
            State(state.clone()),
            HeaderMap::new(),
            AxumPath(("aaaa0001/missing".into(), "1.0.0".into())),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        state
            .storage
            .put_version(StoredReport {
                slug: "aaaa0001/missing".into(),
                version: "1.0.0".into(),
                source: "# Report".into(),
                report_folder: None,
                html: "<html>report</html>".into(),
                frontmatter: json!({}),
                created_at: "now".into(),
                styles: vec![],
                data: BTreeMap::new(),
                files: BTreeMap::from([("file.txt".into(), "aGVsbG8=".into())]),
                content_sha256: String::new(),
            })
            .unwrap();
        let file = || {
            report_file(
                State(state.clone()),
                HeaderMap::new(),
                AxumPath(("aaaa0001/missing".into(), "1.0.0".into(), "file.txt".into())),
            )
        };
        let first = file().await;
        let second = file().await;
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(second.status(), StatusCode::OK);
        assert_eq!(file().await.status(), StatusCode::TOO_MANY_REQUESTS);
        drop(first);
        assert_eq!(file().await.status(), StatusCode::OK);
        drop(second);
    }

    #[tokio::test]
    async fn the_latest_card_is_drawn_in_todays_design_and_a_versions_kept_card_stays() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".access")).unwrap();
        let access_state = json!({"schema":2,"tenancy":"hosted",
            "organizations":[{"id":"aaaa0001","name":"A","created":"now","limits":{}}],
            "memberships":[], "users":[], "sessions":[],
            "acls":{"aaaa0001/chart":{"org":"aaaa0001","owner":"a","visibility":"public","grants":[]}}});
        std::fs::write(
            dir.path().join(".access/state.json"),
            serde_json::to_vec(&access_state).unwrap(),
        )
        .unwrap();
        let storage = crate::storage::FsStorage::new(dir.path())
            .unwrap()
            .with_hosted(true);
        let source = "---\ntitle: T\nstyles: [sleek]\n---\n\n```chart\ntype: bar\ntitle: Share\ndata:\n  inline:\n    - { year: \"2023\", share: 3.0 }\n    - { year: \"2024\", share: 18.2 }\nencoding: { x: year, y: share }\n```\n";
        let report: StoredReport = serde_json::from_value(json!({
            "slug": "aaaa0001/chart", "version": "1.0.0", "source": source, "html": "<html>chart</html>",
            "frontmatter": { "title": "T", "styles": ["sleek"] }, "createdAt": "now",
            "reportFolder": { "root": "card-fixture", "source": "report.rmd", "files": ["report.rmd"] },
        }))
        .unwrap();
        storage.put_version(report.clone()).unwrap();
        // The card kept at publication in an earlier design; the title card stands in for it.
        let kept = draw_share_card(&report, &[]).unwrap();
        storage
            .put_share_card(&report.slug, &report.version, &kept)
            .unwrap();
        let state = AppState {
            storage: storage.clone(),
            access: crate::access::Access::open_hosted(dir.path()).unwrap(),
            preview: None,
            public_origin: None,
            report_origin: None,
        };
        let png = |version: Option<&str>| {
            let state = state.clone();
            let version = version.map(str::to_owned);
            async move {
                let response =
                    share_card(state, HeaderMap::new(), "aaaa0001/chart".into(), version).await;
                assert_eq!(response.status(), StatusCode::OK);
                axum::body::to_bytes(response.into_body(), 4_000_000)
                    .await
                    .unwrap()
                    .to_vec()
            }
        };
        assert_eq!(
            png(Some("1.0.0")).await,
            kept,
            "an immutable address keeps its bytes"
        );
        let today = draw_share_card(&report, &charts_of(&report)).unwrap();
        assert_ne!(today, kept);
        assert_eq!(png(None).await, today);
        assert_eq!(png(None).await, today, "drawn once, then from memory");
        assert_eq!(
            storage
                .share_card(&report.slug, &report.version)
                .unwrap()
                .unwrap(),
            kept,
            "nothing is written into the store"
        );
    }

    #[test]
    fn a_share_card_with_a_chart_differs_from_the_title_only_card() {
        let source = "---\ntitle: T\nstyles: [sleek]\n---\n\n```chart\ntype: bar\ntitle: Share\ndata:\n  inline:\n    - { year: \"2023\", share: 3.0 }\n    - { year: \"2024\", share: 18.2 }\nencoding: { x: year, y: share }\n```\n";
        let rendered = richview_core::document::render(
            source,
            &richview_core::document::RenderOptions {
                report_folder: Some(richview_core::document::ReportFolder {
                    root: "card-fixture".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            },
        )
        .unwrap();
        let report: StoredReport = serde_json::from_value(serde_json::json!({
            "slug": "t", "version": "1.0.0", "source": source, "html": "",
            "frontmatter": { "title": "T", "styles": ["sleek"] }, "createdAt": "",
            "reportFolder": { "root": "card-fixture", "source": "report.rmd", "files": ["report.rmd"] },
        }))
        .unwrap();
        let plain = draw_share_card(&report, &[]).unwrap();
        let led = draw_share_card(&report, &rendered.charts).unwrap();
        assert_ne!(plain, led);
        // A card asked for before the publish-time draw is rebuilt from the stored source.
        assert_eq!(charts_of(&report), rendered.charts);
    }
}
