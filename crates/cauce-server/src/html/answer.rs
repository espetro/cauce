//! The `/answer` page (W4-03, threads in W7-04): the ask form plus a
//! chat thread — one `.answer-turn` per exchange (the user line, the
//! streamed reply, that turn's sources/related links) cloned from
//! `#answer-turn-tpl`, and a bottom-pinned follow-up form revealed once
//! a turn settles. The page's JS POSTs `/api/answer` itself — SSE over
//! `fetch`, since `EventSource` cannot POST — and renders `step` frames
//! as a progress line, `delta` text live, `sources` as numbered cards
//! the `[n]` citation markers link to, and the terminal `done`/`error`
//! frame as metadata or an inline error. Threads are ephemeral page
//! state: each follow-up POST replays the prior turns as `history`;
//! a reload starts a fresh thread (a resumable `threads` table is a
//! documented follow-up).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use askama::Template;
use axum::Extension;
use axum::extract::{Path, State};
use axum::http::Uri;
use axum::response::Html;
use rust_i18n::t;

use super::assets::STYLE_CSS;
use super::{render_err, short_id};
use crate::app::AppState;
use crate::error::ApiError;
use crate::handlers::QueryParams;
use crate::middleware::RequestCtx;
use cauce_core::AnswerStatus;
use cauce_core::ai::render_answer_html;

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
    /// `serde_json`-encoded `q` — the inline script's POST body literal.
    q_json: String,
    /// `answer.*` catalog copy as a `var SA = {...}` JSON literal.
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
        answer_strings: answer_strings(),
    };
    page.render()
        .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
        .map(Html)
}

/// The `answer.*` catalog copy the shell's inline JS interpolates,
/// serialized into the page as `var SA = {...}`; `gen_i18n` mirrors it
/// to `web/src/i18n/answer.json`.
fn answer_strings() -> String {
    serde_json::to_string(&crate::i18n::answer_bundle()).expect("answer strings serialize")
}

/// One source card on the stored render — same `.source-card` markup
/// the streamed page builds client-side (`#src-1-{n}` ids match the
/// `[n]` cite badges).
#[derive(Debug)]
struct SourceCard {
    n: usize,
    url: String,
    title: String,
    host: String,
    snippet: String,
}

/// `GET /answer/{id}` — the durable run render.
#[derive(Template)]
#[template(path = "answer_view.html")]
struct AnswerView {
    /// The shared header's active nav item (same as `/answer`).
    nav_active: &'static str,
    /// Header gate: an answer loop exists.
    answer_available: bool,
    query: String,
    /// `answer.complete` or `answer.error_status`, resolved per status.
    status_label: String,
    model: String,
    /// `answer.confidence` with `{n}` filled; empty when absent.
    confidence_chip: String,
    /// `ungrounded` badge + notice on zero-source done rows.
    ungrounded: bool,
    /// The `cached` chip on `status = cached` replays.
    cached_chip: bool,
    /// `render_answer_html` output — already sanitized; empty on
    /// `status = error` rows (the error line renders instead).
    answer_html: String,
    sources: Vec<SourceCard>,
    /// `(question text, /answer?q= href)` pairs.
    related: Vec<(String, String)>,
    /// `status = error` — the stored error text.
    error: String,
    failed: bool,
    /// `YYYY-MM-DD HH:MM` run time (local).
    when: String,
    request_id: String,
    short_request_id: String,
    style_css: String,
}

/// `GET /answer/{id}` (#254): the durable per-run URL — renders the
/// stored `answer_log` row server-side (no stream), so reloads and
/// back/forward never re-run the loop. The `?q=` page `replaceState`s
/// its history entry onto this URL once the terminal frame carries
/// `log_id`. Unknown ids 404 via the standard `not_found` error.
///
/// Follow-up turns each get their own row id — this page renders just
/// that turn. A resumable full-thread view stays the documented
/// follow-up (the stored-render follow-up form is deliberately hidden:
/// continuing the thread needs the prior turns the page doesn't hold).
pub async fn answer_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(id): Path<i64>,
) -> Result<Html<String>, ApiError> {
    let Some(row) = state
        .store()
        .get_answer_log(id)
        .await
        .map_err(|e| ctx.store(&e))?
    else {
        return Err(ctx.not_found(format!("no answer-log row {id}")));
    };
    let failed = row.status == AnswerStatus::Error;
    let rid = row
        .request_id
        .map(|u| u.to_string())
        .unwrap_or_else(|| ctx.request_id.as_uuid().to_string());
    let page = AnswerView {
        nav_active: "answer",
        answer_available: state.answer().is_some(),
        query: row.query_raw.clone().unwrap_or_else(|| row.query.clone()),
        status_label: if failed {
            t!("answer.error_status").to_string()
        } else {
            t!("answer.complete").to_string()
        },
        model: row.model.clone(),
        confidence_chip: row
            .confidence
            .map(|c| t!("answer.confidence").replace("{n}", &c.to_string()))
            .unwrap_or_default(),
        ungrounded: row.ungrounded,
        cached_chip: row.status == AnswerStatus::Cached,
        answer_html: if failed {
            String::new()
        } else {
            render_answer_html(&row.answer, row.sources.len())
        },
        sources: row
            .sources
            .iter()
            .enumerate()
            .map(|(i, s)| SourceCard {
                n: i + 1,
                url: s.url.as_str().to_string(),
                title: if s.title.is_empty() {
                    s.url.as_str().to_string()
                } else {
                    s.title.clone()
                },
                host: s.url.host_str().unwrap_or("").to_string(),
                snippet: s.snippet.clone(),
            })
            .collect(),
        related: row
            .related_questions
            .iter()
            .map(|rq| (rq.clone(), format!("/answer?q={}", urlencoding::encode(rq))))
            .collect(),
        error: row.error.clone().unwrap_or_default(),
        failed,
        when: row
            .ts
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        short_request_id: short_id(&rid),
        request_id: rid,
        style_css: STYLE_CSS.clone(),
    };
    page.render()
        .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
        .map(Html)
}
