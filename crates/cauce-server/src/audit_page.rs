//! `/trace/{id}` — the last HTMX ops page (FX-05 moved `/audit` to
//! `/app/admin?tab=audit`; `/api/audit` rows still carry `request_id`s
//! that resolve here). Renders the `cauce trace` timeline — the same
//! [`trace::Trace`] / [`trace::render_trace`] pair the CLI runs — plus a
//! per-span HTML list built from that structured trace, so the HTML and
//! terminal views can never drift.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::io;

use askama::Template;
use axum::Extension;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};

use crate::app::AppState;
use crate::error::ApiError;
use crate::html::{STYLE_CSS, prefers_json, render_err, short_id};
use crate::middleware::RequestCtx;
use crate::observability::trace::{self, TraceError};
use rust_i18n::t;
/// One span in the `/trace/{id}` HTML list, summary split into segments so
/// the template can give the status word a color role (`span-status-ok` /
/// `span-status-error`).
#[derive(Debug)]
struct SpanView {
    /// Engine id, or the span name when no `engine` field was recorded.
    name: String,
    /// Busy time (`820 ms`), `-` when the span never closed.
    elapsed: String,
    /// The recorded `status` field; empty when the span carries none.
    status: String,
    /// `span-status-ok`/`span-status-error`; empty for other statuses
    /// (they render with the neutral body color).
    status_class: String,
    /// `N results`; empty when the span recorded no `results` field.
    results: String,
    /// The span's merged raw fields, pretty-printed.
    fields_json: String,
}

/// `/trace/{id}` page.
#[derive(Template)]
#[template(path = "trace.html")]
struct TracePage {
    /// The shared header's active nav item; `/trace/{id}` has no nav
    /// entry of its own, so nothing renders `aria-current`.
    nav_active: &'static str,
    /// W7-01: an answer loop exists — the header shows `/answer`.
    answer_available: bool,
    /// The traced request id (page subject), full form.
    traced_id: String,
    traced_short: String,
    /// Request summary parts (kind, query, timestamp, elapsed, outcome).
    kind: String,
    query: String,
    has_query: bool,
    summary_ts: String,
    summary_ms: String,
    outcome: String,
    /// The rendered `cauce trace` timeline (template-escaped `<pre>` body).
    timeline: String,
    spans: Vec<SpanView>,
    /// Error-frame line for the 404/400 states; empty on the normal page.
    notice: String,
    /// This page request's own id (footer, copyable).
    request_id: String,
    style_css: String,
}

/// `GET /trace/{id}`: the `cauce trace` timeline as HTML.
///
/// HTML arm: a malformed id renders the page frame with `that is not a
/// request id` (400); a well-formed id with no records renders the frame
/// with the retention hint (404). The JSON arm keeps the ApiError envelope.
pub async fn trace(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let wants_json = prefers_json(accept);

    let Ok(uuid) = id.parse::<uuid::Uuid>() else {
        if wants_json {
            return Err(ctx.bad_request(format!("trace id {id:?} is not a UUID")));
        }
        return trace_frame(
            &state,
            &ctx,
            &id,
            &t!("trace.bad_id"),
            StatusCode::BAD_REQUEST,
        );
    };
    let traced = uuid.to_string();
    let logs_dir = state.with_config(|c| c.logs_dir());
    let records = match trace::trace_request(&logs_dir, &traced) {
        Ok(records) => records,
        Err(TraceError::Io(_, e)) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            return Err(ctx.err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                format!("cannot read logs: {e}"),
            ));
        }
    };
    if records.is_empty() {
        if wants_json {
            return Err(ctx.not_found(format!("no trace for {traced}")));
        }
        let days = state.with_config(|c| c.logs.retention_days);
        let notice = t!("trace.no_trace").replace("{days}", &days.to_string());
        return trace_frame(&state, &ctx, &traced, &notice, StatusCode::NOT_FOUND);
    }

    let t = trace::Trace::build(&traced, &records);
    let summary = t.summary();
    let timeline = t.render();
    let spans = t.spans().iter().map(span_view).collect();

    let rid = ctx.request_id.as_uuid().to_string();
    let page = TracePage {
        nav_active: "",
        answer_available: state.answer().is_some(),
        traced_short: short_id(&traced),
        traced_id: traced,
        kind: summary.kind,
        has_query: summary.query.is_some(),
        query: summary.query.unwrap_or_default(),
        summary_ts: local_minute(summary.ts),
        summary_ms: summary
            .total_ms
            .map(|ms| format!("{ms:.0} {}", t!("trace.ms")))
            .unwrap_or_else(|| t!("common.dash").to_string()),
        outcome: summary.outcome,
        timeline,
        spans,
        notice: String::new(),
        request_id: rid,
        style_css: STYLE_CSS.clone(),
    };
    page.render()
        .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
        .map(|html| (StatusCode::OK, Html(html)).into_response())
}

/// `engine · elapsed · status · N results` for one span, kept as separate
/// fields so the template composes the separators and can hang a color
/// class on the status word. Fields the span does not carry come back
/// empty and the template skips their segment.
/// Error/notice frame: renders the trace page shell with `notice` under
/// `status` (the same shape [`trace`] answers for real traces).
fn trace_frame(
    state: &AppState,
    ctx: &RequestCtx,
    traced_id: &str,
    notice: &str,
    status: StatusCode,
) -> Result<Response, ApiError> {
    let page = TracePage {
        nav_active: "",
        answer_available: state.answer().is_some(),
        traced_short: short_id(traced_id),
        traced_id: traced_id.to_string(),
        kind: String::new(),
        has_query: false,
        query: String::new(),
        summary_ts: String::new(),
        summary_ms: String::new(),
        outcome: String::new(),
        timeline: String::new(),
        spans: Vec::new(),
        notice: notice.to_string(),
        request_id: ctx.request_id.as_uuid().to_string(),
        style_css: STYLE_CSS.clone(),
    };
    page.render()
        .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
        .map(|html| (status, Html(html)).into_response())
}

fn span_view(span: &trace::TraceSpan) -> SpanView {
    let status = span
        .fields
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    SpanView {
        name: span
            .fields
            .get("engine")
            .and_then(|v| v.as_str())
            .unwrap_or(&span.name)
            .to_string(),
        elapsed: span
            .busy_ms
            .map(|ms| format!("{ms:.0} {}", t!("trace.ms")))
            .unwrap_or_else(|| t!("common.dash").to_string()),
        status: status.to_string(),
        // Same success/error reading as the tail renderer (`ok` green,
        // `error`/`timeout` red, anything else neutral).
        status_class: match status {
            "ok" => "span-status-ok",
            "error" | "timeout" => "span-status-error",
            _ => "",
        }
        .to_string(),
        results: span
            .fields
            .get("results")
            .and_then(|v| v.as_u64())
            .map(|n| format!("{n} {}", t!("trace.results")))
            .unwrap_or_default(),
        fields_json: serde_json::to_string_pretty(&span.fields).unwrap_or_default(),
    }
}

/// Local `YYYY-MM-DD HH:MM(:SS)` for the summary line; empty when the root
/// span never opened.
fn local_minute(ts: Option<chrono::DateTime<chrono::Utc>>) -> String {
    ts.map(|t| {
        t.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
    .unwrap_or_default()
}
