//! `GET /api/archive` (W5-02): the archived-page browsing + `pages_fts`
//! search surface. `Accept: text/html` renders the `/archive` page through
//! the same data path (the W2-02 one-data-path convention).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::{IntoResponse, Json, Response};
use cauce_core::PageHit;
use serde::Serialize;
use serde_json::Value;
use url::Url;

use super::{MAX_LIMIT, QueryParams};
use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// `GET /api/archive` default page size (the `/archive` page's row
/// budget; `HISTORY_LIMIT`'s role here).
pub(crate) const ARCHIVE_LIMIT: u32 = 50;

/// One archive row on the JSON wire: a `PageHit` with its snippet's
/// `PAGE_MARK_*` delimiters stripped (the marks are the HTML contract,
/// not part of the wire text).
#[derive(Debug, Serialize)]
pub(crate) struct ArchiveRow {
    pub url: Url,
    pub title: String,
    pub snippet: String,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
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

/// The shared archive query result for the JSON route and the HTMX page:
/// the parsed request (`query` is `None` for the browsing arm), the hits,
/// and the `limit`+1 peek the page turns into a pager.
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
/// W5-02 listing surface this route declares in ROUTES. `Accept:
/// text/html` renders `/archive` through the same data path.
pub async fn archive_search(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    #[cfg(feature = "ui")]
    if crate::html::prefers_html(&headers) {
        return crate::html::archive_page(State(state), Extension(ctx), uri).await;
    }
    #[cfg(not(feature = "ui"))]
    let _ = &headers;
    archive_inner(&state, &ctx, &uri)
        .await
        .map(|data| Json(archive_json(&ctx, &data)).into_response())
}

/// One data path for `GET /api/archive` and the `/archive` page. The
/// store sees `limit + 1` on the browsing arm so the page knows whether a
/// next page exists (the `cache_list_data` peek pattern).
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

/// The JSON payload: `{query, results, request_id}` — the `/api/search`
/// envelope's convention (request id top level, results carrying
/// `{url,title,snippet,fetched_at[,score]}`).
fn archive_json(ctx: &RequestCtx, data: &ArchiveData) -> Value {
    serde_json::json!({
        "query": data.query,
        "limit": data.limit,
        "offset": data.offset,
        "has_more": data.has_more,
        "results": data.hits.iter().map(ArchiveRow::from).collect::<Vec<_>>(),
        "request_id": ctx.request_id.as_uuid(),
    })
}
