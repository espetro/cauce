//! Anthropic failure classification into the typed [`AiError`] set:
//! the HTTP status first, then the `error.type` tag
//! (`authentication_error`, `rate_limit_error`, `overloaded_error`,
//! `request_too_large`, …) and `error.message` of the
//! `{"type":"error","error":{...}}` envelope. Unlike the OpenAI
//! envelope there are no body-side retry hints — `Retry-After` is the
//! only source.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use reqwest::header::{HeaderMap, RETRY_AFTER};

use crate::ai::AiError;
use crate::ai::http::truncate;

use super::wire::{ApiError, ErrorBody};

/// Non-2xx response → typed error: envelope `error.message` when the
/// body is `{"type":"error", ...}`, else the truncated body text.
pub(super) fn map_error(status: u16, headers: &HeaderMap, body: &[u8]) -> AiError {
    let parsed = serde_json::from_slice::<ErrorBody>(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|b| b.error.message.clone())
        .unwrap_or_else(|| truncate(&String::from_utf8_lossy(body), 512));
    map_error_envelope(
        status,
        parsed.as_ref().and_then(|b| b.error.kind.as_deref()),
        Some(headers),
        message,
    )
}

/// A mid-stream `error` event — no response headers remain, so
/// `retry_after_s` is unavailable; `error.type` acts as the status.
pub(super) fn map_stream_error(err: &ApiError) -> AiError {
    let message = err
        .message
        .clone()
        .unwrap_or_else(|| "stream error".to_string());
    map_error_envelope(
        status_for_kind(err.kind.as_deref()),
        err.kind.as_deref(),
        None,
        message,
    )
}

fn map_error_envelope(
    status: u16,
    kind: Option<&str>,
    headers: Option<&HeaderMap>,
    message: String,
) -> AiError {
    match (status, kind) {
        (401 | 403, _) | (_, Some("authentication_error" | "permission_error")) => {
            AiError::Auth(message)
        }
        // `rate_limit_error` (429) and `overloaded_error` (529) are the
        // same "back off" outcome for the caller.
        (429 | 529, _) | (_, Some("rate_limit_error" | "overloaded_error")) => {
            AiError::RateLimited {
                retry_after_s: retry_after_s(headers),
            }
        }
        (408 | 504, _) => AiError::Timeout,
        _ if is_context_length(status, kind, &message) => AiError::ContextLength(message),
        _ => AiError::Provider { status, message },
    }
}

/// The HTTP status an `error.type` would carry outside the stream —
/// the `error` event doesn't send one.
fn status_for_kind(kind: Option<&str>) -> u16 {
    match kind {
        Some("authentication_error") => 401,
        Some("permission_error") => 403,
        Some("not_found_error") => 404,
        Some("invalid_request_error") => 400,
        Some("request_too_large") => 413,
        Some("rate_limit_error") => 429,
        Some("overloaded_error") => 529,
        _ => 500,
    }
}

/// `Retry-After` in seconds — the only retry hint the Messages API
/// sends.
fn retry_after_s(headers: Option<&HeaderMap>) -> Option<u64> {
    headers
        .and_then(|h| h.get(RETRY_AFTER))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

/// Provider context-length failures: `request_too_large` (413) or a
/// 400/422 `invalid_request_error` whose message names the prompt size.
fn is_context_length(status: u16, kind: Option<&str>, message: &str) -> bool {
    if kind == Some("request_too_large") || status == 413 {
        return true;
    }
    if !matches!(status, 400 | 422) {
        return false;
    }
    let lower = message.to_ascii_lowercase();
    [
        "prompt is too long",
        "context length",
        "context window",
        "maximum context",
        "too many tokens",
        "token limit",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}
