//! `/api/engines` handlers: the shared engine list data plane, the audited
//! breaker reset, and the enable/disable config write.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use cauce_core::{
    EngineHealthRow, EngineId, HistoryFilter, HistoryItem,
    config::{Config, system_env},
};
use chrono::Utc;
use serde_json::json;

use super::{MAX_LIMIT, write_audit};
use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

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
    /// The live health tracker knows this engine (registered or a
    /// persisted row), so `POST .../reset` will not 404. Card-only: the
    /// JSON wire predates it and stays unchanged.
    #[serde(skip_serializing)]
    #[cfg_attr(not(feature = "ui"), allow(dead_code))]
    pub tracked: bool,
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
            cached: false,
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
    // Persisted `engine_health` rows rebuild their `EngineId` unvalidated
    // (the row decode path predates the parse-time charset check), so a
    // pre-validation garbage id — space, colon — would otherwise mint a
    // card that no config could ever produce. Configured and live ids
    // already passed validation; health keys get the check here.
    ids.extend(health.keys().filter(|id| EngineId::is_valid(id)).cloned());

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
                tracked: health.contains_key(&id),
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
    // Persisted health rows can carry ids the charset rejects (they
    // predate validation); resetting one would re-persist and audit a
    // phantom engine. Reject like the enable/disable pair's unknown-id 404.
    if !EngineId::is_valid(&id) {
        return Err(ctx.not_found(format!("no such engine {id}")));
    }
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
    // File entries match by POSITION against `cfg.engines`, not by the
    // raw `id` leaf: a `${env:...}`/`${file:...}` id template never equals
    // the resolved id, and a miss here would fall through to the
    // serialize path below — persisting the RESOLVED entry (secrets and
    // all) into `config.toml`. `cfg.engines` holds the file's entries in
    // file order followed by appended built-ins, so `cfg.engines[i]`
    // resolves `arr[i]`. The literal-id compare stays as a fallback for
    // trees that diverge from the resolved config (tests, hand-built).
    for (index, entry) in arr.iter_mut().enumerate() {
        let resolved_match = cfg.engines.get(index).is_some_and(|e| e.id == *id);
        let literal_match = entry.get("id").and_then(toml::Value::as_str) == Some(id.as_str());
        if resolved_match || literal_match {
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
    // file-defined engine never reaches this branch because the
    // index-aligned lookup above already matched it.
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
