//! The governance API: people, roles, tokens, who can see and change each report, and the audit
//! trail (FEA-SF-18, FEA-SF-19). Admins only, except the per-report access routes, which need the
//! `share` right.
use super::{
    AppState, api_error,
    auth::{self, access_error, admin, authorize, mutating},
};
use crate::access::{Grant, OrgRole, Right, Visibility};
use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

fn audit(state: &AppState, caller: &auth::Caller, action: &str, target: &str) {
    let _ = state.access.audit(&caller.principal, action, target, true);
}

/// `GET /api/governance/users`
pub async fn list_users(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = admin(&state, &headers, false) {
        return response;
    }
    Json(state.access.users()).into_response()
}

#[derive(Deserialize)]
pub struct NewUser {
    pub name: String,
    pub email: String,
    pub role: OrgRole,
    #[serde(default)]
    pub password: Option<String>,
}

/// `POST /api/governance/users`
pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<NewUser>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let caller = match admin(&state, &headers, true) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let Json(body) = match payload {
        Ok(body) => body,
        Err(e) => return api_error(e.status(), "Invalid person JSON"),
    };
    let access = state.access.clone();
    let result = tokio::task::spawn_blocking(move || {
        access.create_user(&body.name, &body.email, body.role, body.password.as_deref())
    })
    .await;
    match result {
        Ok(Ok(user)) => {
            audit(&state, &caller, "user.create", &user.id);
            (StatusCode::CREATED, Json(user)).into_response()
        }
        Ok(Err(error)) => access_error(error),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Worker failed"),
    }
}

#[derive(Deserialize)]
pub struct UserChange {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<OrgRole>,
    #[serde(default)]
    pub disabled: Option<bool>,
}

/// `PATCH /api/governance/users/{id}`: name, role or standing.
pub async fn update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    payload: Result<Json<UserChange>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let caller = match admin(&state, &headers, true) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let Json(body) = match payload {
        Ok(body) => body,
        Err(e) => return api_error(e.status(), "Invalid person JSON"),
    };
    match state
        .access
        .update_user(&id, body.name.as_deref(), body.role, body.disabled)
    {
        Ok(user) => {
            audit(&state, &caller, "user.update", &id);
            Json(user).into_response()
        }
        Err(error) => access_error(error),
    }
}

/// `DELETE /api/governance/users/{id}`
pub async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let caller = match admin(&state, &headers, true) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    match state.access.delete_user(&id) {
        Ok(()) => {
            audit(&state, &caller, "user.delete", &id);
            Json(json!({"ok": true})).into_response()
        }
        Err(error) => access_error(error),
    }
}

#[derive(Deserialize)]
pub struct NewPassword {
    pub password: String,
}

/// `PUT /api/governance/users/{id}/password`: sets a password and ends the person's sign-ins.
pub async fn set_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    payload: Result<Json<NewPassword>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let caller = match admin(&state, &headers, true) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let Json(body) = match payload {
        Ok(body) => body,
        Err(e) => return api_error(e.status(), "Invalid password JSON"),
    };
    let access = state.access.clone();
    let target = id.clone();
    match tokio::task::spawn_blocking(move || access.set_password(&target, &body.password)).await {
        Ok(Ok(())) => {
            audit(&state, &caller, "user.password", &id);
            Json(json!({"ok": true})).into_response()
        }
        Ok(Err(error)) => access_error(error),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Worker failed"),
    }
}

/// `GET /api/governance/users/{id}/tokens`: the person's tokens, without their secrets.
pub async fn list_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    if let Err(response) = admin(&state, &headers, false) {
        return response;
    }
    if state.access.user(&id).is_none() {
        return auth::not_found();
    }
    Json(state.access.tokens(&id)).into_response()
}

#[derive(Deserialize)]
pub struct NewToken {
    #[serde(default)]
    pub name: String,
}

/// `POST /api/governance/users/{id}/tokens`: the secret is in this response only.
pub async fn create_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    payload: Result<Json<NewToken>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let caller = match admin(&state, &headers, true) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    let name = payload.map(|Json(b)| b.name).unwrap_or_default();
    match state.access.create_token(&id, &name) {
        Ok((token_id, secret)) => {
            audit(&state, &caller, "token.create", &format!("{id}/{token_id}"));
            (
                StatusCode::CREATED,
                Json(json!({"id": token_id, "token": secret})),
            )
                .into_response()
        }
        Err(error) => access_error(error),
    }
}

/// `DELETE /api/governance/users/{id}/tokens/{token}`
pub async fn revoke_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((id, token)): AxumPath<(String, String)>,
) -> Response {
    let caller = match admin(&state, &headers, true) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    match state.access.revoke_single_server_token(&id, &token) {
        Ok(()) => {
            audit(&state, &caller, "token.revoke", &format!("{id}/{token}"));
            Json(json!({"ok": true})).into_response()
        }
        Err(error) => access_error(error),
    }
}

#[derive(Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

/// `GET /api/governance/audit`: the latest lines of the audit trail, newest first.
pub async fn read_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Response {
    if let Err(response) = admin(&state, &headers, false) {
        return response;
    }
    Json(
        state
            .access
            .read_audit(query.limit.unwrap_or(100).min(1000)),
    )
    .into_response()
}

/// `GET /api/reports/{slug}/access`: who can see and change the report; needs `share`.
pub async fn get_access(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> Response {
    let caller = auth::caller(&state, &headers);
    if let Err(response) = authorize(&state, &caller, &slug, Right::Share) {
        return response;
    }
    if state.access.is_hosted() {
        let storage = state.storage.clone();
        let key = slug.clone();
        match tokio::task::spawn_blocking(move || storage.summary(&key, None)).await {
            Ok(Ok(Some(_))) => {}
            Ok(Ok(None)) => return auth::not_found(),
            _ => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Storage read failed"),
        }
    }
    Json(access_json(&state, &slug)).into_response()
}

fn access_json(state: &AppState, slug: &str) -> Value {
    let acl = state.access.acl(slug);
    let mut body = json!({
        "slug": slug,
        "owner": acl.owner,
        "visibility": acl.visibility,
        "grants": acl.grants,
    });
    if state.access.is_hosted() {
        // Shares offered to an address and not accepted yet. They become grants when the
        // person signs in, so anyone deciding who can open the report has to see them.
        let pending: Vec<Value> = state
            .access
            .report_access(slug)
            .map(|access| access.named)
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.pending)
            .map(|row| json!({"email": row.email, "role": row.role}))
            .collect();
        body["pending"] = json!(pending);
    }
    body
}

/// An open share the replacement keeps, named by its address. Its role is not taken from here.
#[derive(Deserialize)]
pub struct PendingShare {
    pub email: String,
}

#[derive(Deserialize)]
pub struct AccessChange {
    pub visibility: Visibility,
    #[serde(default)]
    pub grants: Vec<Grant>,
    /// Hands the report to another person; only the owner or an admin may.
    #[serde(default)]
    pub owner: Option<String>,
    /// Hosted: the open shares that stay. Every one not named here is cancelled.
    #[serde(default)]
    pub pending: Vec<PendingShare>,
}

/// `PUT /api/reports/{slug}/access`: share or protect the report; needs `share`.
pub async fn put_access(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
    payload: Result<Json<AccessChange>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let caller = match mutating(&state, &headers) {
        Ok(caller) => caller,
        Err(response) => return response,
    };
    if let Err(response) = authorize(&state, &caller, &slug, Right::Share) {
        return response;
    }
    let Json(body) = match payload {
        Ok(body) => body,
        Err(e) => return api_error(e.status(), "Invalid access JSON"),
    };
    if body.owner.is_some() {
        // Only the owner or an admin gives a report away.
        let is_owner =
            state.access.acl(&slug).owner == caller.principal.label().trim_start_matches("user:");
        if !is_owner && !state.access.is_admin(&caller.principal) {
            return api_error(
                StatusCode::FORBIDDEN,
                "Only the owner can hand the report to someone else.",
            );
        }
    }
    match state.access.replace_access_authorized(
        || auth::caller(&state, &headers).principal,
        &slug,
        body.owner.as_deref(),
        body.visibility,
        body.grants,
        &body
            .pending
            .iter()
            .map(|share| share.email.clone())
            .collect::<Vec<_>>(),
    ) {
        Ok(_) => {
            audit(&state, &caller, "report.share", &slug);
            Json(access_json(&state, &slug)).into_response()
        }
        Err(error) => access_error(error),
    }
}
