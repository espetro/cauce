//! `/cache` HTMX page (W2-04): the cache admin surface.
//!
//! One page: a paginated `cache_entries` list (newest first, expired rows
//! included and marked), a `q` filter that reads the tier-2 FTS index
//! through `Store::get_lexical` (the same method `GET /api/cache?q=`
//! uses), and deletes that call the audited `DELETE /api/cache*` JSON
//! handlers with `X-Cauce-Client: ui` so the audit actor is `ui`. Row
//! bodies lazy-load the stored payload as pretty JSON from
//! `GET /api/cache/{key}` under `Accept: text/html` — the same handler,
//! content-negotiated, no second data path.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::LazyLock;

use askama::Template;
use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::{Html, IntoResponse, Response};
use cauce_core::CachedSearch;
use chrono::Utc;
use rust_embed::Embed;

use crate::app::AppState;
use crate::error::ApiError;
use crate::handlers::QueryParams;
use crate::middleware::RequestCtx;

/// Static assets vendored under `crates/cauce-server/assets` (embedded
/// per consumer module so pages stay independent of `html.rs`).
#[derive(Embed)]
#[folder = "assets/"]
struct Assets;

fn asset_string(name: &str) -> String {
    Assets::get(name)
        .map(|f| String::from_utf8_lossy(&f.data).into_owned())
        .unwrap_or_default()
}

static HTMX_JS: LazyLock<String> = LazyLock::new(|| asset_string("htmx.min.js"));
static STYLE_CSS: LazyLock<String> = LazyLock::new(|| asset_string("style.css"));

/// Default `/cache` page size.
const PAGE_LIMIT: u32 = 50;
/// Cap on caller-supplied `limit`.
const MAX_LIMIT: u32 = 1_000;
/// Cap for `q` (lexical) result sets: `Store::get_lexical` takes a `u8`.
const FILTER_CAP: u32 = 200;

/// One rendered `cache_entries` row (plain strings so Askama only needs
/// `Display`).
#[derive(Debug)]
struct Row {
    key: String,
    query: String,
    /// Absolute creation time, `YYYY-MM-DD HH:MM:SSZ`.
    created: String,
    /// `expires_at` rendered relative to render time: "in 42m" or
    /// "expired 3m ago".
    expires: String,
    expired: bool,
    hits: u64,
    engines: String,
    /// Approximate stored `payload_json` size, human readable.
    size: String,
}

/// The `/cache` page.
#[derive(Template)]
#[template(path = "cache.html")]
struct CachePage {
    /// Active `q` filter (empty when unfiltered).
    q: String,
    searching: bool,
    rows: Vec<Row>,
    shown: usize,
    /// Empty when no page exists in that direction (or while filtering).
    prev_url: String,
    next_url: String,
    request_id: String,
    short_request_id: String,
    htmx_js: String,
    style_css: String,
}

/// The lazy row-expander fragment: the stored `payload_json` pretty
/// printed. Rendered by the `Accept: text/html` arm of
/// `GET /api/cache/{key}` (see `handlers::cache_get`).
#[derive(Template)]
#[template(path = "cache_payload.html")]
struct Payload {
    payload: String,
}

/// `GET /cache`: paginated cache admin page. `Accept: application/json`
/// delegates to the canonical JSON handler (settled input: pages share
/// the `/api/*` data path).
pub async fn cache(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if prefers_json(&headers) {
        return crate::handlers::cache_list(State(state), Extension(ctx), uri)
            .await
            .map(IntoResponse::into_response);
    }

    let params = QueryParams::parse(uri.query(), &ctx)?;
    params.allow(&ctx, &["q", "limit", "offset"])?;
    let limit = params.u32(&ctx, "limit", PAGE_LIMIT)?.clamp(1, MAX_LIMIT);
    let offset = params.u32(&ctx, "offset", 0)?;
    let q = params.get("q").filter(|v| !v.is_empty());

    let mut entries = match q {
        // Lexical filter: FTS over stored queries, titles and snippets;
        // expired rows included, ranked, no offset (a bounded top-N).
        Some(q) => state
            .store()
            .get_lexical(q, limit.min(FILTER_CAP) as u8)
            .await
            .map_err(|e| ctx.store(&e))?,
        // One extra row detects a next page without a COUNT(*).
        None => state
            .store()
            .list_cache(limit.saturating_add(1), offset)
            .await
            .map_err(|e| ctx.store(&e))?,
    };
    let searching = q.is_some();
    let has_next = !searching && entries.len() > limit as usize;
    entries.truncate(limit as usize);
    let shown = entries.len();
    let rows = entries.iter().map(row).collect();

    let (prev_url, next_url) = if searching {
        (String::new(), String::new())
    } else {
        let prev = (offset > 0).then(|| pager_url(offset.saturating_sub(limit), limit));
        let next = has_next.then(|| pager_url(offset + limit, limit));
        (prev.unwrap_or_default(), next.unwrap_or_default())
    };

    let rid = ctx.request_id.as_uuid().to_string();
    let page = CachePage {
        q: q.unwrap_or_default().to_string(),
        searching,
        rows,
        shown,
        prev_url,
        next_url,
        short_request_id: rid.chars().take(8).collect(),
        request_id: rid,
        htmx_js: HTMX_JS.clone(),
        style_css: STYLE_CSS.clone(),
    };
    page.render()
        .map(Html)
        .map(IntoResponse::into_response)
        .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
}

/// `Accept: text/html` arm of `GET /api/cache/{key}`: the stored payload
/// pretty-printed for the row expander (`handlers::cache_get` calls here
/// after the row is fetched).
pub(crate) fn payload(
    entry: &CachedSearch,
    request_id: uuid::Uuid,
) -> Result<Html<String>, ApiError> {
    let pretty = serde_json::to_string_pretty(&entry.response)
        .map_err(|e| internal(format!("payload serialization failed: {e}"), request_id))?;
    Payload { payload: pretty }
        .render()
        .map(Html)
        .map_err(|e| render_err(e, request_id))
}

/// True when `Accept` asks for HTML — the `GET /api/cache/{key}`
/// fragment arm keys on this (`handlers::cache_get` stays the JSON arm).
pub(crate) fn accepts_html(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"))
}

/// Same negotiation rule as `/search`: JSON wins only when HTML is not
/// also acceptable (a browser `Accept` lists both).
fn prefers_json(headers: &HeaderMap) -> bool {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    accept.contains("application/json") && !accept.contains("text/html")
}

fn pager_url(offset: u32, limit: u32) -> String {
    let mut url = format!("/cache?offset={offset}");
    if limit != PAGE_LIMIT {
        url.push_str(&format!("&limit={limit}"));
    }
    url
}

fn row(e: &CachedSearch) -> Row {
    let now = Utc::now();
    let expires_in = e.expires_at.signed_duration_since(now).num_seconds();
    Row {
        key: e.key.as_str().to_string(),
        query: e.query.clone(),
        created: e.created_at.format("%Y-%m-%d %H:%M:%SZ").to_string(),
        expires: if expires_in >= 0 {
            format!("in {}", human_seconds(expires_in as u64))
        } else {
            format!("expired {} ago", human_seconds(expires_in.unsigned_abs()))
        },
        expired: expires_in < 0,
        hits: e.hits,
        engines: e
            .engines
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        size: human_bytes(
            serde_json::to_string(&e.response)
                .map(|s| s.len() as u64)
                .unwrap_or(0),
        ),
    }
}

/// Seconds -> "45s" / "12m" / "3h" / "2d".
fn human_seconds(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

/// Bytes -> "842 B" / "4.1 KB" / "1.2 MB".
fn human_bytes(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

fn internal(message: String, request_id: uuid::Uuid) -> ApiError {
    ApiError::internal(message).with_request_id(Some(request_id))
}

fn render_err(e: askama::Error, request_id: uuid::Uuid) -> ApiError {
    internal(format!("template render failed: {e}"), request_id)
}
