//! `/settings` form page (W2-07): the config file rendered as a form —
//! the display tree mapped onto fields, `CAUCE_*` env pins, the cache
//! fieldset and the `PUT /api/config` status fragment.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use askama::Template;
use axum::Extension;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use cauce_core::config::{AiConfig, AiProtocol, EngineKind};
use cauce_core::http::HttpClient;
use cauce_core::{EngineError, EngineId};
use serde_json::Value;

use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

use super::{HTMX_JS, STYLE_CSS, prefers_json, render_err};

/// Budget for the `/models` listing behind the model picker; the settings
/// page must not hang on a dead provider.
const MODELS_BUDGET: Duration = Duration::from_millis(1500);

/// A settings field plus the `CAUCE_*` override pinning it, when set. A
/// pinned input renders `disabled` — a save must never bake the env value
/// into `config.toml`.
struct Field {
    value: String,
    /// The `CAUCE_*` var pinning this field, `""` when unset.
    env: String,
}

/// One `[[engines]]` row of the settings form.
struct EngineSettings {
    id: String,
    /// The row's `fe-*` error element id (`fe-engines-<id>` encoded,
    /// dot-free and injective — see [`crate::settings::fe_id`]).
    fe_id: String,
    kind: &'static str,
    enabled: bool,
    /// `""` for "no override", else `"1"`/`"2"`/`"3"`.
    tier: String,
    proxy: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    /// The shared header's active nav item.
    nav_active: &'static str,
    config_path: String,
    deadline: Field,
    ttl: Field,
    min_results: Field,
    hedge_floor: Field,
    hedge_ceiling: Field,
    engines: Vec<EngineSettings>,
    engines_pinned: bool,
    max_wait: Field,
    max_concurrent: Field,
    retention: Field,
    ai_base_url: Field,
    ai_api_key: Field,
    /// `"NAME: set"` / `"NAME: not set"` for a `${env:NAME}` api_key.
    ai_key_status: String,
    ai_model: Field,
    ai_enabled: bool,
    /// `"CAUCE_AI_ENABLED"` when the env var pins `ai.enabled`, else `""`.
    ai_enabled_env: String,
    ai_models: Vec<String>,
    ai_models_failed: bool,
    /// Pre-rendered `settings_cache.html` fieldset (the `?fragment=cache`
    /// target refreshes the same block out of band after a delete).
    cache_block: String,
    request_id: String,
    htmx_js: String,
    style_css: String,
}

/// The `Cache` fieldset alone, for `GET /settings?fragment=cache` (the
/// in-place refresh after `delete expired` / `delete all`).
#[derive(Template)]
#[template(path = "settings_cache.html")]
struct CacheBlock {
    /// `"N entries · N unexpired · <db size> · newest HH:MM"`.
    line: String,
}

/// The `PUT /api/config` status fragment swapped into `#settings-status`,
/// plus out-of-band `<p class="field-error">` lines — one under each
/// offending input (`clear_ids` empties the rest). Rendered at 200 even for
/// validation errors: htmx only swaps 2xx.
#[derive(Template)]
#[template(
    source = "<span class=\"form-status {{ kind }}\">{{ text }}</span>{% for id in clear_ids %}<p class=\"field-error\" id=\"{{ id }}\" hx-swap-oob=\"true\"></p>{% endfor %}{% for (id, msg) in field_errors %}<p class=\"field-error\" id=\"{{ id }}\" hx-swap-oob=\"true\">{{ msg }}</p>{% endfor %}",
    ext = "html"
)]
struct SettingsStatus {
    kind: &'static str,
    text: String,
    /// `fe-*` element ids whose error line should be emptied.
    clear_ids: Vec<String>,
    /// `(fe-* element id, message)` pairs rendered under their inputs.
    field_errors: Vec<(String, String)>,
}

/// `GET /settings`: the config file as a form (W2-07). Reads the redacted
/// display tree — `${...}` templates verbatim — except `ai.api_key`, which
/// comes from the raw file layer so a literal key renders as typed (a
/// `<redacted>` placeholder in the input would save back as the literal
/// string). Saves through `PUT /api/config` (urlencoded merge,
/// `crate::settings`), never a second write path.
pub async fn settings(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Query(query): Query<SettingsQuery>,
) -> Result<Response, ApiError> {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if prefers_json(accept) {
        return crate::handlers::config_get(State(state), Extension(ctx))
            .await
            .map(|j| j.into_response());
    }

    let rid = ctx.request_id.as_uuid();
    let cache_line = cache_line(&state).await;
    if query.fragment.as_deref() == Some("cache") {
        let block = CacheBlock { line: cache_line };
        return Ok(Html(block.render().map_err(|e| render_err(e, rid))?).into_response());
    }
    let (tree, raw_tree, ai, engines, config_path) = state.with_config(|c| {
        (
            c.display_tree(),
            c.raw_tree().cloned(),
            c.ai.clone(),
            c.engines.clone(),
            c.config_path().display().to_string(),
        )
    });
    let tree = tree.map_err(|e| {
        ApiError::internal(format!("config display failed: {e}")).with_request_id(Some(rid))
    })?;

    let field = |path: &str| Field {
        value: tree_display(&tree, path),
        env: env_override(path).unwrap_or_default(),
    };
    // The file layer for the one secret the form renders: a literal key
    // shows as typed and a `${env:...}` template verbatim. File-less
    // configs (`Config::default()` in tests) fall back to the display tree.
    let file_tree = raw_tree.as_ref().unwrap_or(&tree);
    let ai_key_status = env_status(&tree_display(file_tree, "ai.api_key")).unwrap_or_default();
    let engines_pinned = std::env::var("CAUCE_ENGINES")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let engine_rows = engines
        .iter()
        .map(|e| {
            let id = e.id.to_string();
            EngineSettings {
                fe_id: crate::settings::fe_id(&format!("engines.{id}")),
                id,
                kind: kind_label(e.kind),
                enabled: e.enabled,
                tier: e.tier.map(|t| t.as_u8().to_string()).unwrap_or_default(),
                proxy: e
                    .egress
                    .as_ref()
                    .and_then(|g| g.proxy.clone())
                    .unwrap_or_default(),
            }
        })
        .collect();

    // The resolved key authenticates the listing; it is never rendered.
    let (ai_models, ai_models_failed) = list_models(&ai).await;

    let cache_block = CacheBlock { line: cache_line }
        .render()
        .map_err(|e| render_err(e, rid))?;
    let page = SettingsPage {
        nav_active: "settings",
        config_path,
        deadline: field("search.deadline_ms"),
        ttl: field("search.ttl_s"),
        min_results: field("search.min_results"),
        hedge_floor: field("search.hedge_floor_ms"),
        hedge_ceiling: field("search.hedge_ceiling_ms"),
        engines: engine_rows,
        engines_pinned,
        max_wait: field("admission.max_wait_ms"),
        max_concurrent: field("admission.max_concurrent_per_engine"),
        retention: field("logs.retention_days"),
        ai_base_url: field("ai.base_url"),
        ai_api_key: Field {
            value: tree_display(file_tree, "ai.api_key"),
            env: env_override("ai.api_key").unwrap_or_default(),
        },
        ai_key_status,
        ai_model: field("ai.model"),
        ai_enabled: ai.enabled,
        ai_enabled_env: env_override("ai.enabled").unwrap_or_default(),
        ai_models,
        ai_models_failed,
        cache_block,
        request_id: rid.to_string(),
        htmx_js: HTMX_JS.clone(),
        style_css: STYLE_CSS.clone(),
    };
    Ok(Html(page.render().map_err(|e| render_err(e, rid))?).into_response())
}

/// `GET /settings` query params: only the HTMX fragment mode today.
#[derive(serde::Deserialize)]
pub struct SettingsQuery {
    fragment: Option<String>,
}

/// `"{total} entries · {unexpired} unexpired · {db size} · newest {HH:MM}"`
/// — the same stats the dashboard reads (`Store::stats` + the db file size
/// + the newest `cache_entries` row).
async fn cache_line(state: &AppState) -> String {
    use crate::strings::settings as s;
    let (total, unexpired) = match state.store().stats(7).await {
        Ok(st) => (
            st.cache_entries + st.cache_entries_expired,
            st.cache_entries,
        ),
        Err(_) => (0, 0),
    };
    let db_path = state.with_config(|c| c.db_path());
    let size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    let newest = state
        .store()
        .list_cache(1, 0)
        .await
        .ok()
        .and_then(|rows| rows.into_iter().next())
        .map(|e| {
            e.created_at
                .with_timezone(&chrono::Local)
                .format("%H:%M")
                .to_string()
        })
        .unwrap_or_else(|| crate::strings::common::DASH.to_string());
    format!(
        "{total} {entries} · {unexpired} {unexp} · {size} · {newest_label} {newest}",
        entries = s::ENTRIES,
        unexp = s::UNEXPIRED,
        size = human_bytes(size),
        newest_label = s::NEWEST,
    )
}

/// `1234` -> `"1.2 KB"`-ish display for the db file size.
fn human_bytes(n: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    if n as f64 >= MIB {
        format!("{:.1} MB", n as f64 / MIB)
    } else if n >= 1024 {
        format!("{:.0} KB", n as f64 / KIB)
    } else {
        format!("{n} B")
    }
}

/// The `PUT /api/config` response for an HTMX form submit (see
/// `handlers::config_put`): the status line plus an out-of-band error line
/// under each field `field_errors` names (`saved HH:MM`,
/// `not saved: N errors`, `error: could not save (<status>)`).
pub(crate) fn settings_status(
    result: Result<Json<Value>, ApiError>,
    field_errors: Vec<(String, String)>,
    submitted: Vec<String>,
    engine_ids: &[String],
) -> Response {
    use crate::strings::settings as s;
    let (kind, text) = match &result {
        Ok(_) => (
            "ok",
            format!("{} {}", s::SAVED, chrono::Local::now().format("%H:%M")),
        ),
        Err(e) => {
            let n = field_errors.len();
            if n > 0 {
                let word = if n == 1 { s::ERROR_ONE } else { s::ERROR_MANY };
                ("error", format!("{} {n} {word}", s::NOT_SAVED))
            } else {
                (
                    "error",
                    format!("{} ({})", s::COULD_NOT_SAVE, e.status().as_u16()),
                )
            }
        }
    };
    // `engines.<id>.<field>` names land on the row's `fe-engines-<id>`
    // element (there is no per-input error element); errors hitting the
    // same row merge into one line. `fe_id` keeps the emitted ids dot-free
    // and injective so htmx's oob `querySelector` lookup can find them and
    // no two rows share an id.
    let mut merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (name, msg) in field_errors {
        let target = crate::settings::fe_id(&crate::settings::error_target(&name, engine_ids));
        merged
            .entry(target)
            .and_modify(|m| {
                m.push_str("; ");
                m.push_str(&msg);
            })
            .or_insert(msg);
    }
    let field_errors: Vec<(String, String)> = merged.into_iter().collect();
    let errored: std::collections::BTreeSet<&String> =
        field_errors.iter().map(|(id, _)| id).collect();
    // Clears only name elements the page renders — the same row-level
    // mapping applies, so `engines.<id>.tier` clears `fe-engines-<id>`,
    // never a `fe-engines-<id>-tier` phantom.
    let clear_ids: Vec<String> = submitted
        .into_iter()
        .map(|name| crate::settings::fe_id(&crate::settings::error_target(&name, engine_ids)))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|id| !errored.contains(id))
        .collect();
    let html = SettingsStatus {
        kind,
        text: text.clone(),
        clear_ids,
        field_errors,
    }
    .render()
    .unwrap_or(text);
    Html(html).into_response()
}

/// The display-tree value at `path` as a string (integers, floats and bools
/// stringify); missing leaves are `""`.
fn tree_display(tree: &toml::Value, path: &str) -> String {
    let mut cur = tree;
    for seg in path.split('.') {
        cur = match cur {
            toml::Value::Table(t) => match t.get(seg) {
                Some(v) => v,
                None => return String::new(),
            },
            toml::Value::Array(a) => match seg.parse::<usize>().ok().and_then(|i| a.get(i)) {
                Some(v) => v,
                None => return String::new(),
            },
            _ => return String::new(),
        };
    }
    match cur {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        _ => String::new(),
    }
}

/// The `CAUCE_*` var pinning `path`, when set. Mirrors `ENV_OVERRIDES` in
/// `cauce-core::config` for the fields the form renders — keep in sync.
fn env_override(path: &str) -> Option<String> {
    let name = match path {
        "search.deadline_ms" => "CAUCE_SEARCH_DEADLINE_MS",
        "search.ttl_s" => "CAUCE_SEARCH_TTL_S",
        "search.min_results" => "CAUCE_SEARCH_MIN_RESULTS",
        "search.hedge_floor_ms" => "CAUCE_SEARCH_HEDGE_FLOOR_MS",
        "search.hedge_ceiling_ms" => "CAUCE_SEARCH_HEDGE_CEILING_MS",
        "admission.max_wait_ms" => "CAUCE_ADMISSION_MAX_WAIT_MS",
        "admission.max_concurrent_per_engine" => "CAUCE_ADMISSION_MAX_CONCURRENT_PER_ENGINE",
        "logs.retention_days" => "CAUCE_LOGS_RETENTION_DAYS",
        "ai.base_url" => "CAUCE_AI_BASE_URL",
        "ai.api_key" => "CAUCE_AI_API_KEY",
        "ai.model" => "CAUCE_AI_MODEL",
        "ai.enabled" => "CAUCE_AI_ENABLED",
        _ => return None,
    };
    std::env::var_os(name).map(|_| name.to_string())
}

/// `${env:NAME}`/`${env:NAME:...}` in a raw (unresolved) value ->
/// `"NAME is set"` / `"NAME is not set"`. The `:`-suffixed forms count an
/// empty variable as missing (POSIX), the plain form counts it as set.
fn env_status(raw: &str) -> Option<String> {
    let inner = raw.strip_prefix("${env:")?.strip_suffix('}')?;
    let (name, colon_form) = match inner.find(':') {
        Some(i) => (&inner[..i], true),
        None => (inner, false),
    };
    if name.is_empty() {
        return None;
    }
    let set = match std::env::var(name) {
        Ok(v) => !colon_form || !v.is_empty(),
        Err(_) => false,
    };
    use crate::strings::settings as s;
    Some(format!(
        "{name} {}",
        if set { s::IS_SET } else { s::IS_NOT_SET }
    ))
}

fn kind_label(kind: EngineKind) -> &'static str {
    match kind {
        EngineKind::Declarative => "declarative",
        EngineKind::Exec => "exec",
        EngineKind::Replay => "replay",
    }
}

/// `GET {base_url}/models` (OpenAI) or `{base_url}/v1/models`
/// (Anthropic) for the model picker: `(ids, failed)`. An empty
/// `base_url` is `([], false)`; any fetch/parse failure is `([], true)`
/// and the input falls back to free text.
async fn list_models(ai: &AiConfig) -> (Vec<String>, bool) {
    let base = ai.base_url.trim();
    if base.is_empty() {
        return (Vec::new(), false);
    }
    // Same auth + path the provider client uses (W4-05).
    let (url, headers) = match ai.protocol {
        AiProtocol::OpenAi => (
            format!("{}/models", base.trim_end_matches('/')),
            models_headers("Bearer", &ai.api_key),
        ),
        AiProtocol::Anthropic => (
            format!("{}/v1/models", base.trim_end_matches('/')),
            models_headers("x-api-key", &ai.api_key),
        ),
    };
    match fetch_models(&url, headers).await {
        Ok(ids) => (ids, false),
        Err(e) => {
            tracing::debug!(error = %e, "settings: {base} models listing failed");
            (Vec::new(), true)
        }
    }
}

/// Build the auth headers for the models listing. `scheme` is
/// `"Bearer"` (Authorization) or `"x-api-key"` (Anthropic, which also
/// pins `anthropic-version`).
fn models_headers(scheme: &str, api_key: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    if scheme == "x-api-key" {
        headers.insert(
            "anthropic-version",
            reqwest::header::HeaderValue::from_static("2023-06-01"),
        );
    }
    if !api_key.is_empty() {
        let (name, value) = match scheme {
            "x-api-key" => ("x-api-key", api_key.to_string()),
            _ => ("authorization", format!("Bearer {api_key}")),
        };
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&value) {
            headers.insert(name, value);
        }
    }
    headers
}

async fn fetch_models(
    url: &str,
    headers: reqwest::header::HeaderMap,
) -> Result<Vec<String>, EngineError> {
    let client = HttpClient::from_egress_config(EngineId::new("ai"), None)?;
    let res = client.get_with_headers(url, MODELS_BUDGET, headers).await?;
    if res.status != 200 {
        return Err(EngineError::Transport(format!(
            "/models answered HTTP {}",
            res.status
        )));
    }
    let body: Value =
        serde_json::from_slice(&res.body).map_err(|e| EngineError::Parse(e.to_string()))?;
    Ok(body["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}
