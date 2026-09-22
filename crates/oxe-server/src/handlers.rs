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
use chrono::{DateTime, NaiveDate, Utc};
use oxe_core::{
    AuditFilter, AuditRow, CacheKey, ClickRow, EngineId, HistoryFilter, HistoryItem, PipelineError,
    SafeSearch, SearchRequest, SearchResponse, StatsSnapshot, Store, TimeRange,
    config::{Config, system_env},
};
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
/// `engines` is a comma-separated pin (`engines=replay,ddgs`); a non-empty
/// pin that matches no configured engine is 400 `unknown_engines` (even if
/// no engines are configured), an empty pin or zero configured engines is
/// 503 `no_engines`, and an all-failed fan-out is 502 `upstream_failed`.
pub async fn search(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Json<SearchResponse>, ApiError> {
    search_inner(&state, &ctx, &uri)
        .await
        .map(|(_req, resp)| Json(resp))
}

/// Shared search execution used by `GET /api/search` and the HTML/HTMX page.
/// Returns the canonical [`SearchRequest`] alongside the response so callers
/// can compute the cache key and pagination URL without re-parsing params.
pub(crate) async fn search_inner(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
) -> Result<(SearchRequest, SearchResponse), ApiError> {
    let params = QueryParams::parse(uri.query(), ctx)?;
    params.allow(
        ctx,
        &["q", "page", "lang", "time_range", "safesearch", "engines"],
    )?;
    let req = SearchRequest {
        q: params.required(ctx, "q")?.to_string(),
        page: params.page(ctx)?,
        lang: params.get("lang").map(str::to_string),
        time_range: params
            .get("time_range")
            .map(|v| v.parse::<TimeRange>().map_err(|e| ctx.bad_request(e)))
            .transpose()?,
        safesearch: params
            .get("safesearch")
            .map(|v| v.parse::<SafeSearch>().map_err(|e| ctx.bad_request(e)))
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
        // A pin that selected nothing is the client's error; an empty
        // configured set is the operator's.
        Err(PipelineError::NoEngines) if req.engines.is_some() => Err(ctx.err(
            StatusCode::BAD_REQUEST,
            "unknown_engines",
            "engines pin matched no configured engine",
        )),
        Err(PipelineError::NoEngines) => Err(ctx.err(
            StatusCode::SERVICE_UNAVAILABLE,
            "no_engines",
            "no search engines configured",
        )),
        Err(e @ PipelineError::AllEnginesFailed(_)) => {
            Err(ctx.err(StatusCode::BAD_GATEWAY, "upstream_failed", e.to_string()))
        }
        // Admission overflow with no stale row to serve (W1-07): 429 +
        // `Retry-After`. W1-08 maps the same variant for MCP.
        Err(PipelineError::RateLimited { retry_after_s }) => Err(ctx
            .err(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                format!("admission queue saturated; retry after {retry_after_s}s"),
            )
            .with_retry_after(retry_after_s)),
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
/// The `oxe_cache_entries` gauge cell is refreshed from the store before
/// each scrape so the pull model reports a live value.
pub async fn metrics(State(state): State<AppState>) -> Response {
    state.metrics().refresh_cache().await;
    match state.metrics().render() {
        Ok(body) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, METRICS_CONTENT_TYPE)],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("metrics encode: {e}") })),
        )
            .into_response(),
    }
}

/// `GET /api/cache?limit&offset`: cache admin listing (includes
/// expired-but-not-yet-evicted rows).
pub async fn cache_list(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Json<Vec<oxe_core::CachedSearch>>, ApiError> {
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
) -> Result<Json<oxe_core::CachedSearch>, ApiError> {
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
    let tree = toml::from_str::<toml::Value>(text).map_err(|e| {
        ctx.err(
            StatusCode::BAD_REQUEST,
            "invalid_config",
            format!("TOML: {e}"),
        )
    })?;

    // In-memory validation: resolve `${...}` templates, apply `OXE_*` env
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

    /// Present and non-empty.
    pub(crate) fn required<'a>(&'a self, ctx: &RequestCtx, key: &str) -> Result<&'a str, ApiError> {
        self.get(key)
            .filter(|v| !v.is_empty())
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
