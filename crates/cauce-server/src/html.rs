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

use crate::app::AppState;
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
