//! HTMX search page (`/`) and HTML/HTMX results (`/search`).
//!
//! Assets (HTMX 2.x, the JSON encoding extension and a small CSS file) are
//! embedded at compile time via `rust-embed`; templates are rendered with
//! Askama. No external CDN is used.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::borrow::Cow;
use std::sync::LazyLock;
use std::time::Duration;

use askama::Template;
use axum::Extension;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use cauce_core::config::{AiConfig, EngineKind};
use cauce_core::http::HttpClient;
use cauce_core::{
    CacheKey, ClickRow, EngineError, EngineId, EngineStatus, HistoryItem, SearchRequest,
    SearchResponse, Source,
};
use rust_embed::Embed;
use serde_json::{Value, json};

use crate::app::{AppState, RouterOptions};
use crate::error::ApiError;
use crate::handlers::{QueryParams, search_inner};
use crate::middleware::RequestCtx;
use crate::strings::{common, history as hs};

/// Static assets vendored under `crates/cauce-server/assets`.
#[derive(Embed)]
#[folder = "assets/"]
struct Assets;

fn asset_string(name: &str) -> String {
    Assets::get(name)
        .map(|f| String::from_utf8_lossy(&f.data).into_owned())
        .unwrap_or_default()
}

static HTMX_JS: LazyLock<String> = LazyLock::new(|| asset_string("htmx.min.js"));
static JSON_ENC_JS: LazyLock<String> = LazyLock::new(|| asset_string("json-enc.js"));
pub(crate) static STYLE_CSS: LazyLock<String> = LazyLock::new(|| asset_string("style.css"));
static FAVICON_SVG: LazyLock<Cow<'static, [u8]>> = LazyLock::new(|| {
    Assets::get("favicon.svg")
        .map(|f| f.data)
        .unwrap_or_default()
});

/// One rendered result row (plain strings so Askama only needs `Display`).
#[derive(Debug)]
struct Row {
    favicon: String,
    url: String,
    title: String,
    host: String,
    snippet: String,
    hx_vals: String,
}

/// Full page shell, rendered for `GET /` and for non-HTMX `GET /search`.
#[derive(Template)]
#[template(path = "page.html")]
struct Page {
    q: String,
    has_results: bool,
    result_count: usize,
    badge: String,
    request_id: String,
    short_request_id: String,
    results: Vec<Row>,
    more_url: String,
    htmx_js: String,
    json_enc_js: String,
    style_css: String,
}

/// Results partial swapped in by HTMX `hx-get` on the more button.
#[derive(Template)]
#[template(path = "results.html")]
struct Results {
    results: Vec<Row>,
    more_url: String,
}

/// `GET /` landing page with the search form.
pub async fn index(
    State(_state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> Result<Html<String>, ApiError> {
    let rid = ctx.request_id.as_uuid().to_string();
    let page = Page {
        q: String::new(),
        has_results: false,
        result_count: 0,
        badge: String::new(),
        request_id: rid.clone(),
        short_request_id: short_id(&rid),
        results: Vec::new(),
        more_url: String::new(),
        htmx_js: HTMX_JS.clone(),
        json_enc_js: JSON_ENC_JS.clone(),
        style_css: STYLE_CSS.clone(),
    };
    render_html(page, ctx.request_id.as_uuid())
}

/// `GET /search?q=...` with content negotiation and HTMX partial support.
pub async fn search(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if prefers_json(accept) {
        return crate::handlers::search(State(state), Extension(ctx), uri)
            .await
            .map(|j| j.into_response());
    }

    let (req, resp) = search_inner(&state, &ctx, &uri).await?;
    let params = QueryParams::parse(uri.query(), &ctx)?;
    let is_hx = headers.get("hx-request").is_some();

    let rid = resp.meta.request_id.to_string();
    let rows = result_rows(&req, &resp);
    let more_url = more_url(&resp, &params, &req);
    let q = params.required(&ctx, "q")?.to_string();

    if is_hx {
        let partial = Results {
            results: rows,
            more_url,
        };
        Ok(Html(
            partial
                .render()
                .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
        )
        .into_response())
    } else {
        let page = Page {
            q,
            has_results: true,
            result_count: resp.results.len(),
            badge: badge(&resp),
            request_id: rid.clone(),
            short_request_id: short_id(&rid),
            results: rows,
            more_url,
            htmx_js: HTMX_JS.clone(),
            json_enc_js: JSON_ENC_JS.clone(),
            style_css: STYLE_CSS.clone(),
        };
        Ok(Html(
            page.render()
                .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
        )
        .into_response())
    }
}

/// `GET /favicon.ico`: the embedded SVG site icon. Browsers request this
/// path on every page load; wave-0 verification saw it 404 each time (#87).
pub async fn favicon() -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        FAVICON_SVG.clone(),
    )
        .into_response()
}

/// `GET /opensearch.xml` (W2-11): the OpenSearch 1.1 description document
/// browsers fetch after seeing the page head's `<link rel="search">`.
///
/// The absolute URL templates use the configured canonical public origin,
/// or the effective bind host and port when no public origin is configured.
/// Request `Host` and forwarded headers are never used as URL input.
pub async fn opensearch(
    State(state): State<AppState>,
    Extension(options): Extension<RouterOptions>,
) -> Response {
    let origin = state.with_config(|cfg| {
        cfg.server
            .public_origin(&options.bind_host, options.bind_port)
    });
    (
        [(
            header::CONTENT_TYPE,
            "application/opensearchdescription+xml",
        )],
        opensearch_xml(&origin),
    )
        .into_response()
}

fn opensearch_xml(origin: &str) -> String {
    let results_url = xml_attribute_escape(&format!("{origin}/search?q={{searchTerms}}"));
    let suggestions_url = xml_attribute_escape(&format!("{origin}/api/suggest?q={{searchTerms}}"));
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<OpenSearchDescription xmlns=\"http://a9.com/-/spec/opensearch/1.1/\">\n",
            "  <ShortName>cauce</ShortName>\n",
            "  <Description>cauce metasearch</Description>\n",
            "  <InputEncoding>UTF-8</InputEncoding>\n",
            "  <Url type=\"text/html\" rel=\"results\" \
             template=\"{results_url}\"/>\n",
            "  <Url type=\"application/x-suggestions+json\" rel=\"suggestions\" \
             template=\"{suggestions_url}\"/>\n",
            "</OpenSearchDescription>\n",
        ),
        results_url = results_url,
        suggestions_url = suggestions_url
    )
}

fn xml_attribute_escape(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

pub(crate) fn prefers_json(accept: &str) -> bool {
    accept.contains("application/json") && !accept.contains("text/html")
}

fn badge(resp: &SearchResponse) -> String {
    match &resp.meta.source {
        Source::Cache { age_s, ttl_s, .. } => format!("cached · {age_s} s ago · ttl {ttl_s} s"),
        Source::Network => {
            let engines = resp
                .meta
                .engines_used
                .iter()
                .filter(|r| matches!(r.status, EngineStatus::Ok))
                .map(|r| r.engine.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("live · {} ms · {engines}", resp.meta.elapsed_ms)
        }
    }
}

fn result_rows(req: &SearchRequest, resp: &SearchResponse) -> Vec<Row> {
    let query_hash = CacheKey::from(req);

    resp.results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let host = r.url.host_str().unwrap_or("").to_string();
            let url = r.url.as_str().to_string();
            let hx_vals = serde_json::to_string(&json!({
                "url": url,
                "query_hash": query_hash.as_str(),
                "position": i,
            }))
            .unwrap_or_default();
            let favicon = if host.is_empty() {
                String::new()
            } else {
                format!("https://icons.duckduckgo.com/ip3/{host}.ico")
            };
            Row {
                favicon,
                url,
                title: r.title.clone(),
                host,
                snippet: r.snippet.clone(),
                hx_vals,
            }
        })
        .collect::<Vec<_>>()
}

fn more_url(resp: &SearchResponse, params: &QueryParams, req: &SearchRequest) -> String {
    if resp.results.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    parts.push(format!("q={}", urlencoding::encode(&req.q)));
    parts.push(format!("page={}", req.page.saturating_add(1)));
    if let Some(lang) = params.get("lang") {
        parts.push(format!("lang={}", urlencoding::encode(lang)));
    }
    if let Some(time_range) = params.get("time_range") {
        parts.push(format!("time_range={}", urlencoding::encode(time_range)));
    }
    if let Some(safesearch) = params.get("safesearch") {
        parts.push(format!("safesearch={}", urlencoding::encode(safesearch)));
    }
    if let Some(engines) = params.get("engines") {
        parts.push(format!("engines={}", urlencoding::encode(engines)));
    }
    format!("/search?{}", parts.join("&"))
}

pub(crate) fn short_id(request_id: &str) -> String {
    request_id.chars().take(8).collect()
}

// ---------------------------------------------------------------------------
// /settings (W2-07)
// ---------------------------------------------------------------------------

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
    kind: &'static str,
    enabled: bool,
    /// `""` for "no override", else `"1"`/`"2"`/`"3"`.
    tier: String,
    proxy: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    config_path: String,
    deadline: Field,
    ttl: Field,
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
    source = "<span class=\"form-status {{ kind }}\">{{ text }}</span>{% for id in clear_ids %}<p class=\"field-error\" id=\"fe-{{ id }}\" hx-swap-oob=\"true\"></p>{% endfor %}{% for (id, msg) in field_errors %}<p class=\"field-error\" id=\"fe-{{ id }}\" hx-swap-oob=\"true\">{{ msg }}</p>{% endfor %}",
    ext = "html"
)]
struct SettingsStatus {
    kind: &'static str,
    text: String,
    /// Field names whose error line should be emptied.
    clear_ids: Vec<String>,
    /// `(field name, message)` pairs rendered under their inputs.
    field_errors: Vec<(String, String)>,
}

/// `GET /settings`: the config file as a form (W2-07). Reads the redacted
/// display tree — `${...}` templates verbatim, secrets as `<redacted>` —
/// and saves through `PUT /api/config` (urlencoded merge, `crate::settings`),
/// never a second write path.
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
    let (tree, ai, engines, config_path) = state.with_config(|c| {
        (
            c.display_tree(),
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
    let ai_key_status = env_status(&tree_display(&tree, "ai.api_key")).unwrap_or_default();
    let engines_pinned = std::env::var("CAUCE_ENGINES")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let engine_rows = engines
        .iter()
        .map(|e| EngineSettings {
            id: e.id.to_string(),
            kind: kind_label(e.kind),
            enabled: e.enabled,
            tier: e.tier.map(|t| t.as_u8().to_string()).unwrap_or_default(),
            proxy: e
                .egress
                .as_ref()
                .and_then(|g| g.proxy.clone())
                .unwrap_or_default(),
        })
        .collect();

    // The resolved key authenticates the listing; it is never rendered.
    let (ai_models, ai_models_failed) = list_models(&ai).await;

    let cache_block = CacheBlock { line: cache_line }
        .render()
        .map_err(|e| render_err(e, rid))?;
    let page = SettingsPage {
        config_path,
        deadline: field("search.deadline_ms"),
        ttl: field("search.ttl_s"),
        engines: engine_rows,
        engines_pinned,
        max_wait: field("admission.max_wait_ms"),
        max_concurrent: field("admission.max_concurrent_per_engine"),
        retention: field("logs.retention_days"),
        ai_base_url: field("ai.base_url"),
        ai_api_key: field("ai.api_key"),
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
    // `engines.<id>.<field>` names land on the row's `fe-engines.<id>`
    // element (there is no per-input error element); errors hitting the
    // same row merge into one line.
    let mut merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (name, msg) in field_errors {
        let target = crate::settings::error_target(&name, engine_ids);
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
    // mapping applies, so `engines.<id>.tier` clears `fe-engines.<id>`,
    // never a `fe-engines.<id>.tier` phantom.
    let clear_ids: Vec<String> = submitted
        .into_iter()
        .map(|name| crate::settings::error_target(&name, engine_ids))
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

/// `GET {base_url}/models` for the model picker: `(ids, failed)`. An empty
/// `base_url` is `([], false)`; any fetch/parse failure is `([], true)` and
/// the input falls back to free text. W4-01 replaces this with the real
/// provider client (60 s cache).
async fn list_models(ai: &AiConfig) -> (Vec<String>, bool) {
    let base = ai.base_url.trim();
    if base.is_empty() {
        return (Vec::new(), false);
    }
    match fetch_models(
        &format!("{}/models", base.trim_end_matches('/')),
        &ai.api_key,
    )
    .await
    {
        Ok(ids) => (ids, false),
        Err(e) => {
            tracing::debug!(error = %e, "settings: {base}/models listing failed");
            (Vec::new(), true)
        }
    }
}

async fn fetch_models(url: &str, api_key: &str) -> Result<Vec<String>, EngineError> {
    let client = HttpClient::from_egress_config(EngineId::new("ai"), None)?;
    let mut headers = reqwest::header::HeaderMap::new();
    if !api_key.is_empty() {
        let value = reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|e| EngineError::Transport(format!("invalid ai.api_key: {e}")))?;
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }
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

// ---------------------------------------------------------------------------
// `/history` (W2-02)
// ---------------------------------------------------------------------------

/// `Accept: text/html` (without an explicit JSON ask) wants the page — the
/// content-negotiation half of "the HTML page is the API handler".
pub(crate) fn prefers_html(headers: &HeaderMap) -> bool {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    accept.contains("text/html") && !accept.contains("application/json")
}

/// One clicked link nested under a search row (or the single line of a
/// `(click only)` row): domain, title link, result position.
#[derive(Debug)]
struct ClickLine {
    domain: String,
    url: String,
    title: String,
    /// 1-based `#n` position in the result list that was clicked.
    position: String,
}

/// One table row on the history page: a logged search, or a click whose
/// `query_hash` matches no shown search (rendered standalone so the feed
/// never hides recorded clicks).
#[derive(Debug)]
struct HistRow {
    is_search: bool,
    /// `search_log` id (drives the delete button); unused on click rows.
    id: i64,
    /// `YYYY-MM-DD` group header, set on the first row of each local day.
    day_header: Option<String>,
    /// `HH:MM` local time.
    when: String,
    /// The search query (click rows render `hs::CLICK_ONLY` instead).
    query: String,
    /// `cached · <age>` | `cached · expired` | `network · t<N>`;
    /// `-` on click rows.
    source: String,
    /// `/cache?q=<query>#<key>` when the source is a live cache entry.
    source_url: String,
    /// Live cache entry → the `payload` action is shown.
    cached_live: bool,
    engines: String,
    result_count: String,
    latency: String,
    client: String,
    /// Nested click lines under this row.
    clicks: Vec<ClickLine>,
    /// `hx-confirm` on the delete button; names the nested clicks so a
    /// destructive action never hides its blast radius.
    delete_confirm: String,
    rerun_url: String,
    json_url: String,
}

/// `/history` full page.
#[derive(Template)]
#[template(path = "history.html")]
struct History {
    /// Selected `since` window (`24h` | `7d` | `30d` | `all`).
    since: String,
    /// Active `q` substring filter.
    q: String,
    /// `cached=1` filter.
    cached: bool,
    /// Any filter active → the `clear` link is shown.
    filters_active: bool,
    /// `N searches in last 24h · N total · N clicks today`.
    stats_line: String,
    /// Empty-state sentence (fresh store or filtered-empty wording).
    empty_message: String,
    rows: Vec<HistRow>,
    /// `showing 200 of N · use the filters to reach older searches`;
    /// empty unless the feed was truncated.
    capped_line: String,
    request_id: String,
    htmx_js: String,
    style_css: String,
}

/// `GET /history` (W2-02): identical to `GET /api/history` — the shared
/// handler negotiates on `Accept`.
pub async fn history(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    crate::handlers::history(State(state), Extension(ctx), uri, headers).await
}

/// The `Accept: text/html` arm of the shared history handler: same params,
/// same defaults, same row set as the JSON route, plus the render-time
/// reads the page needs (header stats, batched cache states).
pub(crate) async fn history_page(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Response, ApiError> {
    let (params, filter, items) =
        crate::handlers::history_inner(&state, &ctx, &uri, crate::handlers::HISTORY_LIMIT).await?;
    let stats = state
        .store()
        .history_stats(&filter)
        .await
        .map_err(|e| ctx.store(&e))?;

    // One batched lookup for the page's distinct query_hashes — the
    // `source` column is computed at render time from `cache_entries`.
    let keys: Vec<CacheKey> = {
        let mut seen = std::collections::HashSet::new();
        items
            .iter()
            .filter_map(|i| match i {
                HistoryItem::Search(row) if seen.insert(row.query_hash.as_str()) => {
                    Some(row.query_hash.clone())
                }
                _ => None,
            })
            .collect()
    };
    let cache: std::collections::HashMap<String, cauce_core::CacheState> = state
        .store()
        .cache_states(&keys)
        .await
        .map_err(|e| ctx.store(&e))?
        .into_iter()
        .map(|st| (st.key.as_str().to_string(), st))
        .collect();

    // Clicks whose search row fell outside the rendered window (the feed
    // cap counts searches AND clicks) must not render as `(click only)` —
    // only a click with no `search_log` row at all is an orphan.
    let search_hashes_in_feed: std::collections::HashSet<String> = items
        .iter()
        .filter_map(|i| match i {
            HistoryItem::Search(row) => Some(row.query_hash.as_str().to_string()),
            _ => None,
        })
        .collect();
    let orphan_candidates: Vec<CacheKey> = {
        let mut seen = std::collections::HashSet::new();
        items
            .iter()
            .filter_map(|i| match i {
                HistoryItem::Click(c) => c.query_hash.as_ref().filter(|h| {
                    !search_hashes_in_feed.contains(h.as_str()) && seen.insert(h.as_str())
                }),
                _ => None,
            })
            .cloned()
            .collect()
    };
    let off_window: std::collections::HashSet<String> = state
        .store()
        .search_hashes(&orphan_candidates)
        .await
        .map_err(|e| ctx.store(&e))?
        .into_iter()
        .map(|k| k.as_str().to_string())
        .collect();

    let rows = history_rows(items, &cache, &off_window);
    let filters_active = filter.cached || filter.q.is_some() || filter.since.is_some();
    let empty_message = if rows.is_empty() {
        empty_message(&params, &filter)
    } else {
        String::new()
    };
    let capped_line = if stats.matching as usize > rows.len() && !rows.is_empty() {
        format!(
            "{} {} {} {} · {}",
            hs::CAPPED_SHOWING,
            rows.len(),
            hs::CAPPED_OF,
            stats.matching,
            hs::CAPPED_HINT
        )
    } else {
        String::new()
    };
    let stats_line = format!(
        "{} {} · {} {} · {} {}",
        stats.searches_24h,
        hs::STAT_SEARCHES_24H,
        stats.searches_total,
        hs::STAT_TOTAL,
        stats.clicks_today,
        hs::STAT_CLICKS_TODAY,
    );

    let page = History {
        since: params.get("since").unwrap_or("all").to_string(),
        q: params.get("q").unwrap_or("").to_string(),
        cached: filter.cached,
        filters_active,
        stats_line,
        empty_message,
        rows,
        capped_line,
        request_id: ctx.request_id.as_uuid().to_string(),
        htmx_js: HTMX_JS.clone(),
        style_css: STYLE_CSS.clone(),
    };
    Ok(Html(
        page.render()
            .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
    )
    .into_response())
}

/// The one-sentence empty state: plain `empty` when nothing is stored, or
/// the filtered-empty sentence naming the active filters.
fn empty_message(params: &QueryParams, filter: &cauce_core::HistoryFilter) -> String {
    if !filter.cached && filter.q.is_none() && filter.since.is_none() {
        return hs::EMPTY.to_string();
    }
    let mut msg = match params.get("q").filter(|v| !v.trim().is_empty()) {
        Some(q) => format!("{} \"{q}\"", hs::EF_MATCH),
        None => hs::EF_NONE.to_string(),
    };
    match params.get("since") {
        Some("24h") => msg.push_str(&format!(" {}", hs::IN_24H)),
        Some("7d") => msg.push_str(&format!(" {}", hs::IN_7D)),
        Some("30d") => msg.push_str(&format!(" {}", hs::IN_30D)),
        // `all` adds no time clause; an absolute timestamp is spelled out.
        Some(v) if v != "all" => msg.push_str(&format!(" {} {v}", hs::IN_SINCE)),
        _ => {}
    }
    if filter.cached {
        msg.push_str(&format!(" {}", hs::EF_CACHED));
    }
    msg.push('.');
    msg
}

/// Compact entry age for `cached · <age>`: `42s`, `41m`, `3h`, `2d`.
fn cache_age(from: chrono::DateTime<chrono::Utc>, now: chrono::DateTime<chrono::Utc>) -> String {
    let secs = (now - from).num_seconds().max(0) as u64;
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3_600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3_600),
        s => format!("{}d", s / 86_400),
    }
}

/// Fold the merged `list_history` feed into display rows: clicks join the
/// newest search row sharing their `query_hash`; clicks without one render
/// as standalone `(click only)` rows in feed position — except hashes in
/// `off_window`, which have a `search_log` row outside the rendered window
/// and are dropped rather than mislabeled. `cache` carries the render-time
/// `cache_entries` state for the page's distinct query_hashes.
fn history_rows(
    items: Vec<HistoryItem>,
    cache: &std::collections::HashMap<String, cauce_core::CacheState>,
    off_window: &std::collections::HashSet<String>,
) -> Vec<HistRow> {
    use std::collections::{HashMap, HashSet};

    let now = chrono::Utc::now();
    let search_hashes: HashSet<String> = items
        .iter()
        .filter_map(|i| match i {
            HistoryItem::Search(s) => Some(s.query_hash.as_str().to_string()),
            _ => None,
        })
        .collect();
    let mut clicks_by_hash: HashMap<String, Vec<ClickLine>> = HashMap::new();
    for item in &items {
        if let HistoryItem::Click(c) = item
            && let Some(hash) = &c.query_hash
            && search_hashes.contains(hash.as_str())
        {
            clicks_by_hash
                .entry(hash.as_str().to_string())
                .or_default()
                .push(click_line(c));
        }
    }

    let mut attached: HashSet<String> = HashSet::new();
    let mut last_day = String::new();
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        match item {
            HistoryItem::Search(s_row) => {
                let query = s_row
                    .query_raw
                    .clone()
                    .unwrap_or_else(|| s_row.query.clone());
                let encoded = urlencoding::encode(&query).into_owned();
                // First occurrence wins: items are newest-first, so clicks
                // attach to the newest search row with their query_hash.
                let clicks = if attached.insert(s_row.query_hash.as_str().to_string()) {
                    clicks_by_hash
                        .remove(s_row.query_hash.as_str())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                let (source, source_url, cached_live) = match cache.get(s_row.query_hash.as_str()) {
                    Some(st) if st.expires_at > now => {
                        let url = format!(
                            "/cache?q={}#{}",
                            urlencoding::encode(&st.query),
                            st.key.as_str()
                        );
                        (
                            format!("{} · {}", hs::SRC_CACHED, cache_age(st.created_at, now)),
                            url,
                            true,
                        )
                    }
                    Some(_) => (
                        format!("{} · {}", hs::SRC_CACHED, hs::SRC_EXPIRED),
                        String::new(),
                        false,
                    ),
                    None => (
                        match s_row.tier {
                            Some(t) => {
                                format!("{} · {}{}", hs::SRC_NETWORK, hs::TIER_PREFIX, t.as_u8())
                            }
                            None => hs::SRC_NETWORK.to_string(),
                        },
                        String::new(),
                        false,
                    ),
                };
                let delete_confirm = if clicks.is_empty() {
                    hs::DELETE_CONFIRM.to_string()
                } else {
                    let word = if clicks.len() == 1 {
                        hs::CLICK_ONE
                    } else {
                        hs::CLICKS_WORD
                    };
                    format!(
                        "{} {} {}?",
                        hs::DELETE_CONFIRM_CLICKS_PRE,
                        clicks.len(),
                        word
                    )
                };
                rows.push(HistRow {
                    is_search: true,
                    id: s_row.id.unwrap_or(0),
                    day_header: day_header(s_row.ts, &mut last_day),
                    when: s_row
                        .ts
                        .with_timezone(&chrono::Local)
                        .format("%H:%M")
                        .to_string(),
                    query,
                    source,
                    source_url,
                    cached_live,
                    engines: s_row
                        .engines
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    result_count: s_row.result_count.to_string(),
                    latency: s_row.latency_ms.to_string(),
                    client: s_row.client.label(),
                    clicks,
                    delete_confirm,
                    rerun_url: format!("/search?q={encoded}"),
                    json_url: format!("/api/search?q={encoded}"),
                });
            }
            HistoryItem::Click(c) => {
                let absorbed = c.query_hash.as_ref().is_some_and(|h| {
                    search_hashes.contains(h.as_str()) || off_window.contains(h.as_str())
                });
                if absorbed {
                    continue;
                }
                rows.push(HistRow {
                    is_search: false,
                    id: 0,
                    day_header: day_header(c.ts, &mut last_day),
                    when: c
                        .ts
                        .with_timezone(&chrono::Local)
                        .format("%H:%M")
                        .to_string(),
                    query: String::new(),
                    source: common::DASH.to_string(),
                    source_url: String::new(),
                    cached_live: false,
                    engines: common::DASH.to_string(),
                    result_count: common::DASH.to_string(),
                    latency: common::DASH.to_string(),
                    client: c.client.label(),
                    clicks: vec![click_line(&c)],
                    delete_confirm: hs::DELETE_CONFIRM.to_string(),
                    rerun_url: String::new(),
                    json_url: String::new(),
                });
            }
        }
    }
    rows
}

/// `YYYY-MM-DD` the first time each local day appears in the feed.
fn day_header(ts: chrono::DateTime<chrono::Utc>, last_day: &mut String) -> Option<String> {
    let day = ts
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string();
    if day == *last_day {
        None
    } else {
        *last_day = day.clone();
        Some(day)
    }
}

/// One nested click line: domain, title (falling back to the URL), `#n`
/// (the beacon's 0-based position shown 1-based).
fn click_line(c: &ClickRow) -> ClickLine {
    ClickLine {
        domain: c.url.host_str().unwrap_or("").to_string(),
        url: c.url.as_str().to_string(),
        title: if c.title.is_empty() {
            c.url.as_str().to_string()
        } else {
            c.title.clone()
        },
        position: (c.position + 1).to_string(),
    }
}

pub(crate) fn render_err(e: askama::Error, request_id: uuid::Uuid) -> ApiError {
    ApiError::internal(format!("template render failed: {e}")).with_request_id(Some(request_id))
}

fn render_html(page: Page, request_id: uuid::Uuid) -> Result<Html<String>, ApiError> {
    page.render()
        .map_err(|e| render_err(e, request_id))
        .map(Html)
}

#[cfg(test)]
mod tests {
    use super::xml_attribute_escape;

    #[test]
    fn xml_attribute_values_escape_markup_delimiters() {
        assert_eq!(
            xml_attribute_escape("https://search.localhost/?q=\"a&b'<x>"),
            "https://search.localhost/?q=&quot;a&amp;b&apos;&lt;x&gt;"
        );
    }
}
