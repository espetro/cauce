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

use askama::Template;
use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use cauce_core::{CacheKey, EngineStatus, SearchRequest, SearchResponse, Source};
use rust_embed::Embed;
use serde_json::json;

use cauce_core::config::EngineEntry;
use cauce_core::{EngineHealthRow, EngineId, HistoryFilter, HistoryItem, config::EngineKind};

use crate::app::{AppState, RouterOptions};
use crate::error::ApiError;
use crate::handlers::{QueryParams, search_inner};
use crate::middleware::RequestCtx;

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
static STYLE_CSS: LazyLock<String> = LazyLock::new(|| asset_string("style.css"));
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

fn prefers_json(accept: &str) -> bool {
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

// ---------------------------------------------------------------------------
// W2-05 `/engines` page
// ---------------------------------------------------------------------------

/// One `/engines` card. Plain strings so Askama only needs `Display`; the
/// card doubles as the `outerHTML` swap target for the card's actions.
#[derive(Debug)]
pub(crate) struct EngineCard {
    id: String,
    /// `declarative` | `exec` | `replay` | `—` (health-only leftovers).
    kind: String,
    /// Effective tier (`1`/`2`/`3`) — the live engine's own tier, else the
    /// `[[engines]]` override, else `—`.
    tier: String,
    enabled: bool,
    /// The resolved config names this engine (file entry or built-in).
    configured: bool,
    /// Live in the running pipeline's fan-out set.
    live: bool,
    /// `Closed` | `Open` | `HalfOpen` (capitalized; the acceptance asserts
    /// on this spelling).
    breaker: String,
    /// CSS class fragment of `breaker` (`closed`/`open`/`half-open`).
    breaker_class: String,
    /// Humanized time left on an open breaker (`"14m 32s"`); empty
    /// otherwise. `breaker_until` in the past reads "elapsed — next call
    /// probes", which is the lazy `Open -> HalfOpen` truth.
    breaker_remaining: String,
    ewma: String,
    last_ok: String,
    last_error: String,
    p95: String,
    reliability: String,
    /// Searches served by this engine since UTC midnight (`search_log`).
    requests_today: u64,
    /// `POST` target flipping `enabled`; label switches with the state.
    toggle_url: String,
    toggle_label: String,
}

/// `engines.html` — the full page shell.
#[derive(Template)]
#[template(path = "engines.html")]
struct EnginesPage {
    cards: Vec<EngineCard>,
    /// `CAUCE_ENGINES` pins the enabled set; toggles then still write the
    /// file but the page explains why resolved `enabled` does not move.
    engines_pinned: bool,
    request_id: String,
    short_request_id: String,
    htmx_js: String,
    json_enc_js: String,
    style_css: String,
}

/// `engine_card.html` — one card, also the HX action response.
#[derive(Template)]
#[template(path = "engine_card.html")]
struct EngineCardPartial {
    card: EngineCard,
}

/// `GET /engines` (W2-05): one card per configured engine (plus engines the
/// health tracker still knows from a previous config), reading the same
/// planes as `/api/engines` + `/api/stats` + `/api/history`.
pub async fn engines(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> Result<Html<String>, ApiError> {
    let rid = ctx.request_id.as_uuid().to_string();
    let page = EnginesPage {
        cards: engine_cards(&state).await?,
        engines_pinned: std::env::var("CAUCE_ENGINES").is_ok_and(|v| !v.trim().is_empty()),
        request_id: rid.clone(),
        short_request_id: short_id(&rid),
        htmx_js: HTMX_JS.clone(),
        json_enc_js: JSON_ENC_JS.clone(),
        style_css: STYLE_CSS.clone(),
    };
    page.render()
        .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
        .map(Html)
}

/// The re-rendered card partial an HX action response swaps in
/// (`hx-swap="outerHTML"` on `.engine-card`). Called by the `POST
/// /api/engines/{id}/reset` and `.../enabled` handlers when the request is
/// an HTMX one.
pub(crate) async fn engine_card(
    state: &AppState,
    id: &EngineId,
    request_id: uuid::Uuid,
) -> Result<Response, ApiError> {
    let ctx = CardCtx::load(state).await?;
    let Some(card) = ctx.card(id) else {
        return Err(
            ApiError::not_found(format!("no such engine {id}")).with_request_id(Some(request_id))
        );
    };
    let tpl = EngineCardPartial { card };
    Ok(Html(tpl.render().map_err(|e| render_err(e, request_id))?).into_response())
}

/// Data planes the cards read, loaded once per page render / action.
struct CardCtx {
    entries: Vec<EngineEntry>,
    live: Vec<(EngineId, cauce_core::Tier)>,
    health: std::collections::BTreeMap<String, EngineHealthRow>,
    metrics: std::collections::BTreeMap<String, cauce_core::EngineMetricStats>,
    /// Engine id -> searches it served since UTC midnight (`search_log`).
    requests_today: std::collections::BTreeMap<String, u64>,
}

impl CardCtx {
    async fn load(state: &AppState) -> Result<Self, ApiError> {
        let entries = state.with_config(|cfg| cfg.engines.clone());
        let live = state
            .pipeline()
            .engines()
            .iter()
            .map(|e| (e.id(), e.tier()))
            .collect();
        let health = state
            .pipeline()
            .health()
            .snapshot()
            .into_iter()
            .map(|r| (r.engine.to_string(), r))
            .collect();
        let metrics = cauce_core::metrics::engine_stats()
            .into_iter()
            .map(|m| (m.engine.to_string(), m))
            .collect();

        // "Requests today": today's `search_log` rows naming the engine —
        // the same plane `/api/history` reads. The 1000-row cap matches
        // the handler's `limit` clamp; a heavier day just undercounts.
        let midnight = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|t| t.and_utc());
        let history = state
            .store()
            .list_history(&HistoryFilter {
                since: midnight,
                q: None,
                limit: 1000,
            })
            .await
            .map_err(|e| ApiError::store(&e))?;
        let mut requests_today = std::collections::BTreeMap::new();
        for item in history {
            if let HistoryItem::Search(row) = item {
                for engine in &row.engines {
                    *requests_today.entry(engine.to_string()).or_insert(0) += 1;
                }
            }
        }

        Ok(Self {
            entries,
            live,
            health,
            metrics,
            requests_today,
        })
    }

    /// One card per engine in the union of configured entries, live
    /// engines and known health rows, sorted by id.
    fn cards(&self) -> Vec<EngineCard> {
        let mut ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        ids.extend(self.entries.iter().map(|e| e.id.to_string()));
        ids.extend(self.live.iter().map(|(id, _)| id.to_string()));
        ids.extend(self.health.keys().cloned());
        ids.iter()
            .filter_map(|id| self.card(&EngineId::from(id)))
            .collect()
    }

    fn card(&self, id: &EngineId) -> Option<EngineCard> {
        let entry = self.entries.iter().find(|e| e.id == *id);
        let live_tier = self.live.iter().find(|(e, _)| e == id).map(|(_, t)| *t);
        // A card exists only for engines the config or the pipeline knows
        // about, or that still have a persisted health row.
        if entry.is_none() && live_tier.is_none() && !self.health.contains_key(id.as_str()) {
            return None;
        }
        let configured = entry.is_some();
        let enabled = entry.map(|e| e.enabled).unwrap_or(live_tier.is_some());
        let kind = entry.map(|e| kind_label(e.kind)).unwrap_or_else(|| {
            if live_tier.is_some() {
                "declarative" // auto-registered spec engines
            } else {
                "—"
            }
        });
        let tier = live_tier
            .or_else(|| entry.and_then(|e| e.tier))
            .map(|t| t.as_u8().to_string())
            .unwrap_or_else(|| "—".to_string());

        let (breaker, breaker_class, breaker_remaining, ewma, last_ok, last_error) =
            match self.health.get(id.as_str()) {
                Some(row) => health_fields(row),
                None => (
                    "Closed".to_string(),
                    "closed".to_string(),
                    String::new(),
                    "—".to_string(),
                    "—".to_string(),
                    "—".to_string(),
                ),
            };

        let metrics = self.metrics.get(id.as_str());
        let p95 = metrics
            .filter(|m| m.requests > 0)
            .map(|m| format!("{} ms", m.total.p95_ms))
            .unwrap_or_else(|| "—".to_string());
        let reliability = metrics
            .filter(|m| m.requests > 0)
            .map(|m| format!("{:.1}%", m.reliability_pct))
            .unwrap_or_else(|| "—".to_string());
        let requests_today = *self.requests_today.get(id.as_str()).unwrap_or(&0);

        Some(EngineCard {
            toggle_url: format!(
                "/api/engines/{}/enabled?enabled={}",
                urlencoding::encode(id.as_str()),
                !enabled
            ),
            toggle_label: if enabled { "Disable" } else { "Enable" }.to_string(),
            id: id.to_string(),
            kind: kind.to_string(),
            tier,
            enabled,
            configured,
            live: live_tier.is_some(),
            breaker,
            breaker_class,
            breaker_remaining,
            ewma,
            last_ok,
            last_error,
            p95,
            reliability,
            requests_today,
        })
    }
}

fn kind_label(kind: EngineKind) -> &'static str {
    match kind {
        EngineKind::Declarative => "declarative",
        EngineKind::Exec => "exec",
        EngineKind::Replay => "replay",
    }
}

/// Card fields derived from an `engine_health` row.
fn health_fields(row: &EngineHealthRow) -> (String, String, String, String, String, String) {
    use cauce_core::BreakerState;
    let (label, class, remaining) = match row.breaker {
        BreakerState::Closed => ("Closed", "closed", String::new()),
        BreakerState::HalfOpen => ("HalfOpen", "half-open", String::new()),
        BreakerState::Open => {
            let remaining = row
                .breaker_until
                .map(|until| {
                    let left = until - chrono::Utc::now();
                    if left.num_seconds() <= 0 {
                        "window elapsed — next call probes".to_string()
                    } else {
                        format!("resets in {}", humanize(left))
                    }
                })
                .unwrap_or_default();
            ("Open", "open", remaining)
        }
    };
    let ewma = if row.ewma_ms > 0.0 {
        format!("{:.0} ms", row.ewma_ms)
    } else {
        "—".to_string()
    };
    let last_ok = row
        .last_ok_at
        .map(|t| format!("{} ago", humanize(chrono::Utc::now() - t)))
        .unwrap_or_else(|| "—".to_string());
    let last_error = row
        .last_error
        .as_deref()
        .map(|e| e.chars().take(160).collect())
        .unwrap_or_else(|| "—".to_string());
    (
        label.to_string(),
        class.to_string(),
        remaining,
        ewma,
        last_ok,
        last_error,
    )
}

/// `90m 12s`-style duration rendering for breaker windows and `last_ok`.
fn humanize(d: chrono::Duration) -> String {
    let secs = d.num_seconds().max(0);
    if secs >= 3600 {
        format!("{}h {}m", secs / 3600, secs % 3600 / 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

/// Every card for `GET /engines`.
async fn engine_cards(state: &AppState) -> Result<Vec<EngineCard>, ApiError> {
    Ok(CardCtx::load(state).await?.cards())
}

fn short_id(request_id: &str) -> String {
    request_id.chars().take(8).collect()
}

fn render_err(e: askama::Error, request_id: uuid::Uuid) -> ApiError {
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
