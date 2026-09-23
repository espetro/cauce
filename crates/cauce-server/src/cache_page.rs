//! `/cache` HTMX page (W2-04): the cache admin surface.
//!
//! One page: a paginated `cache_entries` list (newest first, expired rows
//! included and marked), a `q` filter through the shared `GET /api/cache`
//! listing logic, and deletes that call the audited `DELETE /api/cache*` JSON
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
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use cauce_core::CachedSearch;
use chrono::Utc;
use rust_embed::Embed;

use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;
use crate::strings::cache as copy;

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
/// One rendered `cache_entries` row (plain strings so Askama only needs
/// `Display`).
#[derive(Debug)]
struct Row {
    key: String,
    query: String,
    /// Absolute creation time in local time, `YYYY-MM-DD HH:MM`.
    created: String,
    /// `expires_at` rendered relative to render time: "expires in 42m" or
    /// "expired 3m ago".
    expires: String,
    expired: bool,
    /// Hit count with singular/plural ("1 hit" / "3 hits").
    hits_label: String,
    engines: String,
    /// Approximate stored `payload_json` size, human readable.
    size: String,
}

/// The `/cache` page.
#[derive(Template)]
#[template(path = "cache.html")]
struct CachePage {
    /// The shared header's active nav item.
    nav_active: &'static str,
    /// Active `q` filter (empty when unfiltered).
    q: String,
    searching: bool,
    rows: Vec<Row>,
    /// Count line under the filter form ("34 entries", or "3 matching
    /// entries" while filtering).
    count_line: String,
    /// One-page cap note shown only while filtering ("showing the newest
    /// 50 matches").
    filtered_cap: String,
    /// Empty-state line (filter-aware); empty string when rows exist.
    empty_line: String,
    /// Empty when no page exists in that direction (or while filtering).
    prev_url: String,
    next_url: String,
    request_id: String,
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

/// The payload-fetch error fragment: a single inline line rendered by the
/// `Accept: text/html` arm of `GET /api/cache/{key}` when the request
/// fails (missing key, malformed key, store error).
#[derive(Template)]
#[template(path = "cache_payload_error.html")]
struct PayloadError {
    line: String,
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

    // Use the same parsing, filters, limits, and store selection as
    // GET /api/cache. One extra unfiltered row detects a next page without
    // a COUNT(*); filtered results keep the JSON handler's bounded limit.
    let listing = crate::handlers::cache_list_data(&state, &ctx, &uri, true).await?;
    let mut entries = listing.entries;
    let limit = listing.limit;
    let offset = listing.offset;
    let q = listing.query.unwrap_or_default();
    let searching = !q.is_empty();
    let has_next = !searching && entries.len() > limit as usize;
    entries.truncate(limit as usize);
    let shown = entries.len();
    let rows: Vec<Row> = entries.iter().map(row).collect();

    let (prev_url, next_url) = if searching {
        (String::new(), String::new())
    } else {
        let prev = (offset > 0).then(|| pager_url(offset.saturating_sub(limit), limit));
        let next = has_next.then(|| pager_url(offset + limit, limit));
        (prev.unwrap_or_default(), next.unwrap_or_default())
    };

    let rid = ctx.request_id.as_uuid().to_string();
    let page = CachePage {
        nav_active: "cache",
        q: q.clone(),
        searching,
        count_line: count_line(shown, searching),
        filtered_cap: if searching {
            copy::FILTERED_CAP.replace("{n}", &limit.to_string())
        } else {
            String::new()
        },
        empty_line: if rows.is_empty() {
            if searching {
                copy::EMPTY_FILTERED.replace("{q}", &q)
            } else {
                copy::EMPTY.to_string()
            }
        } else {
            String::new()
        },
        rows,
        prev_url,
        next_url,
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

/// `Accept: text/html` error arm of `GET /api/cache/{key}`: answers a
/// one-line fragment the row expander can drop into the payload block,
/// so a failed lazy load renders inline instead of sticking at
/// `loading...` on the JSON envelope.
///
/// The fragment always names the failing status in its text. For the
/// page's own htmx fetch (`HX-Request: true`) it answers `200`: htmx
/// swaps 2xx fragments in place, while a real 4xx/5xx would skip the
/// swap and log a console error the page cannot suppress (Chromium logs
/// "Failed to load resource" and htmx `console.error`s any
/// `htmx:responseError`). Non-htmx callers get the real status back.
/// The `hx-on::response-error` handler on the details stays as the
/// fallback for error statuses raised outside the handler (host guard,
/// proxy, transport).
pub(crate) fn payload_error(status: StatusCode, htmx_request: bool) -> Response {
    let line = copy::PAYLOAD_ERROR.replace("{status}", &status.as_u16().to_string());
    let out_status = if htmx_request { StatusCode::OK } else { status };
    match (PayloadError { line }).render() {
        Ok(html) => (out_status, Html(html)).into_response(),
        // The fragment is a static one-liner; a render failure still gets
        // an empty body rather than a swapped-in envelope.
        Err(_) => (out_status, Html(String::new())).into_response(),
    }
}

/// True when `Accept` asks for HTML — the `GET /api/cache/{key}`
/// fragment arm keys on this (`handlers::cache_get` stays the JSON arm).
pub(crate) fn accepts_html(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"))
}

/// True for htmx-issued requests (`HX-Request: true`, sent on every
/// `hx-*` call). Used by the error arm to pick a swap-friendly status.
pub(crate) fn is_htmx(headers: &HeaderMap) -> bool {
    headers
        .get("hx-request")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "true")
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
    let expired = e.expires_at <= now;
    let rel = human_seconds(
        e.expires_at
            .signed_duration_since(now)
            .num_seconds()
            .unsigned_abs(),
    );
    Row {
        key: e.key.as_str().to_string(),
        query: e.query.clone(),
        created: e
            .created_at
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        expires: if expired {
            copy::EXPIRED_AGO.replace("{rel}", &rel)
        } else {
            copy::EXPIRES_IN.replace("{rel}", &rel)
        },
        expired,
        hits_label: format!(
            "{} {}",
            e.hits,
            plural(e.hits, copy::HIT_ONE, copy::HIT_MANY)
        ),
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

/// `34 entries` unfiltered, `3 matching entries` under an active `q`.
fn count_line(shown: usize, searching: bool) -> String {
    let word = plural(shown as u64, copy::ENTRY_ONE, copy::ENTRY_MANY);
    if searching {
        format!("{shown} {} {word}", copy::MATCHING)
    } else {
        format!("{shown} {word}")
    }
}

/// Singular/plural word pick (`1 hit` / `3 hits`).
fn plural<'a>(n: u64, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
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
