//! The `/answer` page (W4-03): the ask form plus the streamed-answer
//! shell. The shell's inline script POSTs `/api/answer` itself — SSE over
//! `fetch`, since `EventSource` cannot POST — and renders `step` frames
//! as a progress line, `delta` text live, `sources` as numbered cards the
//! `[n]` citation markers link to, and the terminal `done`/`error` frame
//! as metadata or an inline error.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use askama::Template;
use axum::Extension;
use axum::extract::State;
use axum::http::Uri;
use axum::response::Html;
use serde_json::json;

use crate::app::AppState;
use crate::error::ApiError;
use crate::handlers::QueryParams;
use crate::middleware::RequestCtx;
use crate::strings::answer as copy;

use super::assets::{HTMX_JS, STYLE_CSS};
use super::{render_err, short_id};

/// Full page, rendered for `GET /answer` (ask form only) and
/// `GET /answer?q=...` (form + stream shell or disabled notice).
#[derive(Template)]
#[template(path = "answer.html")]
struct AnswerPage {
    /// The shared header's active nav item; W7-01 promotes `/answer`
    /// into the primary nav, so this page marks `answer` current.
    nav_active: &'static str,
    /// The header's `/answer` nav link renders only while this is true
    /// (same gate as `enabled` below — kept a separate field because
    /// the include contract wants the header-scoped name).
    answer_available: bool,
    q: String,
    /// `ai` is effectively on: an [`AppState`] answer loop exists, so
    /// `POST /api/answer` will stream. Otherwise the page renders the
    /// disabled notice with a link to `/settings`.
    enabled: bool,
    /// The request carried a non-blank `?q=` — render the stream shell
    /// only then (`/answer` alone is just the ask form).
    has_query: bool,
    request_id: String,
    short_request_id: String,
    style_css: String,
    htmx_js: String,
    /// `serde_json`-encoded `q` — the inline script's POST body literal.
    q_json: String,
    /// `crate::strings::answer` copy as a `var S = {...}` JSON literal.
    answer_strings: String,
}

/// `GET /answer?q=...` — the AI-answer page.
///
/// Three render modes: bare `/answer` is the ask form alone; `?q=` with
/// an available answer loop adds the streaming shell (the page's inline
/// JS owns the POST-SSE exchange); `?q=` without one shows the disabled
/// notice and links `/settings`. Content negotiation stays HTML-only:
/// agents read the `POST /api/answer` stream directly.
pub async fn answer(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Html<String>, ApiError> {
    let params = QueryParams::parse(uri.query(), &ctx)?;
    params.allow(&ctx, &["q"])?;
    let q = params.get("q").unwrap_or_default().to_string();
    let rid = ctx.request_id.as_uuid().to_string();
    let answer_available = state.answer().is_some();
    let page = AnswerPage {
        nav_active: "answer",
        answer_available,
        q_json: serde_json::to_string(&q).expect("query string serializes"),
        has_query: !q.trim().is_empty(),
        q,
        enabled: answer_available,
        request_id: rid.clone(),
        short_request_id: short_id(&rid),
        style_css: STYLE_CSS.clone(),
        htmx_js: HTMX_JS.clone(),
        answer_strings: answer_strings(),
    };
    page.render()
        .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
        .map(Html)
}

/// The `strings::answer` copy the shell's inline JS interpolates,
/// serialized into the page as `var S = {...}` so every user-visible
/// string lives in `crate::strings` (the i18n seam), not in the script.
fn answer_strings() -> String {
    serde_json::to_string(&json!({
        "waiting": copy::WAITING,
        "complete": copy::COMPLETE,
        "error_status": copy::ERROR_STATUS,
        "confidence": copy::CONFIDENCE,
        "cached": copy::CACHED,
        "ungrounded": copy::UNGROUNDED,
        "related": copy::RELATED,
        "sources": copy::SOURCES,
        "retry_after": copy::RETRY_AFTER,
        "stream_failed": copy::STREAM_FAILED,
        "invalid_stream": copy::INVALID_STREAM,
    }))
    .expect("answer strings serialize")
}
