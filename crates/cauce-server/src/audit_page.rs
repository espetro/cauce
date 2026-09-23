//! `/audit` and `/trace/{id}` pages (W2-06).
//!
//! `/audit` reads the same plane as `GET /api/audit` (`Store::list_audit`),
//! newest first, with `actor`/`action` filters as URL params; each row's
//! `details_json` expands via `<details>` and its `request_id` links to the
//! trace page. `/trace/{id}` renders the `cauce trace` timeline — the same
//! [`trace::trace_request`] / [`trace::render_trace`] pair the CLI runs —
//! inside a `<pre>`, so the HTML and terminal views can never drift.
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
use crate::strings::{self, AuditStrings, TraceStrings};

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
    /// `details_json` pretty-printed for the expandable `<pre>`.
    details: String,
}

/// `/audit` page.
#[derive(Template)]
#[template(path = "audit.html")]
struct AuditPage {
    /// Current `actor` filter value (echoed into the form).
    actor: String,
    /// Current `action` filter value.
    action: String,
    filtered: bool,
    rows: Vec<AuditRowView>,
    request_id: String,
    short_request_id: String,
    style_css: String,
    strings: &'static AuditStrings,
}

/// `/trace/{id}` page.
#[derive(Template)]
#[template(path = "trace.html")]
struct TracePage {
    /// The traced request id (page subject), full form.
    traced_id: String,
    traced_short: String,
    /// The rendered `cauce trace` timeline (template-escaped `<pre>` body).
    timeline: String,
    /// This page request's own id (footer, copyable).
    request_id: String,
    short_request_id: String,
    style_css: String,
    strings: &'static TraceStrings,
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

    let (filter, rows) = audit_list_data(state.store().as_ref(), uri, &ctx).await?;

    let rid = ctx.request_id.as_uuid().to_string();
    let page = AuditPage {
        actor: filter.actor.clone().unwrap_or_default(),
        action: filter.action.clone().unwrap_or_default(),
        filtered: filter.actor.is_some() || filter.action.is_some() || filter.since.is_some(),
        rows: rows.iter().map(row_view).collect(),
        short_request_id: short_id(&rid),
        request_id: rid,
        style_css: STYLE_CSS.clone(),
        strings: &strings::AUDIT,
    };
    Ok(Html(
        page.render()
            .map_err(|e| render_err(e, ctx.request_id.as_uuid()))?,
    )
    .into_response())
}

/// `GET /trace/{id}`: the `cauce trace` timeline as HTML. A non-UUID id is
/// a 400 (same contract as the CLI's usage error); a missing logs dir reads
/// as "no records found" rather than a 500 — a server that has not logged
/// yet has nothing to show.
pub async fn trace(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(id): Path<String>,
) -> Result<Html<String>, ApiError> {
    let uuid = id
        .parse::<uuid::Uuid>()
        .map_err(|_| ctx.bad_request(format!("trace id {id:?} is not a UUID")))?;
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
    let timeline = trace::render_trace(&traced, &records);

    let rid = ctx.request_id.as_uuid().to_string();
    let page = TracePage {
        traced_short: short_id(&traced),
        traced_id: traced,
        timeline,
        short_request_id: short_id(&rid),
        request_id: rid,
        style_css: STYLE_CSS.clone(),
        strings: &strings::TRACE,
    };
    page.render()
        .map_err(|e| render_err(e, ctx.request_id.as_uuid()))
        .map(Html)
}

fn row_view(row: &AuditRow) -> AuditRowView {
    let request_id = row.request_id.map(|u| u.to_string()).unwrap_or_default();
    AuditRowView {
        ts: row.ts.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        actor: row.actor.clone(),
        action: row.action.clone(),
        target: row.target.clone(),
        short_request_id: short_id(&request_id),
        request_id,
        details: serde_json::to_string_pretty(&row.details).unwrap_or_default(),
    }
}
