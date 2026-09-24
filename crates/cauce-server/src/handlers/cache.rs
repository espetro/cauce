//! `/api/cache` handlers: listing (with the tier-2 lexical `q` switch),
//! single-entry get and the audited single/bulk deletes.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Uri};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{Value, json};

use super::{MAX_LIMIT, QueryParams, cache_key, write_audit};
use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// `GET /api/cache?limit&offset&q`: cache admin listing (includes
/// expired-but-not-yet-evicted rows). `q` switches to the tier-2 lexical
/// index (`Store::get_lexical`): FTS over stored queries, titles and
/// snippets, ranked, capped at 255 rows, `offset` ignored.
pub async fn cache_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Json<Vec<cauce_core::CachedSearch>>, ApiError> {
    cache_list_data(&state, &ctx, &uri, false)
        .await
        .map(|listing| Json(listing.entries))
}

/// Shared cache listing logic for the JSON API and `/cache` page. The page
/// requests one additional unfiltered row to determine whether a next page
/// exists; parsing, filter semantics, limits, and store selection remain
/// authoritative here for both surfaces.
pub(crate) struct CacheListing {
    pub(crate) entries: Vec<cauce_core::CachedSearch>,
    pub(crate) limit: u32,
    pub(crate) offset: u32,
    pub(crate) query: Option<String>,
}

pub(crate) async fn cache_list_data(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
    include_next: bool,
) -> Result<CacheListing, ApiError> {
    let params = QueryParams::parse(uri.query(), ctx)?;
    params.allow(ctx, &["limit", "offset", "q"])?;
    let limit = params.u32(ctx, "limit", 50)?.clamp(1, MAX_LIMIT);
    let offset = params.u32(ctx, "offset", 0)?;
    let query = params
        .get("q")
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let entries = if let Some(q) = &query {
        state
            .store()
            .get_lexical(q, limit.min(u8::MAX as u32) as u8)
            .await
            .map_err(|e| ctx.store(&e))?
    } else {
        state
            .store()
            .list_cache(limit.saturating_add(u32::from(include_next)), offset)
            .await
            .map_err(|e| ctx.store(&e))?
    };
    Ok(CacheListing {
        entries,
        limit,
        offset,
        query,
    })
}

/// `GET /api/cache/{key}`: one entry by hex `CacheKey` (400 malformed,
/// 404 absent). `Accept: text/html` renders the stored payload as a
/// pretty-JSON fragment for the `/cache` row expander (W2-04); that arm
/// exists only in `ui` builds, and under it error statuses answer a
/// one-line fragment instead of the JSON envelope so the expander can
/// swap the failure in place.
#[cfg_attr(not(feature = "ui"), allow(unused_variables))]
pub async fn cache_get(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Result<Response, ApiError> {
    match cache_get_entry(&state, &ctx, &headers, &key).await {
        Ok(resp) => Ok(resp),
        Err(e) => {
            #[cfg(feature = "ui")]
            if crate::cache_page::accepts_html(&headers) {
                return Ok(crate::cache_page::payload_error(
                    e.status(),
                    crate::cache_page::is_htmx(&headers),
                ));
            }
            Err(e)
        }
    }
}

#[cfg_attr(not(feature = "ui"), allow(unused_variables))]
async fn cache_get_entry(
    state: &AppState,
    ctx: &RequestCtx,
    headers: &HeaderMap,
    key: &str,
) -> Result<Response, ApiError> {
    let key = cache_key(ctx, key)?;
    match state
        .store()
        .get_cache(&key)
        .await
        .map_err(|e| ctx.store(&e))?
    {
        Some(entry) => {
            #[cfg(feature = "ui")]
            if crate::cache_page::accepts_html(headers) {
                return crate::cache_page::payload(&entry, ctx.request_id.as_uuid())
                    .map(IntoResponse::into_response);
            }
            Ok(Json(entry).into_response())
        }
        None => Err(ctx.not_found(format!("no cache entry for key {key}"))),
    }
}

/// `DELETE /api/cache/{key}`: audited single-entry delete.
pub async fn cache_delete(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let key = cache_key(&ctx, &key)?;
    if !state
        .store()
        .delete_cache(&key)
        .await
        .map_err(|e| ctx.store(&e))?
    {
        return Err(ctx.not_found(format!("no cache entry for key {key}")));
    }
    write_audit(
        state.store(),
        &ctx,
        &headers,
        "cache.delete",
        key.to_string(),
        json!({}),
    )
    .await?;
    Ok(Json(json!({ "deleted": true, "key": key.as_str() })))
}

/// `DELETE /api/cache?expired=true|all=true`: bulk delete, exactly one flag
/// required (both destructive variants write an audit row).
pub async fn cache_bulk_delete(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<Value>, ApiError> {
    let params = QueryParams::parse(uri.query(), &ctx)?;
    params.allow(&ctx, &["expired", "all"])?;
    let expired = params.flag(&ctx, "expired")?;
    let all = params.flag(&ctx, "all")?;
    let (action, removed) = match (expired, all) {
        (true, false) => (
            "cache.evict_expired",
            state
                .store()
                .evict_expired(
                    state.with_config(|c| std::time::Duration::from_secs(c.cache.stale_grace_s)),
                )
                .await
                .map_err(|e| ctx.store(&e))?,
        ),
        (false, true) => (
            "cache.clear",
            state
                .store()
                .clear_cache()
                .await
                .map_err(|e| ctx.store(&e))?,
        ),
        _ => {
            return Err(ctx.bad_request("exactly one of `expired=true` or `all=true` is required"));
        }
    };
    write_audit(
        state.store(),
        &ctx,
        &headers,
        action,
        "cache_entries".to_string(),
        json!({ "removed": removed }),
    )
    .await?;
    Ok(Json(json!({ "removed": removed })))
}
