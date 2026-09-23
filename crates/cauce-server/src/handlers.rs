//! Route handlers for the wave-0 JSON surface.
//!
//! Every handler takes the [`RequestCtx`] extension the middleware installs
//! and returns errors through [`ApiError`], so every response (success or
//! not) is traceable by `request_id`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;

use axum::Extension;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use cauce_core::{
    AuditFilter, AuditRow, CacheKey, ClickRow, EngineError, EngineHealthRow, EngineId,
    HistoryFilter, HistoryItem, PipelineError, SafeSearch, SearchRequest, SearchResponse,
    StatsSnapshot, Store, TimeRange,
    config::{Config, system_env},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};

use crate::app::AppState;
use crate::error::ApiError;
use crate::metrics::METRICS_CONTENT_TYPE;
use crate::middleware::RequestCtx;
use crate::observability::audit;

/// Cap on caller-supplied `limit`/`offset`-style page sizes.
const MAX_LIMIT: u32 = 1_000;

/// `GET /api/search?q&page&lang&time_range&safesearch&engines`.
///
/// `engines` is a comma-separated pin (`engines=replay,ddgs`); a pin
/// naming any id outside the configured set is 400 `unknown_engines` —
/// the message lists the rejected ids and the configured set (issue #90
/// strict contract, even if no engines are configured) — an empty pin or
/// zero configured engines is 503 `no_engines`, and an all-failed fan-out
/// is 502 `upstream_failed`.
///
/// `Accept: text/html` renders the inline result-list fragment the
/// engines page's test query swaps in (W2-05) — same handler, negotiated.
#[cfg_attr(not(feature = "ui"), allow(unused_variables))]
pub async fn search(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    #[cfg(feature = "ui")]
    if crate::html::accepts_html(&headers) {
        return crate::html::search_fragment(&state, &ctx, &uri, &headers).await;
    }
    search_inner(&state, &ctx, &uri)
        .await
        .map(|(_req, resp)| Json(resp).into_response())
}

/// Shared search execution used by `GET /api/search` and the HTML/HTMX page.
/// Returns the canonical [`SearchRequest`] alongside the response so callers
/// can compute the cache key and pagination URL without re-parsing params.
pub(crate) async fn search_inner(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
) -> Result<(SearchRequest, SearchResponse), ApiError> {
    search_inner_classed(state, ctx, uri)
        .await
        .map_err(|(e, _class)| e)
}

/// [`search_inner`] plus a display class on the error side (`blocked`,
/// `timeout`, `no results`, `breaker open`, ...) — the engines page's
/// inline test fragment renders it as the meta line on failure (screen
/// spec `engines.md`: "the engine's error class"). JSON callers discard
/// the class through [`search_inner`].
pub(crate) async fn search_inner_classed(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
) -> Result<(SearchRequest, SearchResponse), (ApiError, &'static str)> {
    use crate::strings::engines as copy;

    let params = QueryParams::parse(uri.query(), ctx).map_err(|e| (e, copy::TEST_BAD_REQUEST))?;
    params
        .allow(
            ctx,
            &["q", "page", "lang", "time_range", "safesearch", "engines"],
        )
        .map_err(|e| (e, copy::TEST_BAD_REQUEST))?;
    let req = SearchRequest {
        q: params
            .required(ctx, "q")
            .map_err(|e| (e, copy::TEST_BAD_REQUEST))?
            .to_string(),
        page: params.page(ctx).map_err(|e| (e, copy::TEST_BAD_REQUEST))?,
        lang: params.get("lang").map(str::to_string),
        time_range: params
            .get("time_range")
            .map(|v| {
                v.parse::<TimeRange>()
                    .map_err(|e| (ctx.bad_request(e), copy::TEST_BAD_REQUEST))
            })
            .transpose()?,
        safesearch: params
            .get("safesearch")
            .map(|v| {
                v.parse::<SafeSearch>()
                    .map_err(|e| (ctx.bad_request(e), copy::TEST_BAD_REQUEST))
            })
            .transpose()?
            .unwrap_or_default(),
        engines: params
            .get("engines")
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(EngineId::from)
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty()),
        client: ctx.client.clone(),
    };
    match state
        .pipeline()
        .search_with_id(&req, ctx.request_id.as_uuid())
        .await
    {
        Ok(resp) => Ok((req, resp)),
        // A pin naming ids outside the configured set is the client's
        // error; the message names the offenders and the configured set
        // (issue #90).
        Err(e @ PipelineError::UnknownEngines { .. }) => Err((
            ctx.err(StatusCode::BAD_REQUEST, "unknown_engines", e.to_string()),
            copy::TEST_UNKNOWN_ENGINES,
        )),
        // A bare `Some([])` pin that selected nothing is the client's
        // error; an empty configured set is the operator's.
        Err(PipelineError::NoEngines) if req.engines.is_some() => Err((
            ctx.err(
                StatusCode::BAD_REQUEST,
                "unknown_engines",
                "engines pin matched no configured engine",
            ),
            copy::TEST_UNKNOWN_ENGINES,
        )),
        Err(PipelineError::NoEngines) => Err((
            ctx.err(
                StatusCode::SERVICE_UNAVAILABLE,
                "no_engines",
                "no search engines configured",
            ),
            copy::TEST_NO_ENGINES,
        )),
        Err(PipelineError::AllEnginesFailed(failures)) => {
            let class = engine_error_class(&failures);
            Err((
                ctx.err(
                    StatusCode::BAD_GATEWAY,
                    "upstream_failed",
                    PipelineError::AllEnginesFailed(failures).to_string(),
                ),
                class,
            ))
        }
        // Admission overflow with no stale row to serve (W1-07): 429 +
        // `Retry-After`. W1-08 maps the same variant for MCP.
        Err(PipelineError::RateLimited { retry_after_s }) => Err((
            ctx.err(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                format!("admission queue saturated; retry after {retry_after_s}s"),
            )
            .with_retry_after(retry_after_s),
            copy::TEST_RATE_LIMITED,
        )),
        // Every matched engine was breaker-skipped (W1-06): temporary,
        // so 503 regardless of pinning — the pin *did* match.
        Err(e @ PipelineError::BreakerOpen(_)) => Err((
            ctx.err(
                StatusCode::SERVICE_UNAVAILABLE,
                "breaker_open",
                e.to_string(),
            ),
            copy::TEST_BREAKER_OPEN,
        )),
    }
}

/// The error class an engines-page test query renders for an all-failed
/// fan-out: the single engine's [`EngineError`] class when exactly one
/// failed (a pinned test), `upstream failed` otherwise.
fn engine_error_class(failures: &[(EngineId, EngineError)]) -> &'static str {
    use crate::strings::engines as copy;

    if failures.len() != 1 {
        return copy::TEST_UPSTREAM;
    }
    match failures[0].1 {
        EngineError::Blocked => copy::TEST_BLOCKED,
        EngineError::Timeout => copy::TEST_TIMEOUT,
        EngineError::NoResults => copy::TEST_NO_RESULTS,
        EngineError::RateLimited => copy::TEST_RATE_LIMITED,
        EngineError::Parse(_) => copy::TEST_PARSE,
        EngineError::Transport(_) => copy::TEST_TRANSPORT,
    }
}

/// `GET /api/history?since&q&limit`: searches and clicks, newest first.
pub async fn history(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Json<Vec<HistoryItem>>, ApiError> {
    let params = QueryParams::parse(uri.query(), &ctx)?;
    params.allow(&ctx, &["since", "q", "limit"])?;
    let filter = HistoryFilter {
        since: params.since(&ctx, "since")?,
        q: params.get("q").map(str::to_string),
        limit: params.u32(&ctx, "limit", 50)?.clamp(1, MAX_LIMIT),
    };
    state
        .store()
        .list_history(&filter)
        .await
        .map(Json)
        .map_err(|e| ctx.store(&e))
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

/// `GET /api/cache?limit&offset`: cache admin listing (includes
/// expired-but-not-yet-evicted rows).
pub async fn cache_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Json<Vec<cauce_core::CachedSearch>>, ApiError> {
    let params = QueryParams::parse(uri.query(), &ctx)?;
    params.allow(&ctx, &["limit", "offset"])?;
    let limit = params.u32(&ctx, "limit", 50)?.clamp(1, MAX_LIMIT);
    let offset = params.u32(&ctx, "offset", 0)?;
    state
        .store()
        .list_cache(limit, offset)
        .await
        .map(Json)
        .map_err(|e| ctx.store(&e))
}

/// `GET /api/cache/{key}`: one entry by hex `CacheKey` (400 malformed,
/// 404 absent).
pub async fn cache_get(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(key): Path<String>,
) -> Result<Json<cauce_core::CachedSearch>, ApiError> {
    let key = cache_key(&ctx, &key)?;
    match state
        .store()
        .get_cache(&key)
        .await
        .map_err(|e| ctx.store(&e))?
    {
        Some(entry) => Ok(Json(entry)),
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
                .evict_expired()
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

/// `GET /api/engines` (W1-06) and `GET /engines` (W2-05) share this one
/// handler — the settled input is that pages read the same data path as
/// `/api/*`, and the screen spec (`engines.md`) requires the JSON body to
/// carry the same per-engine fields the cards show. `Accept: text/html`
/// renders the engines page; anything else gets the [`EngineView`] rows.
///
/// Each row flattens the W1-06 health wire shape (`engine`, `ewma_ms`,
/// `failures`, `breaker`, `breaker_until`, `last_ok_at`, `last_error` —
/// straight from the pipeline's tracker, fresher than the debounced
/// `engine_health` table) with the card fields `kind`, `tier`, `enabled`,
/// `configured`, `live`, `p95_ms`, `reliability_pct`, `requests_today`.
#[cfg_attr(not(feature = "ui"), allow(unused_variables))]
pub async fn engines_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    #[cfg(feature = "ui")]
    if crate::html::accepts_html(&headers) {
        return crate::engines_page::page(&state, &ctx)
            .await
            .map(IntoResponse::into_response);
    }
    Ok(Json(engine_views(&state).await?).into_response())
}

/// One row of the shared `/api/engines` + `/engines` data plane: the live
/// health row plus every field an engines-page card renders.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineView {
    /// W1-06 health fields, flattened so the wire names are unchanged.
    #[serde(flatten)]
    pub health: EngineHealthRow,
    /// `declarative` | `exec` | `replay`; `"-"` for health-only leftovers.
    pub kind: String,
    /// Effective tier: the live engine's own, else the `[[engines]]`
    /// override. Absent for health-only leftovers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<u8>,
    /// Resolved `enabled` flag; engines live but absent from the resolved
    /// config count as enabled.
    pub enabled: bool,
    /// The resolved config names this engine (file entry or built-in).
    pub configured: bool,
    /// Live in the running pipeline's fan-out set.
    pub live: bool,
    /// Whole-call p95 from the in-process metrics registry (W1-09);
    /// absent until the engine has served a request this process.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_ms: Option<u32>,
    /// `ok / requests * 100` from the same registry; same absent-until-seen
    /// rule as `p95_ms`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reliability_pct: Option<f64>,
    /// Searches that named this engine since UTC midnight (`search_log`).
    pub requests_today: u64,
}

/// The shared `/api/engines` + `/engines` data plane: one row per engine
/// in the union of configured entries, live pipeline engines and known
/// health rows, sorted by id.
pub(crate) async fn engine_views(state: &AppState) -> Result<Vec<EngineView>, ApiError> {
    use std::collections::{BTreeMap, BTreeSet};

    let entries = state.with_config(|cfg| cfg.engines.clone());
    let live: Vec<(EngineId, cauce_core::Tier)> = state
        .pipeline()
        .engines()
        .iter()
        .map(|e| (e.id(), e.tier()))
        .collect();
    let health: BTreeMap<String, EngineHealthRow> = state
        .pipeline()
        .health()
        .snapshot()
        .into_iter()
        .map(|r| (r.engine.to_string(), r))
        .collect();
    let metrics: BTreeMap<String, cauce_core::metrics::EngineMetricStats> =
        cauce_core::metrics::engine_stats()
            .into_iter()
            .map(|m| (m.engine.to_string(), m))
            .collect();

    // "Requests today": today's `search_log` rows naming the engine — the
    // same plane `/api/history` reads. The cap matches that handler's
    // `limit` clamp; a heavier day just undercounts.
    let midnight = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|t| t.and_utc());
    let history = state
        .store()
        .list_history(&HistoryFilter {
            since: midnight,
            q: None,
            limit: MAX_LIMIT,
        })
        .await
        .map_err(|e| ApiError::store(&e))?;
    let mut requests_today: BTreeMap<String, u64> = BTreeMap::new();
    for item in history {
        if let HistoryItem::Search(row) = item {
            for engine in &row.engines {
                *requests_today.entry(engine.to_string()).or_insert(0) += 1;
            }
        }
    }

    let mut ids: BTreeSet<String> = BTreeSet::new();
    ids.extend(entries.iter().map(|e| e.id.to_string()));
    ids.extend(live.iter().map(|(id, _)| id.to_string()));
    ids.extend(health.keys().cloned());

    Ok(ids
        .into_iter()
        .map(|id| {
            let entry = entries.iter().find(|e| e.id.as_str() == id);
            let live_tier = live.iter().find(|(e, _)| e.as_str() == id).map(|(_, t)| *t);
            let kind = entry
                .map(|e| match e.kind {
                    cauce_core::config::EngineKind::Declarative => "declarative",
                    cauce_core::config::EngineKind::Exec => "exec",
                    cauce_core::config::EngineKind::Replay => "replay",
                })
                .unwrap_or(if live_tier.is_some() {
                    // Live but unnamed by the resolved config: a spec
                    // auto-registered engine, always declarative.
                    "declarative"
                } else {
                    "-"
                })
                .to_string();
            let metrics = metrics.get(&id).filter(|m| m.requests > 0);
            EngineView {
                health: health.get(&id).cloned().unwrap_or_else(|| EngineHealthRow {
                    engine: EngineId::from(id.as_str()),
                    ewma_ms: 0.0,
                    failures: 0,
                    breaker: cauce_core::BreakerState::Closed,
                    breaker_until: None,
                    last_ok_at: None,
                    last_error: None,
                }),
                kind,
                tier: live_tier
                    .or_else(|| entry.and_then(|e| e.tier))
                    .map(|t| t.as_u8()),
                enabled: entry.map(|e| e.enabled).unwrap_or(live_tier.is_some()),
                configured: entry.is_some(),
                live: live_tier.is_some(),
                p95_ms: metrics.map(|m| m.total.p95_ms),
                reliability_pct: metrics.map(|m| m.reliability_pct),
                requests_today: *requests_today.get(&id).unwrap_or(&0),
            }
        })
        .collect())
}

/// `POST /api/engines/{id}/reset` (W1-06): clear EWMA/failures and put the
/// breaker into `HalfOpen` (W2-05: the next call is the single probe, so a
/// reset engine re-earns trust instead of rejoining the fan-out at full
/// concurrency). Audited (`engine.reset`, with the previous and new
/// breaker in `details`); the fresh row is persisted immediately rather
/// than through the 1/s debounce.
///
/// HTMX callers (`HX-Request` header, the engines page's reset button) get
/// the re-rendered card partial for `hx-swap="outerHTML"` instead of the
/// JSON row — same data plane, negotiated like `html::search` does.
pub async fn engine_reset(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let id = EngineId::from(id);
    let Some((previous, row)) = state.pipeline().health().reset(&id) else {
        return Err(ctx.not_found(format!("no such engine {id}")));
    };
    state
        .store()
        .put_health(&row)
        .await
        .map_err(|e| ctx.store(&e))?;
    write_audit(
        state.store(),
        &ctx,
        &headers,
        "engine.reset",
        id.to_string(),
        json!({ "from": previous, "to": row.breaker }),
    )
    .await?;
    #[cfg(feature = "ui")]
    if crate::html::is_htmx(&headers) {
        return crate::engines_page::card(&state, &id, ctx.request_id.as_uuid(), None).await;
    }
    Ok(Json(row).into_response())
}

/// `POST /api/engines/{id}/enable` (W2-05): set `enabled = true` on the
/// engine's config entry. See [`engine_set_enabled`].
pub async fn engine_enable(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    engine_set_enabled(&state, &ctx, &headers, EngineId::from(id), true).await
}

/// `POST /api/engines/{id}/disable` (W2-05): set `enabled = false` on the
/// engine's config entry. See [`engine_set_enabled`].
pub async fn engine_disable(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    engine_set_enabled(&state, &ctx, &headers, EngineId::from(id), false).await
}

/// Shared enable/disable write (screen spec `engines.md`:
/// `POST /api/engines/<id>/enable` or `/disable`, audited as
/// `engine.enable` / `engine.disable`).
///
/// The mutation patches the raw (pre-interpolation) file tree in place so
/// `${env:...}`/`${file:...}` templates persist verbatim, validates the
/// candidate in memory, then writes + swaps the live config — the same
/// discipline as `PUT /api/config`. Engines absent from the file get a
/// synthesized `[[engines]]` entry (built-ins serialize their full typed
/// entry; auto-registered declarative specs get `{id, kind = "declarative"}`,
/// which resolves the spec by id). Unknown ids 404. The running pipeline
/// keeps its engine set until restart, so the response carries
/// `effective_after_restart: true`, and the card fragment an HTMX caller
/// swaps in carries the `saved; applies after restart` hint.
async fn engine_set_enabled(
    state: &AppState,
    ctx: &RequestCtx,
    headers: &HeaderMap,
    id: EngineId,
    enabled: bool,
) -> Result<Response, ApiError> {
    // 404 before touching the file: a togglable engine is one the resolved
    // config names (file entry or built-in) or one live in the pipeline
    // (spec auto-registered). Persisted health rows for removed engines
    // cannot be re-enabled — there is no entry to flip.
    let known = state.with_config(|cfg| cfg.engine(id.as_str()).is_some())
        || state.pipeline().engines().iter().any(|e| e.id() == id);
    if !known {
        return Err(ctx.not_found(format!("no such engine {id}")));
    }

    // All sync file IO inside the lock; nothing awaits in the closure
    // (same discipline as `config_put`).
    state.with_config(|cfg| -> Result<(), ApiError> {
        let mut tree = match cfg.raw_tree() {
            Some(raw) => raw.clone(),
            // `Config::default()` has no file layer; the display tree is a
            // complete schema-valid tree to patch (identical to what
            // `Config::save` would write as a first file).
            None => cfg.display_tree().map_err(|e| {
                ctx.err(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string())
            })?,
        };
        set_engine_enabled(&mut tree, &id, enabled, cfg, state.pipeline())
            .map_err(|e| ctx.bad_request(e))?;
        let new_cfg = Config::from_raw(&tree, &system_env()).map_err(|e| {
            ctx.err(
                StatusCode::BAD_REQUEST,
                "invalid_config",
                format!("invalid config: {e}"),
            )
        })?;
        new_cfg.save().map_err(|e| {
            ctx.err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                format!("cannot write {}: {e}", new_cfg.config_path().display()),
            )
        })?;
        *cfg = new_cfg;
        Ok(())
    })?;

    write_audit(
        state.store(),
        ctx,
        headers,
        if enabled {
            "engine.enable"
        } else {
            "engine.disable"
        },
        id.to_string(),
        json!({ "enabled": enabled }),
    )
    .await?;

    #[cfg(feature = "ui")]
    if crate::html::is_htmx(headers) {
        return crate::engines_page::card(
            state,
            &id,
            ctx.request_id.as_uuid(),
            Some(crate::strings::engines::TOGGLE_SAVED),
        )
        .await;
    }
    Ok(Json(json!({
        "id": id.to_string(),
        "enabled": enabled,
        "effective_after_restart": true,
    }))
    .into_response())
}

/// Set `enabled` on the `engines` entry for `id` inside the raw file-layer
/// tree, appending a synthesized `[[engines]]` table when the file does not
/// name the engine (built-ins and spec auto-registered engines only appear
/// in the resolved config, not in `config.toml`).
fn set_engine_enabled(
    tree: &mut toml::Value,
    id: &EngineId,
    enabled: bool,
    cfg: &Config,
    pipeline: &cauce_core::SearchPipeline,
) -> Result<(), String> {
    let table = tree
        .as_table_mut()
        .ok_or_else(|| "config root is not a TOML table".to_string())?;
    let entries = table
        .entry("engines")
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    let arr = entries
        .as_array_mut()
        .ok_or_else(|| "config key \"engines\" is not an array".to_string())?;
    for entry in arr.iter_mut() {
        if entry.get("id").and_then(toml::Value::as_str) == Some(id.as_str()) {
            let entry_table = entry
                .as_table_mut()
                .ok_or_else(|| format!("engines entry {id:?} is not a table"))?;
            entry_table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
            return Ok(());
        }
    }
    // Not in the file: a built-in (`replay`, `ddgs`) or a spec
    // auto-registered engine. Built-ins serialize their typed entry —
    // every field of a built-in is non-secret (env maps are empty); a
    // file-defined engine never reaches this branch.
    if let Some(entry) = cfg.engine(id.as_str()) {
        let mut value = toml::Value::try_from(entry.clone())
            .map_err(|e| format!("cannot serialize engine {id:?}: {e}"))?;
        if let Some(t) = value.as_table_mut() {
            t.insert("enabled".to_string(), toml::Value::Boolean(enabled));
        }
        arr.push(value);
        return Ok(());
    }
    // Auto-registered spec engines are always declarative; the entry id
    // resolves the spec (path or embedded name) at next load.
    if pipeline.engines().iter().any(|e| e.id() == *id) {
        let mut t = toml::Table::new();
        t.insert("id".to_string(), toml::Value::String(id.to_string()));
        t.insert(
            "kind".to_string(),
            toml::Value::String("declarative".to_string()),
        );
        t.insert("enabled".to_string(), toml::Value::Boolean(enabled));
        arr.push(toml::Value::Table(t));
        return Ok(());
    }
    Err(format!("no such engine {id}"))
}

/// `GET /api/audit?since&actor&action&limit`.
pub async fn audit_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Json<Vec<AuditRow>>, ApiError> {
    let params = QueryParams::parse(uri.query(), &ctx)?;
    params.allow(&ctx, &["since", "actor", "action", "limit"])?;
    let filter = AuditFilter {
        since: params.since(&ctx, "since")?,
        actor: params.get("actor").map(str::to_string),
        action: params.get("action").map(str::to_string),
        limit: params.u32(&ctx, "limit", 50)?.clamp(1, MAX_LIMIT),
    };
    state
        .store()
        .list_audit(&filter)
        .await
        .map(Json)
        .map_err(|e| ctx.store(&e))
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

/// `GET /api/config`: the effective config, redacted. `Config`'s `Serialize`
/// impl renders `${env:...}`/`${file:...}` templates literally, so resolved
/// secrets can never appear in the response.
pub async fn config_get(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> Result<Json<Value>, ApiError> {
    state
        .with_config(|cfg| serde_json::to_value(cfg))
        .map(Json)
        .map_err(|e| ctx.err(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()))
}

/// `PUT /api/config`: replace the config file with the submitted TOML tree.
///
/// Validation runs in-memory against the current process environment *before*
/// any write, so a crash or `kill -9` cannot leave `config.toml` in an
/// unbootable state. On success the new raw tree is written atomically and
/// `state.config` is swapped; the running pipeline/engines still use the
/// values they were started with, so the response carries
/// `effective_after_restart: true`.
pub async fn config_put(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let text = std::str::from_utf8(&body)
        .map_err(|_| ctx.bad_request("PUT /api/config expects a UTF-8 TOML body"))?;
    let mut tree = toml::from_str::<toml::Value>(text).map_err(|e| {
        ctx.err(
            StatusCode::BAD_REQUEST,
            "invalid_config",
            format!("TOML: {e}"),
        )
    })?;

    // `<redacted>` leaves from a `GET /api/config` roundtrip get their real
    // values back from the current config; a literal `<redacted>` with no
    // current secret behind it is rejected rather than persisted.
    let restored = state
        .with_config(|cfg| cfg.restore_redacted(&mut tree))
        .map_err(|e| {
            ctx.err(
                StatusCode::BAD_REQUEST,
                "invalid_config",
                format!("invalid config: {e}"),
            )
        })?;
    if !restored.is_empty() {
        tracing::info!(paths = ?restored, "restored redacted config secrets on PUT");
    }

    // In-memory validation: resolve `${...}` templates, apply `CAUCE_*` env
    // overrides, check schema and engine pinning. The original file is not
    // touched until we know the candidate is loadable.
    let new_cfg = Config::from_raw(&tree, &system_env()).map_err(|e| {
        ctx.err(
            StatusCode::BAD_REQUEST,
            "invalid_config",
            format!("invalid config: {e}"),
        )
    })?;

    // All sync file IO inside the lock; nothing awaits in the closure.
    let loaded = state.with_config(|cfg| {
        if let Err(e) = new_cfg.save() {
            return Err(ctx.err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                format!("cannot write {}: {e}", new_cfg.config_path().display()),
            ));
        }
        *cfg = new_cfg.clone();
        Ok(new_cfg)
    })?;

    write_audit(
        state.store(),
        &ctx,
        &headers,
        "config.put",
        loaded.config_path().display().to_string(),
        json!({}),
    )
    .await?;
    let mut body = serde_json::to_value(&loaded)
        .map_err(|e| ctx.err(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()))?;
    if let Value::Object(m) = &mut body {
        m.insert("effective_after_restart".to_string(), json!(true));
    }
    Ok(Json(body))
}

/// Emit + persist one audit row (observability helper: JSONL event first,
/// `audit` table write after). `actor` honours `X-Actor`.
async fn write_audit(
    store: &Arc<dyn Store>,
    ctx: &RequestCtx,
    headers: &HeaderMap,
    action: &str,
    target: String,
    details: Value,
) -> Result<(), ApiError> {
    audit(
        store.as_ref(),
        AuditRow {
            id: None,
            ts: Utc::now(),
            actor: ctx.actor(headers),
            action: action.to_string(),
            target,
            details,
            request_id: Some(ctx.request_id.as_uuid()),
        },
    )
    .await
    .map_err(|e| ctx.store(&e))
}

fn cache_key(ctx: &RequestCtx, raw: &str) -> Result<CacheKey, ApiError> {
    raw.parse::<CacheKey>().map_err(|e| ctx.bad_request(e))
}

/// A query string decoded into ordered `(key, value)` pairs. Decoding is
/// `url::form_urlencoded` (percent-escapes, `+` for space); duplicates and
/// unknown keys are 400s instead of silent surprises.
pub(crate) struct QueryParams(Vec<(String, String)>);

impl QueryParams {
    pub(crate) fn parse(raw: Option<&str>, ctx: &RequestCtx) -> Result<Self, ApiError> {
        let mut pairs = Vec::new();
        for (k, v) in url::form_urlencoded::parse(raw.unwrap_or_default().as_bytes()) {
            if pairs.iter().any(|(seen, _)| *seen == k) {
                return Err(ctx.bad_request(format!("duplicate query parameter {k:?}")));
            }
            pairs.push((k.into_owned(), v.into_owned()));
        }
        Ok(Self(pairs))
    }

    /// 400 when a key outside `allowed` is present (the inbound
    /// `deny_unknown_fields` contract applied to the query string).
    pub(crate) fn allow(&self, ctx: &RequestCtx, allowed: &[&str]) -> Result<(), ApiError> {
        for (k, _) in &self.0 {
            if !allowed.contains(&k.as_str()) {
                return Err(ctx.bad_request(format!("unknown query parameter {k:?}")));
            }
        }
        Ok(())
    }

    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Present and non-blank: `q=` and whitespace-only values are 400s
    /// just like an absent parameter — a blank `q` would otherwise run a
    /// fan-out on the empty normalized query (#89).
    pub(crate) fn required<'a>(&'a self, ctx: &RequestCtx, key: &str) -> Result<&'a str, ApiError> {
        self.get(key)
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| ctx.bad_request(format!("missing required parameter {key:?}")))
    }

    pub(crate) fn u32(&self, ctx: &RequestCtx, key: &str, default: u32) -> Result<u32, ApiError> {
        match self.get(key) {
            None => Ok(default),
            Some(v) => v
                .parse::<u32>()
                .map_err(|_| ctx.bad_request(format!("invalid {key} {v:?}: expected a u32"))),
        }
    }

    /// `page` is 1-based; `page=0` and non-numeric values are 400s.
    pub(crate) fn page(&self, ctx: &RequestCtx) -> Result<u8, ApiError> {
        match self.get("page") {
            None => Ok(1),
            Some(v) => v
                .parse::<u8>()
                .ok()
                .filter(|p| *p >= 1)
                .ok_or_else(|| ctx.bad_request(format!("invalid page {v:?}"))),
        }
    }

    /// Presence-style flag: `?expired`, `?expired=true|1|yes` are true;
    /// `?expired=false|0|no` is false; anything else is a 400.
    pub(crate) fn flag(&self, ctx: &RequestCtx, key: &str) -> Result<bool, ApiError> {
        match self.get(key) {
            None => Ok(false),
            Some(v) => match v.to_ascii_lowercase().as_str() {
                "" | "true" | "1" | "yes" => Ok(true),
                "false" | "0" | "no" => Ok(false),
                _ => Err(ctx.bad_request(format!("invalid {key} {v:?}: expected a boolean"))),
            },
        }
    }

    /// `since` accepts RFC 3339 (`2026-10-01T12:00:00Z`) or a bare
    /// `YYYY-MM-DD` date (interpreted as that UTC midnight).
    pub(crate) fn since(
        &self,
        ctx: &RequestCtx,
        key: &str,
    ) -> Result<Option<DateTime<Utc>>, ApiError> {
        let Some(v) = self.get(key) else {
            return Ok(None);
        };
        if let Ok(dt) = DateTime::parse_from_rfc3339(v) {
            return Ok(Some(dt.with_timezone(&Utc)));
        }
        if let Ok(day) = NaiveDate::parse_from_str(v, "%Y-%m-%d")
            && let Some(dt) = day.and_hms_opt(0, 0, 0)
        {
            return Ok(Some(dt.and_utc()));
        }
        Err(ctx.bad_request(format!(
            "invalid {key} {v:?}: expected RFC 3339 or YYYY-MM-DD"
        )))
    }
}
