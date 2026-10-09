//! `GET /api/archive` (W5-02): the archived-page browsing + `pages_fts`
//! search surface — pure JSON since FX-05 ported `/archive` into the SPA.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::extract::State;
use axum::http::Uri;
use axum::response::{IntoResponse, Json, Response};
use cauce_core::PageHit;
use serde::Serialize;
use ts_rs::TS;
use url::Url;
use uuid::Uuid;

use super::{MAX_LIMIT, QueryParams};
use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// `GET /api/archive` default page size (the SPA `/archive` listing's
/// row budget; `HISTORY_LIMIT`'s role here).
pub(crate) const ARCHIVE_LIMIT: u32 = 50;

/// One archive row on the JSON wire: a `PageHit` with its snippet's
/// `PAGE_MARK_*` delimiters stripped (the marks are the HTML contract,
/// not part of the wire text).
#[derive(Debug, Serialize, TS)]
pub struct ArchiveRow {
    pub url: Url,
    pub title: String,
    pub snippet: String,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

/// `GET /api/archive` envelope (`{query,limit,offset,has_more,results,
/// request_id}`) — the `/api/search` envelope's convention: request id
/// top level, results carrying `{url,title,snippet,fetched_at[,score]}`.
#[derive(Debug, Serialize, TS)]
pub struct ArchiveResponse {
    /// The `?q=` text as typed; `null` on the browsing arm.
    pub query: Option<String>,
    /// Page size the request resolved to.
    pub limit: u32,
    /// `?offset=` the request resolved to.
    pub offset: u32,
    /// A further page exists.
    pub has_more: bool,
    /// `limit` hits in bm25 rank order (search arm) or newest first
    /// (browsing arm).
    pub results: Vec<ArchiveRow>,
    /// The request id (`RequestCtx`).
    pub request_id: Uuid,
}

impl From<&PageHit> for ArchiveRow {
    fn from(hit: &PageHit) -> Self {
        Self {
            url: hit.url.clone(),
            title: hit.title.clone(),
            snippet: hit.plain_snippet(),
            fetched_at: hit.fetched_at,
            score: hit.score,
        }
    }
}

/// The shared archive query result for the JSON route: the parsed
/// request (`query` is `None` for the browsing arm), the hits, and the
/// `limit`+1 peek the caller turns into a pager.
#[derive(Debug)]
pub(crate) struct ArchiveData {
    /// The `?q=` text as typed (only on the search arm).
    pub query: Option<String>,
    /// Page size the request resolved to.
    pub limit: u32,
    /// `?offset=` the request resolved to.
    pub offset: u32,
    /// `limit` hits (the +1 peek is already dropped).
    pub hits: Vec<PageHit>,
    /// A further page exists (`list_pages` was asked for `limit + 1`).
    pub has_more: bool,
}

/// `GET /api/archive?q&limit&offset` (W5-02): with `q`, `pages_fts` hits
/// in bm25 rank order; without it, the newest `pages` rows first — the
/// W5-02 listing surface this route declares in ROUTES.
pub async fn archive_search(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Response, ApiError> {
    // FX-07: `archive.enabled = false` takes the listing/search surface
    // down with the write one (the `archiving` flag is off for both).
    if !state.with_config(|c| c.archive.enabled) {
        return Err(ctx.err(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "archive_disabled",
            "the archive pipeline is not available (disabled at build or config)",
        ));
    }
    archive_inner(&state, &ctx, &uri)
        .await
        .map(|data| Json(archive_response(&ctx, &data)).into_response())
}

/// The `GET /api/archive` data path. The store sees `limit + 1` on the
/// browsing arm so the response knows whether a next page exists (the
/// `cache_list_data` peek pattern).
pub(crate) async fn archive_inner(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
) -> Result<ArchiveData, ApiError> {
    let params = QueryParams::parse(uri.query(), ctx)?;
    params.allow(ctx, &["q", "limit", "offset"])?;
    let limit = params.u32(ctx, "limit", ARCHIVE_LIMIT)?.clamp(1, MAX_LIMIT);
    let offset = params.u32(ctx, "offset", 0)?;
    // Blank `q=` is no query at all — JSON and the page must agree (the
    // history filter's convention).
    let query = params
        .get("q")
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string);
    let store = state.store();
    let (hits, has_more) = match &query {
        Some(q) => (
            store
                .search_pages(q, limit)
                .await
                .map_err(|e| ctx.store(&e))?,
            false,
        ),
        None => {
            let mut rows = store
                .list_pages(limit.saturating_add(1), offset)
                .await
                .map_err(|e| ctx.store(&e))?;
            let more = rows.len() > limit as usize;
            rows.truncate(limit as usize);
            (rows, more)
        }
    };
    Ok(ArchiveData {
        query,
        limit,
        offset,
        hits,
        has_more,
    })
}

/// The JSON payload (`ArchiveResponse`): `{query, results, request_id}`
/// — the `/api/search` envelope's convention (request id top level,
/// results carrying `{url,title,snippet,fetched_at[,score]}`).
fn archive_response(ctx: &RequestCtx, data: &ArchiveData) -> ArchiveResponse {
    ArchiveResponse {
        query: data.query.clone(),
        limit: data.limit,
        offset: data.offset,
        has_more: data.has_more,
        results: data.hits.iter().map(ArchiveRow::from).collect(),
        request_id: ctx.request_id.as_uuid(),
    }
}
