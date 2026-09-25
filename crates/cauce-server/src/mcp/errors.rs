//! `ErrorData` mapping helpers shared by the MCP tool bodies.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

#[cfg(feature = "archive")]
use cauce_core::ArchiveError;
use cauce_core::{EngineError, PipelineError, StoreError};
use serde::Serialize;

use super::*;

/// Serialize `value` into a structured tool result (`content` + `structuredContent`).
pub(super) fn structured<T: Serialize>(
    value: &T,
    request_id: Uuid,
) -> Result<CallToolResult, ErrorData> {
    serde_json::to_value(value)
        .map(CallToolResult::structured)
        .map_err(|e| internal_error(e.to_string(), request_id))
}

/// `data` always carries `request_id` so even failures are traceable.
fn with_request_id(request_id: Uuid) -> Option<Value> {
    Some(json!({ "request_id": request_id }))
}

pub(super) fn invalid_params(message: impl Into<String>, request_id: Uuid) -> ErrorData {
    ErrorData::invalid_params(message.into(), with_request_id(request_id))
}

pub(super) fn internal_error(message: impl Into<String>, request_id: Uuid) -> ErrorData {
    ErrorData::internal_error(message.into(), with_request_id(request_id))
}

pub(super) fn store_error(e: &StoreError, request_id: Uuid) -> ErrorData {
    ErrorData::internal_error(
        format!("store: {e}"),
        Some(json!({ "error": "store_error", "request_id": request_id })),
    )
}

/// `ArchiveError` -> MCP error (W5-01): caller faults (bad URL, bad key)
/// are `invalid_params`; upstream/extraction failures carry their own
/// `data.error` label so agents can branch on the cause.
#[cfg(feature = "archive")]
pub(super) fn archive_error(e: &ArchiveError, request_id: Uuid) -> ErrorData {
    let label = match e {
        // Egress-guard refusals are caller faults like a bad URL.
        ArchiveError::InvalidUrl(_) | ArchiveError::Blocked(_) => {
            return invalid_params(e.to_string(), request_id);
        }
        ArchiveError::Fetch(EngineError::Timeout) => "upstream_timeout",
        ArchiveError::Fetch(_) | ArchiveError::Status(_) => "upstream_error",
        ArchiveError::Extract(_) => "extraction_failed",
        ArchiveError::Store(se) => return store_error(se, request_id),
    };
    ErrorData::internal_error(
        e.to_string(),
        Some(json!({ "error": label, "request_id": request_id })),
    )
}

/// `PipelineError` -> MCP error. `NoEngines` splits on whether the caller
/// pinned `engines` (bad pin = `invalid_params`, unconfigured = internal);
/// an all-`RateLimited` `AllEnginesFailed` is the settled `rate_limited`
/// error with `retry_after_s` (W1-07 will route the real budget through the
/// same shape).
pub(super) fn pipeline_error(e: &PipelineError, pinned: bool, request_id: Uuid) -> ErrorData {
    match e {
        // Issue #90 strict contract: the pin's offenders and the
        // configured set ride along in `data`, mirroring the 400
        // `unknown_engines` envelope's message.
        PipelineError::UnknownEngines {
            unknown,
            configured,
        } => ErrorData::invalid_params(
            e.to_string(),
            Some(json!({
                "error": "unknown_engines",
                "unknown": unknown.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                "configured": configured.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                "request_id": request_id,
            })),
        ),
        PipelineError::NoEngines if pinned => {
            invalid_params("engines pin matched no configured engine", request_id)
        }
        PipelineError::NoEngines => internal_error("no search engines configured", request_id),
        // W1-07 admission rejection: the real retry budget, not the hint.
        PipelineError::RateLimited { retry_after_s } => ErrorData::new(
            MCP_RATE_LIMITED,
            "rate_limited",
            Some(json!({
                "error": "rate_limited",
                "retry_after_s": retry_after_s,
                "request_id": request_id,
            })),
        ),
        // Fallback while engines surface throttling as per-engine failures
        // rather than an admission rejection.
        PipelineError::AllEnginesFailed(failures)
            if !failures.is_empty()
                && failures
                    .iter()
                    .all(|(_, err)| matches!(err, EngineError::RateLimited)) =>
        {
            ErrorData::new(
                MCP_RATE_LIMITED,
                "rate_limited",
                Some(json!({
                    "error": "rate_limited",
                    "retry_after_s": RATE_LIMIT_RETRY_AFTER_S,
                    "request_id": request_id,
                })),
            )
        }
        PipelineError::AllEnginesFailed(_) => ErrorData::internal_error(
            e.to_string(),
            Some(json!({ "error": "upstream_failed", "request_id": request_id })),
        ),
        // W1-06: every matched engine was breaker-skipped. Temporary, like
        // the HTTP 503 `breaker_open`, but there is no client retry budget
        // to communicate — the breaker window is server-side state.
        PipelineError::BreakerOpen(_) => ErrorData::internal_error(
            e.to_string(),
            Some(json!({ "error": "breaker_open", "request_id": request_id })),
        ),
    }
}
