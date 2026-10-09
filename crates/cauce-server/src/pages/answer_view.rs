//! `/answer/{id}` — the durable run render.
//!
//! FX-06 kept this page server-rendered: `?q=` threads live in the SPA,
//! but a reload of `/answer/{id}` must never re-run the loop, so the
//! stored `answer_log` row renders here server-side (the SPA navigates to
//! it via `history.replaceState` once the `done` frame carries `log_id`).
//! `/api/answer-log/{id}` is the JSON twin.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::Extension;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use rust_i18n::t;

use cauce_core::AnswerStatus;
use cauce_core::ai::render_answer_html;

use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;
use crate::pages::{doc, esc, short_id};

/// `GET /answer/{id}` (#254): renders the stored `answer_log` row — no
/// stream — so reloads and back/forward never re-run the loop. Unknown
/// ids 404 via the standard `not_found` error. Follow-up turns each get
/// their own row id — this page renders just that turn; a resumable
/// full-thread view stays the documented follow-up.
pub async fn answer_view(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let Some(row) = state
        .store()
        .get_answer_log(id)
        .await
        .map_err(|e| ctx.store(&e))?
    else {
        return Err(ctx.not_found(format!("no answer-log row {id}")));
    };

    let failed = row.status == AnswerStatus::Error;
    let query = row.query_raw.clone().unwrap_or_else(|| row.query.clone());
    let rid = row
        .request_id
        .map(|u| u.to_string())
        .unwrap_or_else(|| ctx.request_id.as_uuid().to_string());

    let mut meta = String::new();
    meta.push_str(&format!(
        "<span class=\"answer-status\" role=\"status\">{}</span>",
        esc(&if failed {
            t!("answer.error_status").to_string()
        } else {
            t!("answer.complete").to_string()
        })
    ));
    meta.push_str(&format!(
        "<span class=\"meta-chip\">{}</span>",
        esc(&row
            .ts
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string())
    ));
    if !row.model.is_empty() {
        meta.push_str(&format!(
            "<span class=\"meta-chip\">{}</span>",
            esc(&row.model)
        ));
    }
    if let Some(c) = row.confidence {
        meta.push_str(&format!(
            "<span class=\"meta-chip answer-confidence\">{}</span>",
            esc(&t!("answer.confidence").replace("{n}", &c.to_string()))
        ));
    }
    if row.ungrounded {
        meta.push_str(&format!(
            "<span class=\"meta-chip warn answer-ungrounded-badge\">{}</span>",
            esc(&t!("answer.ungrounded_badge"))
        ));
    }
    if row.status == AnswerStatus::Cached {
        meta.push_str(&format!(
            "<span class=\"meta-chip\">{}</span>",
            esc(&t!("answer.cached"))
        ));
    }
    meta.push_str(&format!(
        "<span class=\"request-id\" title=\"{}\">{}</span>",
        esc(&rid),
        esc(short_id(&rid))
    ));

    let mut body = String::with_capacity(4096);
    body.push_str("<section class=\"answer-turn\">");
    body.push_str(&format!("<p class=\"turn-q\">{}</p>", esc(&query)));
    body.push_str(&format!("<div class=\"meta\">{meta}</div>"));
    if row.ungrounded {
        body.push_str(&format!(
            "<p class=\"ungrounded\" role=\"note\">{}</p>",
            esc(&t!("answer.ungrounded"))
        ));
    }
    if failed {
        body.push_str(&format!(
            "<p class=\"field-error answer-error\" role=\"alert\">{}</p>",
            esc(&row.error.unwrap_or_default())
        ));
    } else {
        body.push_str(&format!(
            "<div class=\"answer-text md\">{}</div>",
            render_answer_html(&row.answer, row.sources.len())
        ));
    }
    if !row.sources.is_empty() {
        body.push_str("<div class=\"answer-sources\">");
        body.push_str(&format!(
            "<p class=\"section-label\">{}</p>",
            esc(&t!("answer.sources"))
        ));
        for (i, s) in row.sources.iter().enumerate() {
            let host = s.url.host_str().unwrap_or("");
            let title = if s.title.is_empty() {
                s.url.as_str().to_string()
            } else {
                s.title.clone()
            };
            body.push_str(&format!(
                "<article class=\"source-card\" id=\"src-1-{}\">",
                i + 1
            ));
            body.push_str(&format!("<span class=\"cite-badge\">[{}]</span>", i + 1));
            if !host.is_empty() {
                body.push_str(&format!(
                    "<img src=\"https://icons.duckduckgo.com/ip3/{}.ico\" width=\"16\" height=\"16\" alt=\"\" loading=\"lazy\">",
                    urlencoding::encode(host)
                ));
            }
            body.push_str(&format!(
                "<a href=\"{}\" target=\"_blank\" rel=\"noopener\">{}</a>",
                esc(s.url.as_str()),
                esc(&title)
            ));
            if !host.is_empty() {
                body.push_str(&format!("<span class=\"host\">{}</span>", esc(host)));
            }
            if !s.snippet.is_empty() {
                body.push_str(&format!("<p class=\"snippet\">{}</p>", esc(&s.snippet)));
            }
            body.push_str("</article>");
        }
        body.push_str("</div>");
    }
    if !row.related_questions.is_empty() {
        body.push_str("<div class=\"answer-related\">");
        body.push_str(&format!(
            "<p class=\"section-label\">{}</p>",
            esc(&t!("answer.related"))
        ));
        for rq in &row.related_questions {
            body.push_str(&format!(
                "<p class=\"related-question\"><a href=\"/app/answer?q={}\">{}</a></p>",
                urlencoding::encode(rq),
                esc(rq)
            ));
        }
        body.push_str("</div>");
    }
    body.push_str("</section>");

    Ok(doc(
        &query,
        &t!("common.brand"),
        "/app/answer",
        &t!("common.brand"),
        &body,
    )
    .into_response())
}
