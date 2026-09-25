//! Provider failure classification into the typed [`AiError`] set:
//! the HTTP status first, then the `{"error": ...}` envelope's `code`,
//! `message` and OpenRouter-style `metadata` retry hints.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use reqwest::header::{HeaderMap, RETRY_AFTER};

use crate::ai::AiError;
use crate::ai::http::truncate;

use super::wire::{ErrorBody, ProviderError};

/// Non-2xx response → typed error: envelope message if the body is a
/// `{"error": ...}` object, else the truncated body text.
pub(super) fn map_error(status: u16, headers: &HeaderMap, body: &[u8]) -> AiError {
    let parsed = serde_json::from_slice::<ErrorBody>(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|b| b.error.message.clone())
        .unwrap_or_else(|| truncate(&String::from_utf8_lossy(body), 512));
    map_error_envelope(
        status,
        Some(headers),
        parsed.as_ref().map(|b| &b.error),
        message,
    )
}

/// A mid-stream `{"error": ...}` chunk — no response headers remain, so
/// retry hints come from the envelope alone. A numeric `code` acts as
/// the status (OpenRouter sends `429` there).
pub(super) fn map_stream_error(err: &ProviderError) -> AiError {
    let status = err
        .code
        .as_ref()
        .and_then(|c| c.as_u64())
        .and_then(|c| u16::try_from(c).ok())
        .unwrap_or(200);
    let message = err
        .message
        .clone()
        .unwrap_or_else(|| "stream error".to_string());
    map_error_envelope(status, None, Some(err), message)
}

fn map_error_envelope(
    status: u16,
    headers: Option<&HeaderMap>,
    err: Option<&ProviderError>,
    message: String,
) -> AiError {
    match status {
        401 | 403 => AiError::Auth(message),
        429 => AiError::RateLimited {
            retry_after_s: retry_after_s(headers, err),
        },
        408 | 504 => AiError::Timeout,
        _ if is_context_length(status, err, &message) => AiError::ContextLength(message),
        _ => AiError::Provider { status, message },
    }
}

/// `Retry-After` in seconds. Header first (the standard), then the
/// envelope's own hints: OpenRouter's `metadata.retry_after_seconds`
/// and `metadata.headers.Retry-After`, plus `retry_after` /
/// `retry_after_ms` fields some providers put on the error object.
fn retry_after_s(headers: Option<&HeaderMap>, err: Option<&ProviderError>) -> Option<u64> {
    if let Some(v) = headers
        .and_then(|h| h.get(RETRY_AFTER))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        return Some(v);
    }
    let err = err?;
    let from = |value: Option<&serde_json::Value>, millis: bool| -> Option<u64> {
        let raw = value.and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str()?.trim().parse::<u64>().ok())
        })?;
        Some(if millis { raw.div_ceil(1000) } else { raw })
    };
    if let Some(v) =
        from(err.retry_after.as_ref(), false).or_else(|| from(err.retry_after_ms.as_ref(), true))
    {
        return Some(v);
    }
    let md = err.metadata.as_ref()?;
    for key in [
        "retry_after_seconds",
        "retry_after_seconds_raw",
        "retry_after",
    ] {
        if let Some(v) = md.get(key).and_then(|v| v.as_u64()) {
            return Some(v);
        }
    }
    if let Some(v) = md.get("retry_after_ms").and_then(|v| v.as_u64()) {
        return Some(v.div_ceil(1000));
    }
    md.get("headers")
        .and_then(|h| h.as_object())
        .and_then(|obj| {
            obj.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
                .and_then(|(_, v)| v.as_str()?.trim().parse::<u64>().ok())
        })
}

/// Provider context-length errors: a known `code` wins; otherwise a 400
/// whose message names the context window.
fn is_context_length(status: u16, err: Option<&ProviderError>, message: &str) -> bool {
    if let Some(code) = err.and_then(|e| e.code.as_ref()).and_then(|c| c.as_str())
        && matches!(
            code,
            "context_length_exceeded"
                | "context_window_exceeded"
                | "max_tokens_exceeded"
                | "model_max_length_exceeded"
        )
    {
        return true;
    }
    if status != 400 {
        return false;
    }
    let lower = message.to_ascii_lowercase();
    [
        "context length",
        "context window",
        "context_length",
        "maximum context",
        "too many tokens",
        "token limit",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}
