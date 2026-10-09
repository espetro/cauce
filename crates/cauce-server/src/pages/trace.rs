//! `/trace/{id}` — the request-trace replay view the SPA's audit tab
//! links to. Renders the `cauce trace` timeline — the same
//! [`trace::Trace`] / [`trace::render_trace`] pair the CLI runs — plus a
//! per-span HTML list built from that structured trace, so the HTML and
//! terminal views can never drift.
//!
//! FX-06 kept this page server-rendered: it has no SPA route and no JSON
//! twin, and the audit tab deep-links to it (`/api/traces/{id}` covers the
//! data plane). It renders without a template engine — small string
//! interpolation through the shared [`crate::pages::doc`] shell.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::io;

use axum::Extension;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;
use crate::observability::trace::{self, TraceError};
use crate::pages::{doc, esc, short_id};
use rust_i18n::t;

/// One span in the `/trace/{id}` HTML list.
struct SpanView {
    /// Engine id, or the span name when no `engine` field was recorded.
    name: String,
    /// Busy time (`820 ms`), `-` when the span never closed.
    elapsed: String,
    /// The recorded `status` field; empty when the span carries none.
    status: String,
    /// `span-status-ok`/`span-status-error`; empty for other statuses
    /// (they render with the neutral body color).
    status_class: &'static str,
    /// `N results`; empty when the span recorded no `results` field.
    results: String,
    /// The span's merged raw fields, pretty-printed.
    fields_json: String,
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
    let wants_json = accept.contains("application/json") && !accept.contains("text/html");

    let Ok(uuid) = id.parse::<uuid::Uuid>() else {
        if wants_json {
            return Err(ctx.bad_request(format!("trace id {id:?} is not a UUID")));
        }
        return Ok(trace_frame(
            &ctx,
            &id,
            &t!("trace.bad_id"),
            StatusCode::BAD_REQUEST,
        ));
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
        return Ok(trace_frame(&ctx, &traced, &notice, StatusCode::NOT_FOUND));
    }

    let t = trace::Trace::build(&traced, &records);
    let summary = t.summary();
    let timeline = t.render();
    let spans: Vec<SpanView> = t.spans().iter().map(span_view).collect();

    Ok(page(
        &ctx,
        &traced,
        &summary_line(&summary),
        &timeline,
        &spans,
        "",
        StatusCode::OK,
    ))
}

/// `{kind} "{query}" · ts · ms · outcome` — the same reading the `cauce
/// trace` summary line renders. Fields are escaped individually; the
/// separators and the quote marks stay literal.
fn summary_line(summary: &trace::TraceSummary) -> String {
    if summary.kind.is_empty() {
        return String::new();
    }
    let mut line = esc(&summary.kind);
    if let Some(q) = &summary.query {
        line.push_str(&format!(" \"{}\"", esc(q)));
    }
    let ts = local_minute(summary.ts);
    if !ts.is_empty() {
        line.push_str(&format!(" · {}", esc(&ts)));
    }
    line.push_str(&format!(
        " · {} · {}",
        esc(&summary
            .total_ms
            .map(|ms| format!("{ms:.0} {}", t!("trace.ms")))
            .unwrap_or_else(|| t!("common.dash").to_string())),
        esc(&summary.outcome)
    ));
    line
}

fn page(
    ctx: &RequestCtx,
    traced: &str,
    summary: &str,
    timeline: &str,
    spans: &[SpanView],
    notice: &str,
    status: StatusCode,
) -> Response {
    let mut body = String::with_capacity(4096);
    body.push_str(&format!(
        "<div class=\"meta\"><code class=\"request-id\" id=\"traced-id\">{}</code></div>",
        esc(traced)
    ));
    if !notice.is_empty() {
        body.push_str(&format!("<p class=\"empty\">{}</p>", esc(notice)));
    } else {
        if !summary.is_empty() {
            body.push_str(&format!("<p class=\"meta\">{summary}</p>"));
        }
        body.push_str(&format!(
            "<pre class=\"trace\" tabindex=\"0\" role=\"region\" aria-label=\"{}\">{}</pre>",
            esc(&t!("trace.timeline_region")),
            esc(timeline)
        ));
        if !spans.is_empty() {
            body.push_str(&format!("<h2>{}</h2>", esc(&t!("trace.spans_heading"))));
            for span in spans {
                body.push_str("<details class=\"span\"><summary>");
                body.push_str(&format!("{} · {}", esc(&span.name), esc(&span.elapsed)));
                if !span.status.is_empty() {
                    body.push_str(" · <span");
                    if !span.status_class.is_empty() {
                        body.push_str(&format!(" class=\"{}\"", span.status_class));
                    }
                    body.push_str(&format!(">{}</span>", esc(&span.status)));
                }
                if !span.results.is_empty() {
                    body.push_str(&format!(" · {}", esc(&span.results)));
                }
                body.push_str(&format!(
                    "</summary><pre tabindex=\"0\" role=\"group\" aria-label=\"{}\">{}</pre></details>",
                    esc(&t!("trace.span_region")),
                    esc(&span.fields_json)
                ));
            }
        }
    }
    body.push_str(&format!(
        "<div class=\"meta\">{} <code class=\"request-id\">{}</code></div>",
        esc(&t!("common.request_label")),
        esc(&ctx.request_id.as_uuid().to_string())
    ));

    (
        status,
        doc(
            &format!(
                "{} {}",
                t!("trace.page_title").to_lowercase(),
                short_id(traced)
            ),
            &t!("trace.page_title"),
            "/app/admin?tab=audit",
            &t!("trace.back_to_audit"),
            &body,
        ),
    )
        .into_response()
}

/// Error/notice frame: renders the trace page shell with `notice` under
/// `status` (the same shape [`trace`] answers for real traces).
fn trace_frame(ctx: &RequestCtx, traced_id: &str, notice: &str, status: StatusCode) -> Response {
    page(ctx, traced_id, "", "", &[], notice, status)
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
        },
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
