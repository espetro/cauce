//! `pages` (W5-01): `POST /api/pages` runs the fetch-and-index pipeline for
//! one URL (also the UI click beacon's target); `GET /api/pages/{url}`
//! reads the stored row by its percent-encoded URL (the SPA `/archive`
//! row expander reads `markdown` off the row), and `DELETE
//! /api/pages/{url}` removes the row, audited.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use cauce_core::{ArchiveError, CacheKey, EngineError, PageRow, normalize_url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;
use url::Url;

use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

use super::write_audit;

/// `POST /api/pages` body: the URL to index plus, for the click beacon,
/// the `CacheKey` of the search that surfaced it.
#[derive(Debug, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct IndexBody {
    url: String,
    #[serde(default)]
    query_hash: Option<CacheKey>,
}

/// `DELETE /api/pages/{url}` acknowledgement: echoes the normalized key
/// the row was stored under (`{"deleted":true,"url":...}`).
#[derive(Debug, Clone, Serialize, TS)]
pub struct PageDeleteAck {
    pub deleted: bool,
    /// Normalized URL key the deleted row was stored under.
    pub url: Url,
}

fn archive_unavailable(ctx: &RequestCtx) -> ApiError {
    ctx.err(
        StatusCode::SERVICE_UNAVAILABLE,
        "archive_disabled",
        "the archive pipeline is not available (disabled at build or config)",
    )
}

/// `ArchiveError` -> the error envelope: caller faults to 4xx, upstream and
/// pipeline faults to 5xx, store faults through the shared mapping.
fn archive_error(ctx: &RequestCtx, e: &ArchiveError) -> ApiError {
    match e {
        ArchiveError::InvalidUrl(_) => ctx.bad_request(e.to_string()),
        // Egress-guard refusal (private/reserved target): the URL is a
        // caller fault, not an upstream failure — 403, not 502.
        ArchiveError::Blocked(_) => ctx.err(StatusCode::FORBIDDEN, "url_blocked", e.to_string()),
        ArchiveError::Fetch(EngineError::Timeout) => ctx.err(
            StatusCode::GATEWAY_TIMEOUT,
            "upstream_timeout",
            e.to_string(),
        ),
        ArchiveError::Fetch(_) | ArchiveError::Status(_) => {
            ctx.err(StatusCode::BAD_GATEWAY, "upstream_error", e.to_string())
        }
        ArchiveError::Extract(_) => ctx.err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "extraction_failed",
            e.to_string(),
        ),
        ArchiveError::Store(se) => ctx.store(se),
    }
}

/// `POST /api/pages`: fetch `url`, extract the article to markdown, upsert
/// the `pages` row and return it (201). One synchronous round trip —
/// callers wanting fire-and-forget (the UI beacon) don't wait on the
/// response.
pub async fn pages_index(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<PageRow>), ApiError> {
    let body: IndexBody = serde_json::from_slice(&body)
        .map_err(|e| ctx.bad_request(format!("invalid pages body: {e}")))?;
    let archiver = state.archive().ok_or_else(|| archive_unavailable(&ctx))?;
    let row = archiver
        .fetch_and_index(&body.url, body.query_hash)
        .await
        .map_err(|e| archive_error(&ctx, &e))?;
    write_audit(
        state.store(),
        &ctx,
        &headers,
        "page.index",
        row.url.to_string(),
        json!({ "byte_len": row.byte_len }),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

/// `GET /api/pages/{url}`: read the archived row for a percent-encoded
/// URL. The `{url}` path segment carries the whole URL
/// (`/api/pages/https%3A%2F%2Fexample.com%2Fa`); axum percent-decodes the
/// segment and the lookup key is the same normalized form
/// `fetch_and_index` stores.
pub async fn pages_get(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(url): Path<String>,
) -> Result<Response, ApiError> {
    // FX-07: `archive.enabled = false` 503s the read surface like the
    // write one — the `archiving` flag is down, so every surface is.
    if !state.with_config(|c| c.archive.enabled) {
        return Err(archive_unavailable(&ctx));
    }
    pages_get_entry(&state, &ctx, &url).await
}

async fn pages_get_entry(
    state: &AppState,
    ctx: &RequestCtx,
    url: &str,
) -> Result<Response, ApiError> {
    let parsed =
        Url::parse(url).map_err(|e| ctx.bad_request(format!("invalid url path parameter: {e}")))?;
    let key = normalize_url(&parsed);
    match state
        .store()
        .get_page(&key)
        .await
        .map_err(|e| ctx.store(&e))?
    {
        Some(row) => Ok(Json(row).into_response()),
        None => Err(ctx.not_found(format!("no archived page for {url}"))),
    }
}

/// `DELETE /api/pages/{url}` (W5-02): audited single-row delete — the
/// `cache_delete`/`history_delete` convention (DELETE over POST; the
/// engine-reset POST stays for non-destructive actions). The
/// `pages_fts_ad` trigger retracts the index row with it. The JSON body
/// echoes the normalized key the row was stored under.
pub async fn pages_delete(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(url): Path<String>,
) -> Result<Json<PageDeleteAck>, ApiError> {
    // FX-07: same `archive.enabled` 503 as the rest of the surface.
    if !state.with_config(|c| c.archive.enabled) {
        return Err(archive_unavailable(&ctx));
    }
    let parsed = Url::parse(&url)
        .map_err(|e| ctx.bad_request(format!("invalid url path parameter: {e}")))?;
    let key = normalize_url(&parsed);
    let removed = state
        .store()
        .delete_page(&key)
        .await
        .map_err(|e| ctx.store(&e))?;
    if !removed {
        return Err(ctx.not_found(format!("no archived page for {url}")));
    }
    write_audit(
        state.store(),
        &ctx,
        &headers,
        "page.delete",
        key.to_string(),
        json!({}),
    )
    .await?;
    Ok(Json(PageDeleteAck {
        deleted: true,
        url: key,
    }))
}
