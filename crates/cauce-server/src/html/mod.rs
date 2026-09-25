//! HTMX search page (`/`) and HTML/HTMX results (`/search`).
//!
//! Assets (HTMX 2.x, the JSON encoding extension and a small CSS file) are
//! embedded at compile time via `rust-embed`; templates are rendered with
//! Askama. No external CDN is used.
//!
//! Module map: `mod.rs` is the shared shell — [`Page`] (the full-page
//! template), [`Row`] (one rendered result), `render_html`, the
//! `Accept`/`HX-Request` negotiation helpers, `render_err` and `short_id`,
//! which every `ui` page uses. The page domains live beside it:
//! [`search`] — `/` + `/search` (badge, engine statuses, stream URL and
//! strings, result rows) plus the `Accept: text/html` arm of
//! `GET /api/search`; [`answer`] — the `/answer` AI-answer page (W4-03);
//! [`history`] — the `/history` feed page;
//! [`settings`] — the `/settings` form page; [`assets`] — the embedded
//! static assets and the `favicon`/`opensearch` endpoints.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod answer;
mod assets;
mod history;
mod search;
mod settings;

use askama::Template;
use axum::http::HeaderMap;
use axum::response::Html;

use crate::error::ApiError;

pub use answer::answer;
pub(crate) use assets::{HTMX_JS, JSON_ENC_JS, STYLE_CSS, VERSION_LABEL};
pub use assets::{favicon, opensearch};
pub use history::history;
pub(crate) use history::{history_page, prefers_html};
pub(crate) use search::search_fragment;
pub use search::{index, search};
pub use settings::settings;
pub(crate) use settings::settings_status;

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
    /// The shared header's active nav item (`"search"` on `/` and
    /// `/search`; `templates/header.html` compares against it).
    nav_active: &'static str,
    q: String,
    has_results: bool,
    show_empty: bool,
    empty_status: String,
    result_count: usize,
    badge: String,
    request_id: String,
    short_request_id: String,
    results: Vec<Row>,
    more_url: String,
    htmx_js: String,
    json_enc_js: String,
    style_css: String,
    is_streaming: bool,
    stream_url: String,
    query_hash: String,
    sse_js: String,
    /// `crate::strings::search` copy the inline JS uses, as a JSON literal.
    stream_strings: String,
    /// W4-03: `/answer?q=...` the meta line links to when an answer loop
    /// exists (`ai` effectively on); empty otherwise and on `/`.
    ask_url: String,
}

/// `Accept` prefers JSON (shared by every `ui` page's content negotiation).
pub(crate) fn prefers_json(accept: &str) -> bool {
    accept.contains("application/json") && !accept.contains("text/html")
}

/// True when `Accept` asks for HTML — the `Accept: text/html` fragment
/// arms on `/api/*` handlers key on this (`handlers::search`,
/// `handlers::engines_list` keep the JSON arm for everything else).
pub(crate) fn accepts_html(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"))
}

/// True for htmx-issued requests (`HX-Request: true`, sent on every
/// `hx-*` call). Fragment error arms use it to pick a swap-friendly
/// status (a real 4xx/5xx would skip the swap and console-error).
pub(crate) fn is_htmx(headers: &HeaderMap) -> bool {
    headers
        .get("hx-request")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "true")
}

/// The footer id every page renders (settled input: copyable request id).
pub(crate) fn short_id(request_id: &str) -> String {
    request_id.chars().take(8).collect()
}

/// Askama render failure -> 500 envelope (shared by the `ui` pages).
pub(crate) fn render_err(e: askama::Error, request_id: uuid::Uuid) -> ApiError {
    ApiError::internal(format!("template render failed: {e}")).with_request_id(Some(request_id))
}

fn render_html(page: Page, request_id: uuid::Uuid) -> Result<Html<String>, ApiError> {
    page.render()
        .map_err(|e| render_err(e, request_id))
        .map(Html)
}
