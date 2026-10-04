//! `GET /api/report` (#240): the support-report download — the collector's
//! [`ReportBundle`] served as an attachment (`cauce-report-<ts>.json`)
//! so the file drags straight into an issue.

use axum::Extension;
use axum::extract::State;
use axum::http::header::HeaderName;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

use cauce_core::report;

use super::QueryParams;
use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// The prefilled `issues/new` URL rides a header rather than the body:
/// the body is the file itself (the schema stays v1-pure) and the header
/// gives the `/settings` link and `curl` callers the share path without
/// any UI chrome.
pub const ISSUE_URL_HEADER: HeaderName = HeaderName::from_static("x-report-issue-url");

/// `GET /api/report?days&include_queries` (also `verbose` for the flag).
/// `days` bounds the stats window, the audit tail and the log files read
/// (7 by default, clamped to `1..=365` like `/api/stats`). The verbose
/// profile is a per-request opt-in — off unless asked for, never
/// persisted.
pub async fn report(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    uri: Uri,
) -> Result<Response, ApiError> {
    let params = QueryParams::parse(uri.query(), &ctx)?;
    params.allow(&ctx, &["days", "include_queries", "verbose"])?;
    let days = params.u32(&ctx, "days", 7)?.clamp(1, 365);
    let include_queries = params.flag(&ctx, "include_queries")? || params.flag(&ctx, "verbose")?;

    let bundle = crate::report::collect(&state, days, include_queries).await;
    let rid = ctx.request_id.as_uuid();
    let header_value = |v: &str| {
        HeaderValue::from_str(v).map_err(|e| {
            ApiError::internal(format!("report header: {e}")).with_request_id(Some(rid))
        })
    };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, header_value("application/json")?),
            (
                header::CONTENT_DISPOSITION,
                header_value(&format!(
                    "attachment; filename=\"{}\"",
                    report::filename(&bundle.generated_at)
                ))?,
            ),
            (ISSUE_URL_HEADER, header_value(&report::issue_url(&bundle))?),
        ],
        bundle.to_json(),
    )
        .into_response())
}
