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
    AuditFilter, AuditRow, CacheKey, ClickRow, EngineHealthRow, EngineId, HistoryFilter,
    HistoryItem, PipelineError, SafeSearch, SearchRequest, SearchResponse, StatsSnapshot, Store,
    TimeRange,
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
        // A pin naming ids outside the configured set is the client's
        // error; the message names the offenders and the configured set
        // (issue #90).
        Err(e @ PipelineError::UnknownEngines { .. }) => {
            Err(ctx.err(StatusCode::BAD_REQUEST, "unknown_engines", e.to_string()))
        }
        // A bare `Some([])` pin that selected nothing is the client's
        // error; an empty configured set is the operator's.
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
        // Every matched engine was breaker-skipped (W1-06): temporary,
        // so 503 regardless of pinning — the pin *did* match.
        Err(e @ PipelineError::BreakerOpen(_)) => Err(ctx.err(
            StatusCode::SERVICE_UNAVAILABLE,
            "breaker_open",
            e.to_string(),
        )),
    }
}

/// `GET /api/history?since&q&limit`: searches and clicks, newest first.
pub async fn history(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Json<Vec<HistoryItem>>, ApiError> {
    history_inner(&state, &ctx, &uri, 50)
        .await
        .map(|(_params, items)| Json(items))
}

/// Shared `GET /api/history` / `/history` query handling (W2-02): one
/// filter grammar and one data path (`Store::list_history`) for the JSON
/// route and the HTMX page. Returns the parsed params so the page can
/// re-render the current filter state.
pub(crate) async fn history_inner(
    state: &AppState,
    ctx: &RequestCtx,
    uri: &Uri,
    default_limit: u32,
) -> Result<(QueryParams, Vec<HistoryItem>), ApiError> {
    let params = QueryParams::parse(uri.query(), ctx)?;
    params.allow(ctx, &["since", "q", "limit"])?;
    let filter = HistoryFilter {
        since: params.since(ctx, "since")?,
        q: params.get("q").map(str::to_string),
        limit: params.u32(ctx, "limit", default_limit)?.clamp(1, MAX_LIMIT),
    };
    let items = state
        .store()
        .list_history(&filter)
        .await
        .map_err(|e| ctx.store(&e))?;
    Ok((params, items))
}

/// `DELETE /api/history/{id}` (W2-02): audited history-row delete. The
/// `search_log` row goes together with the `clicks` rows sharing its
/// `query_hash` (the join the page renders).
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

/// `GET /api/engines` (W1-06): every known engine with its live health —
/// EWMA latency, consecutive failures, breaker state — straight from the
/// pipeline's tracker (fresher than the debounced `engine_health` table).
pub async fn engines_list(State(state): State<AppState>) -> Json<Vec<EngineHealthRow>> {
    Json(state.pipeline().health().snapshot())
}

/// `POST /api/engines/{id}/reset` (W1-06): close the breaker and clear
/// EWMA/failures for one engine. Audited (`engine.reset`, with the
/// previous breaker in `details`); the fresh row is persisted immediately
/// rather than through the 1/s debounce.
pub async fn engine_reset(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<EngineHealthRow>, ApiError> {
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
        json!({ "from": previous, "to": "closed" }),
    )
    .await?;
    Ok(Json(row))
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

/// `PUT /api/config`: replace the config file with the submitted tree.
///
/// Two body encodings are accepted: the canonical TOML document and, for
/// the `/settings` page (W2-07), `application/x-www-form-urlencoded` fields
/// named after dotted config paths that [`crate::settings`] merges onto the
/// raw file tree. Both encodings share the rest of the pipeline below, so
/// there is one write path.
///
/// Validation runs in-memory against the current process environment *before*
/// any write, so a crash or `kill -9` cannot leave `config.toml` in an
/// unbootable state. On success the new raw tree is written atomically and
/// `state.config` is swapped; the running pipeline/engines still use the
/// values they were started with, so the response carries
/// `effective_after_restart: true`.
///
/// A form submit marked `HX-Request` gets an HTML fragment back (200 on both
/// success and validation failure, so htmx swaps it inline); everything else
/// gets the JSON body / envelope.
pub async fn config_put(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let form = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/x-www-form-urlencoded"));
    let result = config_put_inner(&state, &ctx, &headers, &body, form).await;
    #[cfg(feature = "ui")]
    if form && headers.get("hx-request").is_some() {
        // On a failed save, re-run the fields one by one so the fragment can
        // place an error line under each offending input.
        let pairs: Vec<(String, String)> =
            url::form_urlencoded::parse(std::str::from_utf8(&body).unwrap_or("").as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
        let submitted: Vec<String> = pairs.iter().map(|(k, _)| k.clone()).collect();
        let (engine_ids, field_errors) = state.with_config(|cfg| {
            let ids = cfg
                .engines
                .iter()
                .map(|e| e.id.to_string())
                .collect::<Vec<_>>();
            let errs = if result.is_err() {
                crate::settings::field_errors(cfg, &pairs)
            } else {
                Vec::new()
            };
            (ids, errs)
        });
        return Ok(crate::html::settings_status(
            result,
            field_errors,
            submitted,
            &engine_ids,
        ));
    }
    result.map(Json::into_response)
}

async fn config_put_inner(
    state: &AppState,
    ctx: &RequestCtx,
    headers: &HeaderMap,
    body: &Bytes,
    form: bool,
) -> Result<Json<Value>, ApiError> {
    let text = std::str::from_utf8(body)
        .map_err(|_| ctx.bad_request("PUT /api/config expects a UTF-8 TOML or form body"))?;
    // Snapshot the file layer before the merge so the audit row can name the
    // keys that actually changed (`{"changed": ["search.deadline_ms"]}`).
    let old_tree = state.with_config(|cfg| {
        cfg.raw_tree()
            .cloned()
            .or_else(|| cfg.display_tree().ok())
            .unwrap_or_else(|| toml::Value::Table(toml::Table::new()))
    });
    let mut tree = if form {
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(text.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        state
            .with_config(|cfg| crate::settings::merge_form_config(cfg, &pairs))
            .map_err(|e| {
                ctx.err(
                    StatusCode::BAD_REQUEST,
                    "invalid_config",
                    format!("invalid config: {e}"),
                )
            })?
    } else {
        toml::from_str::<toml::Value>(text).map_err(|e| {
            ctx.err(
                StatusCode::BAD_REQUEST,
                "invalid_config",
                format!("TOML: {e}"),
            )
        })?
    };
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

    // Computed after `restore_redacted`: a `<redacted>` leaf the restore
    // just filled back with the file's own secret is no change at all.
    let changed = changed_config_paths(&old_tree, &tree);

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
        ctx,
        headers,
        "config.put",
        loaded.config_path().display().to_string(),
        json!({"changed": changed}),
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

    /// `since` accepts RFC 3339 (`2026-10-01T12:00:00Z`), a bare
    /// `YYYY-MM-DD` date (interpreted as that UTC midnight) or a relative
    /// window token — `24h`, `7d`, `30d` (W2-02's history filter set) or
    /// `all` (no lower bound).
    pub(crate) fn since(
        &self,
        ctx: &RequestCtx,
        key: &str,
    ) -> Result<Option<DateTime<Utc>>, ApiError> {
        let Some(v) = self.get(key) else {
            return Ok(None);
        };
        match v {
            "24h" => return Ok(Some(Utc::now() - chrono::Duration::hours(24))),
            "7d" => return Ok(Some(Utc::now() - chrono::Duration::days(7))),
            "30d" => return Ok(Some(Utc::now() - chrono::Duration::days(30))),
            "all" => return Ok(None),
            _ => {}
        }
        if let Ok(dt) = DateTime::parse_from_rfc3339(v) {
            return Ok(Some(dt.with_timezone(&Utc)));
        }
        if let Ok(day) = NaiveDate::parse_from_str(v, "%Y-%m-%d")
            && let Some(dt) = day.and_hms_opt(0, 0, 0)
        {
            return Ok(Some(dt.and_utc()));
        }
        Err(ctx.bad_request(format!(
            "invalid {key} {v:?}: expected RFC 3339, YYYY-MM-DD, or one of 24h|7d|30d|all"
        )))
    }
}

/// Dotted paths whose values differ between two raw config trees, for the
/// `config.put` audit detail. `[[engines]]` entries pair by `id` so a tier
/// edit reports `engines.ddgs.tier`, not the whole array.
fn changed_config_paths(old: &toml::Value, new: &toml::Value) -> Vec<String> {
    fn walk(path: &str, a: &toml::Value, b: &toml::Value, out: &mut Vec<String>) {
        if path == "engines"
            && let (Some(ea), Some(eb)) = (a.as_array(), b.as_array())
        {
            let ids: std::collections::BTreeSet<String> = ea
                .iter()
                .chain(eb.iter())
                .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect();
            for id in ids {
                fn find_engine<'v>(arr: &'v [toml::Value], id: &str) -> Option<&'v toml::Value> {
                    arr.iter()
                        .find(|e| e.get("id").and_then(|v| v.as_str()) == Some(id))
                }
                match (find_engine(ea, &id), find_engine(eb, &id)) {
                    (Some(va), Some(vb)) => walk(&format!("engines.{id}"), va, vb, out),
                    (entry_a, entry_b) => {
                        if entry_a.is_some() != entry_b.is_some() {
                            out.push(format!("engines.{id}"));
                        }
                    }
                }
            }
            return;
        }
        match (a.as_table(), b.as_table()) {
            (Some(ta), Some(tb)) => {
                let keys: std::collections::BTreeSet<&String> =
                    ta.keys().chain(tb.keys()).collect();
                for k in keys {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    match (ta.get(k), tb.get(k)) {
                        (Some(va), Some(vb)) => walk(&p, va, vb, out),
                        _ => out.push(p),
                    }
                }
            }
            _ => {
                if a != b {
                    out.push(path.to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk("", old, new, &mut out);
    out
}
