//! Provider-HTTP helpers shared by the protocol clients: the capped
//! error-body read, the reqwest → [`AiError`] transport mapping, and the
//! message truncation used when an error body is not the provider's
//! JSON envelope. Moved here in W4-05 so `openai/` and `anthropic/`
//! classify transport failures identically.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use futures_util::StreamExt;

use crate::ai::AiError;

/// Provider error bodies are small; cap the read at 64 KiB.
pub(crate) const ERROR_BODY_CAP: usize = 64 * 1024;

pub(crate) fn map_reqwest_error(e: reqwest::Error) -> AiError {
    if e.is_timeout() {
        AiError::Timeout
    } else {
        AiError::Transport(e.to_string())
    }
}

/// Read an error body under the cap; a body we cannot read maps to an
/// empty message rather than masking the HTTP status.
pub(crate) async fn read_capped(res: reqwest::Response, cap: usize) -> Vec<u8> {
    let mut body = Vec::new();
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        if body.len() + chunk.len() > cap {
            body.extend_from_slice(&chunk[..cap - body.len()]);
            break;
        }
        body.extend_from_slice(&chunk);
    }
    body
}

/// Elide a non-envelope error body to `cap` bytes on a char boundary.
pub(crate) fn truncate(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_string();
    }
    let mut end = cap;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}
