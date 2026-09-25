//! SSE framing shared by the protocol pumps: both
//! [`openai`](super::openai) and [`anthropic`](super::anthropic) read
//! `data:` payloads off blank-line-delimited events over a chunked byte
//! stream. Moved here in W4-05 so the two pumps frame identically.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

pub(crate) fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

pub(crate) fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
    while let Some((b, rest)) = bytes.split_first() {
        if !b.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    bytes
}

/// Pop one complete SSE event (terminated by a blank line) off `buf`.
pub(crate) fn extract_event(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    for i in 0..buf.len() {
        if buf[i] != b'\n' {
            continue;
        }
        let end = match (buf.get(i + 1), buf.get(i + 2)) {
            (Some(b'\n'), _) => i + 2,
            (Some(b'\r'), Some(b'\n')) => i + 3,
            _ => continue,
        };
        let event = buf[..i].to_vec();
        buf.drain(..end);
        return Some(event);
    }
    None
}
