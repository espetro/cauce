//! `/archive` page (W5-02): the `Accept: text/html` arm of
//! `GET /api/archive` — a search box over `pages_fts`, hit rows whose
//! snippets render the `PAGE_MARK_*` delimiters as `<mark>`, a per-row
//! lazy markdown view and the audited delete — plus the
//! `GET /api/pages/{url}` markdown-fragment arms the rows expand with.
//! Without the `archive` feature the page renders the disabled notice
//! (the `/answer` convention).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use askama::Template;
use axum::Extension;
use axum::response::Html;
#[cfg(feature = "archive")]
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
#[cfg(feature = "archive")]
use cauce_core::{PageHit, PageRow};

#[cfg(feature = "archive")]
use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;
#[cfg(feature = "archive")]
use crate::strings::archive as copy;

use super::{HTMX_JS, STYLE_CSS, render_err};

/// One rendered archive row: display strings plus the URL-keyed action
/// endpoints (`/api/pages/{url}`, percent-encoded).
#[derive(Debug)]
struct Row {
    /// Readability title; falls back to the URL when the page had none.
    title: String,
    host: String,
    /// Absolute fetch time in local time, `YYYY-MM-DD HH:MM`.
    fetched: String,
    /// Snippet split into `(text, is_mark)` runs for the `<mark>` tags.
    snippet_parts: Vec<SnippetPart>,
    /// `hx-get` of the lazy markdown fragment (`/api/pages/{enc_url}`).
    markdown_url: String,
    /// `hx-delete` of the audited row delete (same path).
    delete_url: String,
}

/// One `(text, is_mark)` snippet run: `is_mark` runs render in `<mark>`.
/// Askama escapes `text` at insertion — the snippet is stored page
/// content and is never `|safe`.
#[derive(Debug)]
struct SnippetPart {
    text: String,
    marked: bool,
}

/// The `/archive` page.
#[derive(Template)]
#[template(path = "archive.html")]
struct Archive {
    /// The shared header's active nav item.
    nav_active: &'static str,
    /// The `archive` feature is on (a `build_archive` pipeline exists):
    /// the search form and rows render only then; otherwise the disabled
    /// notice links `/settings` (`/answer`'s convention).
    enabled: bool,
    /// Active `q` (empty on the browsing arm).
    q: String,
    searching: bool,
    /// Count line under the form ("34 pages", "3 matching pages").
    count_line: String,
    /// Empty-state line (query-aware); empty string when rows exist.
    empty_line: String,
    rows: Vec<Row>,
    /// Empty when no page exists in that direction or while searching
    /// (search results are one bounded bm25 page).
    prev_url: String,
    next_url: String,
    request_id: String,
    htmx_js: String,
    style_css: String,
}

/// The markdown fragment the row expander lazy-loads: the stored
/// `markdown` verbatim in a `<pre>` (Askama escapes it at insertion) —
/// deliberately raw text, no renderer: no markdown->HTML renderer is in
/// the dependency set and W5-02 does not add one.
#[cfg(feature = "archive")]
#[derive(Template)]
#[template(path = "archive_markdown.html")]
struct PageMarkdown {
    title: String,
    url: String,
    markdown: String,
}

/// The markdown-fetch error fragment: a single inline line rendered by
/// the `Accept: text/html` arm of `GET /api/pages/{url}` when the lookup
/// fails (the `cache_payload_error` pattern).
#[cfg(feature = "archive")]
#[derive(Template)]
#[template(path = "archive_markdown_error.html")]
struct PageMarkdownError {
    line: String,
}

/// `GET /archive` — identical to `GET /api/archive` with an HTML
/// `Accept`: the shared handler negotiates (the `/history` pattern).
#[cfg(feature = "archive")]
pub async fn archive(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    crate::handlers::archive_search(State(state), Extension(ctx), uri, headers).await
}

/// `GET /archive` without the `archive` feature: the page shell with the
/// disabled notice (no data path — `/api/archive` is unmounted).
#[cfg(not(feature = "archive"))]
pub async fn archive(Extension(ctx): Extension<RequestCtx>) -> Result<Html<String>, ApiError> {
    Archive {
        nav_active: "archive",
        enabled: false,
        q: String::new(),
        searching: false,
        count_line: String::new(),
        empty_line: String::new(),
        rows: Vec::new(),
        prev_url: String::new(),
        next_url: String::new(),
        request_id: ctx.request_id.as_uuid().to_string(),
        htmx_js: HTMX_JS.clone(),
        style_css: STYLE_CSS.clone(),
    }
    .render()
    .map(Html)
    .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
}

/// The `Accept: text/html` arm of `GET /api/archive` (`archive` builds):
/// same params, same row set as the JSON route.
#[cfg(feature = "archive")]
pub(crate) async fn archive_page(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Response, ApiError> {
    let data = crate::handlers::archive_inner(&state, &ctx, &uri).await?;
    let rows: Vec<Row> = data.hits.iter().map(row).collect();
    let searching = data.query.is_some();
    let q = data.query.clone().unwrap_or_default();
    let (prev_url, next_url) = if searching {
        // Search hits are one bm25-ranked page, not offset pages.
        (String::new(), String::new())
    } else {
        let prev = (data.offset > 0)
            .then(|| pager_url(data.offset.saturating_sub(data.limit), data.limit));
        let next = data
            .has_more
            .then(|| pager_url(data.offset + data.limit, data.limit));
        (prev.unwrap_or_default(), next.unwrap_or_default())
    };
    let rid = ctx.request_id.as_uuid().to_string();
    let page = Archive {
        nav_active: "archive",
        // The search/delete data path needs the store, not the fetch
        // pipeline — but `/answer`'s convention shows the notice when
        // the pipeline could not be built (indexing is down).
        enabled: state.archive().is_some(),
        q: q.clone(),
        searching,
        count_line: count_line(rows.len(), searching),
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
    Ok(Html(
        page.render()
            .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
    )
    .into_response())
}

/// `Accept: text/html` arm of `GET /api/pages/{url}`: the row expander's
/// markdown fragment (`handlers::pages_get` calls here after the row is
/// fetched).
#[cfg(feature = "archive")]
pub(crate) fn page_markdown(
    row: &PageRow,
    request_id: uuid::Uuid,
) -> Result<Html<String>, ApiError> {
    PageMarkdown {
        title: if row.title.is_empty() {
            row.url.as_str().to_string()
        } else {
            row.title.clone()
        },
        url: row.url.as_str().to_string(),
        markdown: row.markdown.clone(),
    }
    .render()
    .map(Html)
    .map_err(|e| render_err(e, request_id))
}

/// `Accept: text/html` error arm of `GET /api/pages/{url}`: a one-line
/// fragment the row expander swaps in on failure — `200` for htmx
/// requests (htmx swaps 2xx; a real error status would skip the swap),
/// the real status otherwise (`cache_page::payload_error`'s contract).
#[cfg(feature = "archive")]
pub(crate) fn page_markdown_error(status: StatusCode, htmx_request: bool) -> Response {
    let line = copy::MARKDOWN_FAILED.replace("{status}", &status.as_u16().to_string());
    let out_status = if htmx_request { StatusCode::OK } else { status };
    match (PageMarkdownError { line }).render() {
        Ok(html) => (out_status, Html(html)).into_response(),
        Err(_) => (out_status, Html(String::new())).into_response(),
    }
}

#[cfg(feature = "archive")]
fn row(hit: &PageHit) -> Row {
    let enc = urlencoding::encode(hit.url.as_str());
    Row {
        title: if hit.title.is_empty() {
            hit.url.as_str().to_string()
        } else {
            hit.title.clone()
        },
        host: hit.url.host_str().unwrap_or("").to_string(),
        fetched: hit
            .fetched_at
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        snippet_parts: hit
            .snippet_parts()
            .into_iter()
            .map(|(text, marked)| SnippetPart { text, marked })
            .collect(),
        markdown_url: format!("/api/pages/{enc}"),
        delete_url: format!("/api/pages/{enc}"),
    }
}

#[cfg(feature = "archive")]
fn pager_url(offset: u32, limit: u32) -> String {
    let mut url = format!("/archive?offset={offset}");
    if limit != crate::handlers::ARCHIVE_LIMIT {
        url.push_str(&format!("&limit={limit}"));
    }
    url
}

/// `34 pages` unfiltered, `3 matching pages` under an active `q`.
#[cfg(feature = "archive")]
fn count_line(shown: usize, searching: bool) -> String {
    let word = plural(shown as u64, copy::PAGE_ONE, copy::PAGE_MANY);
    if searching {
        format!("{shown} {} {word}", copy::MATCHING)
    } else {
        format!("{shown} {word}")
    }
}

/// Singular/plural word pick (`1 page` / `3 pages`).
#[cfg(feature = "archive")]
fn plural<'a>(n: u64, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}
