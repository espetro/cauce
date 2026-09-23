//! `/audit` and `/trace/{id}` pages (W2-06).
//!
//! `/audit` reads the same plane as `GET /api/audit` (`Store::list_audit`),
//! newest first, with `actor`/`action` filters as URL params; each row's
//! `details_json` expands via `<details>` and its `request_id` links to the
//! trace page. `/trace/{id}` renders the `cauce trace` timeline — the same
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
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use cauce_core::AuditRow;

use crate::app::AppState;
use crate::error::ApiError;
use crate::handlers::{audit_list, audit_list_data};
use crate::html::{STYLE_CSS, prefers_json, render_err, short_id};
use crate::middleware::RequestCtx;
use crate::observability::trace::{self, TraceError};
use crate::strings;

/// One audit row pre-rendered to plain strings for the template.
#[derive(Debug)]
struct AuditRowView {
    ts: String,
    actor: String,
    action: String,
    target: String,
    /// Full request id; empty when the row carries none (drives the
    /// `if`-guard rather than `Option` matching in the template).
    request_id: String,
    short_request_id: String,
    /// `details_json` pretty-printed for the expandable `<pre>`; empty when
    /// the stored details are `null` (the row then omits the toggle).
    details: String,
}

/// One `<option>` in a filter `<select>` (Askama cannot compare `&String`
/// loop items to a `String` field, so selection is resolved here).
#[derive(Debug)]
struct Opt {
    value: String,
    selected: bool,
}

/// `/audit` page.
#[derive(Template)]
#[template(path = "audit.html")]
struct AuditPage {
    /// Distinct actors/actions from `Store::audit_facets`, for the selects.
    actor_options: Vec<Opt>,
    action_options: Vec<Opt>,
    filtered: bool,
    /// `no audit rows match actor "x" and action "y".` for the filtered
    /// empty state; unused when the listing is unfiltered.
    filtered_empty: String,
    rows: Vec<AuditRowView>,
    /// The effective `limit` param, echoed into the cap note.
    limit: u32,
    /// `rows.len() == limit`: the listing may hide older rows.
    capped: bool,
    request_id: String,
    style_css: String,
}

/// One span in the `/trace/{id}` HTML list.
#[derive(Debug)]
struct SpanView {
    /// `engine · elapsed · status · N results`.
    summary: String,
    /// The span's merged raw fields, pretty-printed.
    fields_json: String,
}

/// `/trace/{id}` page.
#[derive(Template)]
#[template(path = "trace.html")]
struct TracePage {
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

/// `GET /audit?actor&action&since&limit`: the audit table, newest first.
/// Both representations use the same parsing, defaults, filters, and store query.
pub async fn audit(
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
        return audit_list(State(state), Extension(ctx), uri)
            .await
            .map(|j| j.into_response());
    }

    let store = state.store().clone();
    let (filter, rows) = audit_list_data(store.as_ref(), uri, &ctx).await?;
    let facets = store
        .audit_facets()
        .await
        .map_err(|error| ctx.store(&error))?;

    let filtered = filter.actor.is_some() || filter.action.is_some() || filter.since.is_some();
    let filtered_empty = filtered_empty_message(&filter);
    let capped = rows.len() as u32 == filter.limit;

    let page = AuditPage {
        actor_options: facets
            .actors
            .iter()
            .map(|a| Opt {
                value: a.clone(),
                selected: filter.actor.as_deref() == Some(a.as_str()),
            })
            .collect(),
        action_options: facets
            .actions
            .iter()
            .map(|a| Opt {
                value: a.clone(),
                selected: filter.action.as_deref() == Some(a.as_str()),
            })
            .collect(),
        filtered,
        filtered_empty,
        rows: rows.iter().map(row_view).collect(),
        limit: filter.limit,
        capped,
        request_id: ctx.request_id.as_uuid().to_string(),
        style_css: STYLE_CSS.clone(),
    };
    Ok(Html(
        page.render()
            .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
    )
    .into_response())
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
            strings::trace::BAD_ID,
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
        let notice = strings::trace::NO_TRACE.replace("{days}", &days.to_string());
        return trace_frame(&state, &ctx, &traced, &notice, StatusCode::NOT_FOUND);
    }

    let t = trace::Trace::build(&traced, &records);
    let summary = t.summary();
    let timeline = t.render();
    let spans = t
        .spans()
        .iter()
        .map(|s| SpanView {
            summary: span_summary(s),
            fields_json: serde_json::to_string_pretty(&s.fields).unwrap_or_default(),
        })
        .collect();

    let rid = ctx.request_id.as_uuid().to_string();
    let page = TracePage {
        traced_short: short_id(&traced),
        traced_id: traced,
        kind: summary.kind,
        has_query: summary.query.is_some(),
        query: summary.query.unwrap_or_default(),
        summary_ts: local_minute(summary.ts),
        summary_ms: summary
            .total_ms
            .map(|ms| format!("{ms:.0} {}", strings::trace::MS))
            .unwrap_or_else(|| strings::common::DASH.to_string()),
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

/// Render the trace page frame with a `notice` line (404/400 states).
fn trace_frame(
    _state: &AppState,
    ctx: &RequestCtx,
    traced_id: &str,
    notice: &str,
    status: StatusCode,
) -> Result<Response, ApiError> {
    let page = TracePage {
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

fn row_view(row: &AuditRow) -> AuditRowView {
    let request_id = row.request_id.map(|u| u.to_string()).unwrap_or_default();
    AuditRowView {
        ts: row
            .ts
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        actor: row.actor.clone(),
        action: row.action.clone(),
        target: row.target.clone(),
        short_request_id: short_id(&request_id),
        request_id,
        details: if row.details.is_null() {
            String::new()
        } else {
            serde_json::to_string_pretty(&row.details).unwrap_or_default()
        },
    }
}

/// `engine · elapsed · status · N results` for one span; fields the span
/// does not carry are skipped.
fn span_summary(span: &trace::TraceSpan) -> String {
    let name = span
        .fields
        .get("engine")
        .and_then(|v| v.as_str())
        .unwrap_or(&span.name);
    let mut parts = vec![name.to_string()];
    parts.push(
        span.busy_ms
            .map(|ms| format!("{ms:.0} {}", strings::trace::MS))
            .unwrap_or_else(|| strings::common::DASH.to_string()),
    );
    if let Some(status) = span.fields.get("status").and_then(|v| v.as_str()) {
        parts.push(status.to_string());
    }
    if let Some(results) = span.fields.get("results").and_then(|v| v.as_u64()) {
        parts.push(format!("{results} {}", strings::trace::RESULTS));
    }
    parts.join(" · ")
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

/// `no audit rows match actor "x" and action "y".` with either clause
/// dropped when that filter is unset.
fn filtered_empty_message(filter: &cauce_core::AuditFilter) -> String {
    let mut clauses = Vec::new();
    if let Some(actor) = &filter.actor {
        clauses.push(format!("{} \"{actor}\"", strings::audit::ACTOR_LABEL));
    }
    if let Some(action) = &filter.action {
        clauses.push(format!("{} \"{action}\"", strings::audit::ACTION_LABEL));
    }
    if let Some(since) = &filter.since {
        clauses.push(format!("since {}", since.to_rfc3339()));
    }
    format!(
        "{} {}.",
        strings::audit::FILTERED_EMPTY_PREFIX,
        clauses.join(&format!(" {} ", strings::audit::FILTERED_EMPTY_AND))
    )
}
