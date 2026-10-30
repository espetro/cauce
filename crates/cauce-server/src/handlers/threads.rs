//! `threads` (UX-01): the server-authoritative conversation surface —
//! `GET /api/threads` (the owner's threads, newest-first),
//! `GET /api/threads/{id}` (one transcript for hydration) and
//! `DELETE /api/threads/{id}` (audited). Owner isolation: rows carry the
//! caller's identity — `"local"` in local mode, the SHA-256 hex of the
//! bearer token in public mode (never the raw token) — and every read
//! filters by it, so another user's thread is a plain 404.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;

use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

use super::write_audit;

/// Page cap shared with the history surface (`MAX_LIMIT` semantics:
/// a caller asking for more still gets this many rows).
const MAX_LIMIT: u32 = 200;

/// `GET /api/threads?limit=` paging.
#[derive(Debug, Deserialize)]
pub struct ThreadListQuery {
    #[serde(default)]
    limit: Option<u32>,
}

/// The owner key for the caller (UX-01): `"local"` while
/// `server.public_instance` is off — the single-user deployment — and
/// the SHA-256 hex of the bearer token once public, so the store never
/// holds a raw credential and two callers never share a thread space.
pub fn thread_owner(state: &AppState, headers: &HeaderMap) -> String {
    let public = state.with_config(|cfg| cfg.server.public_instance);
    if !public {
        return "local".to_string();
    }
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty());
    match token {
        Some(token) => {
            let mut hasher = Sha256::new();
            hasher.update(token.as_bytes());
            format!("{:x}", hasher.finalize())
        }
        // An anonymous public caller has no thread space: the key is
        // stable (so lookups are consistent) but no row is ever written
        // to it — `POST /api/answer` without a bearer has no identity to
        // own a thread, and the threads handlers return the empty set.
        None => "anonymous".to_string(),
    }
}

/// `GET /api/threads?limit=` — the caller's threads, `updated_at`
/// newest-first.
pub async fn threads_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Query(query): Query<ThreadListQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<cauce_core::AnswerThreadRow>>, ApiError> {
    let limit = query.limit.unwrap_or(50).min(MAX_LIMIT);
    let owner = thread_owner(&state, &headers);
    let rows = state
        .store()
        .list_threads(&owner, limit)
        .await
        .map_err(|e| ctx.store(&e))?;
    Ok(Json(rows))
}

fn thread_id(ctx: &RequestCtx, raw: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(raw).map_err(|_| ctx.bad_request(format!("invalid thread id {raw:?}")))
}

/// `GET /api/threads/{id}` — one transcript. A foreign owner's id is a
/// 404 (`get_thread` scopes by owner), never a leak.
pub async fn threads_get(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<cauce_core::AnswerThreadRow>, ApiError> {
    let id = thread_id(&ctx, &id)?;
    let owner = thread_owner(&state, &headers);
    let Some(row) = state
        .store()
        .get_thread(&id.to_string(), &owner)
        .await
        .map_err(|e| ctx.store(&e))?
    else {
        return Err(ctx.not_found(format!("no thread {id}")));
    };
    Ok(Json(row))
}

/// `DELETE /api/threads/{id}` — audited; a foreign owner's id 404s.
pub async fn threads_delete(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = thread_id(&ctx, &id)?;
    let owner = thread_owner(&state, &headers);
    let deleted = state
        .store()
        .delete_thread(&id.to_string(), &owner)
        .await
        .map_err(|e| ctx.store(&e))?;
    if !deleted {
        return Err(ctx.not_found(format!("no thread {id}")));
    }
    write_audit(
        state.store(),
        &ctx,
        &headers,
        "answer_thread.delete",
        id.to_string(),
        json!({ "owner": owner }),
    )
    .await?;
    Ok(Json(json!({ "deleted": true, "id": id })))
}
