//! History, telemetry and ops handlers: `/api/history`, the click beacon,
//! `/api/stats`, `/metrics`, `/health` and `/api/audit`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use cauce_core::{
    AuditFilter, AuditRow, ClickRow, HistoryFilter, HistoryItem, StatsSnapshot, Store,
};
use chrono::Utc;
use serde_json::{Value, json};

use super::{MAX_LIMIT, QueryParams, write_audit};
use crate::app::AppState;
use crate::error::ApiError;
use crate::metrics::METRICS_CONTENT_TYPE;
use crate::middleware::RequestCtx;

/// `GET /api/history` row cap (W2-02: `limit` is clamped to the page's
/// 200-row budget).
pub(crate) const HISTORY_LIMIT: u32 = 200;

/// `GET /api/history?since&q&cached&limit`: searches and clicks, newest
/// first. `Accept: text/html` renders the history page through the same
/// handler (W2-02 settled input: one data path).
pub async fn history(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    #[cfg(feature = "ui")]
    if crate::html::prefers_html(&headers) {
        return crate::html::history_page(State(state), Extension(ctx), uri).await;
    }
    #[cfg(not(feature = "ui"))]
    let _ = &headers;
    history_inner(&state, &ctx, &uri, HISTORY_LIMIT)
        .await
        .map(|(_params, _filter, items)| Json(items).into_response())
}

/// Shared `GET /api/history` / `/history` query handling (W2-02): one
/// filter grammar and one data path (`Store::list_history`) for the JSON
/// route and the HTMX page. Returns the parsed params and the resolved
/// filter so the page can re-render the filter state and the cap note.
pub(crate) async fn history_inner(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
    default_limit: u32,
) -> Result<(QueryParams, HistoryFilter, Vec<HistoryItem>), ApiError> {
    let params = QueryParams::parse(uri.query(), ctx)?;
    params.allow(ctx, &["since", "q", "cached", "limit"])?;
    let filter = HistoryFilter {
        since: params.since(ctx, "since")?,
        // Blank `q=` is no filter at all — JSON and the page must agree,
        // and a blank substring would match every row anyway.
        q: params
            .get("q")
            .filter(|v| !v.trim().is_empty())
            .map(str::to_string),
        cached: params.flag(ctx, "cached")?,
        limit: params
            .u32(ctx, "limit", default_limit)?
            .clamp(1, HISTORY_LIMIT),
    };
    let items = state
        .store()
        .list_history(&filter)
        .await
        .map_err(|e| ctx.store(&e))?;
    Ok((params, filter, items))
}

/// `DELETE /api/history/{id}` (W2-02): audited history-row delete. The
/// `clicks` rows sharing its `query_hash` go with it only when it was the
/// last `search_log` row for that hash (the join the page renders).
pub async fn history_delete(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let id = id
        .parse::<i64>()
        .map_err(|_| ctx.bad_request(format!("invalid history id {id:?}")))?;
    let Some(outcome) = state
        .store()
        .delete_search_log(id)
        .await
        .map_err(|e| ctx.store(&e))?
    else {
        return Err(ctx.not_found(format!("no history row {id}")));
    };
    write_audit(
        state.store(),
        &ctx,
        &headers,
        "history.delete",
        id.to_string(),
        json!({ "query": outcome.query, "clicks_removed": outcome.clicks_removed }),
    )
    .await?;
    Ok(Json(json!({
        "deleted": true,
        "id": id,
        "clicks_removed": outcome.clicks_removed,
    })))
}

/// `POST /api/click`: the result-click beacon. `id`, `ts` and `client` are
/// server-owned (`ClickRow` docs); the body only supplies the click itself.
pub async fn click(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let mut row: ClickRow = serde_json::from_slice(&body)
        .map_err(|e| ctx.bad_request(format!("invalid click body: {e}")))?;
    row.id = None;
    row.ts = Utc::now();
    row.client = ctx.client.clone();
    state
        .store()
        .record_click(row)
        .await
        .map_err(|e| ctx.store(&e))?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/stats?days`: dashboard aggregates over the trailing window.
/// The store serves the persisted aggregates (day series stay sourced from
/// `search_log`); `merge_metrics` overlays the in-process engine percentiles
/// and admission counters (W1-09).
pub async fn stats(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Json<StatsSnapshot>, ApiError> {
    let params = QueryParams::parse(uri.query(), &ctx)?;
    params.allow(&ctx, &["days"])?;
    let days = params.u32(&ctx, "days", 7)?.clamp(1, 365);
    let mut snap = state.store().stats(days).await.map_err(|e| ctx.store(&e))?;
    snap.merge_metrics();
    // W3-05: the newest evals/results/*-engines.json, read per request —
    // a stats response never caches the file's contents.
    match cauce_core::evals::latest_report(&cauce_core::evals::results_dir()) {
        Ok(report) => snap.engine_eval = report,
        Err(e) => tracing::warn!(error = %e, "engine eval report unreadable; omitting from stats"),
    }
    Ok(Json(snap))
}

/// `GET /metrics` (W1-09): Prometheus text exposition of the process
/// metrics. Loopback-only through the default loopback bind; no auth.
/// The `cauce_cache_entries` gauge cell is refreshed from the store before
/// each scrape so the pull model reports a live value.
pub async fn metrics(State(state): State<AppState>) -> Response {
    state.metrics().refresh_cache().await;
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, METRICS_CONTENT_TYPE)],
        state.metrics().render(),
    )
        .into_response()
}

/// `GET /api/audit?since&actor&action&limit`.
pub async fn audit_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Json<Vec<AuditRow>>, ApiError> {
    audit_list_data(state.store().as_ref(), uri, &ctx)
        .await
        .map(|(_, rows)| Json(rows))
}

/// Shared audit query path for JSON and HTML responses.
///
/// Empty actor/action values are treated as unset for both surfaces. The
/// limit default and cap also stay identical regardless of content type.
pub(crate) async fn audit_list_data(
    store: &dyn Store,
    uri: Uri,
    ctx: &RequestCtx,
) -> Result<(AuditFilter, Vec<AuditRow>), ApiError> {
    let params = QueryParams::parse(uri.query(), ctx)?;
    params.allow(ctx, &["since", "actor", "action", "limit"])?;
    let filter = AuditFilter {
        since: params.since(ctx, "since")?,
        actor: params
            .get("actor")
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        action: params
            .get("action")
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        limit: params.u32(ctx, "limit", 50)?.clamp(1, MAX_LIMIT),
    };
    let rows = store
        .list_audit(&filter)
        .await
        .map_err(|error| ctx.store(&error))?;
    Ok((filter, rows))
}

/// `GET /health`: liveness plus store connectivity. The pipeline degrades
/// store failures to cache misses on purpose, so this endpoint is the place
/// store trouble surfaces: 200 `{status:"ok"}` vs 503 `{status:"degraded"}`.
pub async fn health(State(state): State<AppState>) -> Response {
    match state.store().list_cache(1, 0).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION"),
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "degraded",
                "error": e.to_string(),
            })),
        )
            .into_response(),
    }
}
