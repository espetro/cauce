//! The spawned pump behind [`OpenAiClient::chat_stream`]: run the HTTP
//! exchange inside an `ai_http` span, push [`AiStreamEvent`]s as chunks
//! parse, then record metrics and the audit row exactly once — whether
//! the call ended ok, in error, or by the receiver going away.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::time::Instant;

use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::Instrument;
use tracing::debug;

use crate::ai::http::{ERROR_BODY_CAP, map_reqwest_error, read_capped};
use crate::ai::pump::PumpCtx;
use crate::ai::sse::{extract_event, trim_ascii_start, trim_cr};
use crate::ai::{AiError, AiStreamEvent, ChatCompletion, ToolCall, Usage};

use super::errors::{map_error, map_stream_error};
use super::wire::{Chunk, ToolCallDelta};

/// Stream state folded across chunks.
#[derive(Default)]
struct CompletionAcc {
    id: Option<String>,
    model: Option<String>,
    content: String,
    /// Tool calls keyed by their `index` slot.
    tool_calls: BTreeMap<u32, ToolCallAcc>,
    finish_reason: Option<String>,
    usage: Option<Usage>,
}

#[derive(Default)]
struct ToolCallAcc {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl CompletionAcc {
    fn merge_tool_call(&mut self, delta: ToolCallDelta) {
        let acc = self.tool_calls.entry(delta.index.unwrap_or(0)).or_default();
        if let Some(id) = delta.id {
            acc.id = Some(id);
        }
        if let Some(function) = delta.function {
            if let Some(name) = function.name {
                acc.name = Some(name);
            }
            if let Some(arguments) = function.arguments {
                acc.arguments.push_str(&arguments);
            }
        }
    }

    fn into_completion(self) -> ChatCompletion {
        ChatCompletion {
            id: self.id,
            model: self.model,
            content: self.content,
            tool_calls: self
                .tool_calls
                .into_values()
                .map(|acc| ToolCall {
                    id: acc.id.unwrap_or_default(),
                    name: acc.name.unwrap_or_default(),
                    arguments: acc.arguments,
                })
                .collect(),
            finish_reason: self.finish_reason,
            usage: self.usage,
        }
    }
}

/// The spawned task: run the request, push events, then record metrics
/// and the audit row exactly once (any outcome, cancelled receivers
/// included).
pub(super) async fn drive(
    pump: PumpCtx,
    request: reqwest::Request,
    tx: mpsc::UnboundedSender<AiStreamEvent>,
) {
    let started = Instant::now();
    let span = pump.span(request.url());
    let (outcome, usage) = exchange(&pump, request, &tx, &span)
        .instrument(span.clone())
        .await;
    pump.record(started, &span, outcome, usage).await;
}

/// The HTTP exchange itself; returns the `outcome` label and usage for
/// the recorder. Sends at most one terminal event (`Done` or `Error`).
async fn exchange(
    pump: &PumpCtx,
    request: reqwest::Request,
    tx: &mpsc::UnboundedSender<AiStreamEvent>,
    span: &tracing::Span,
) -> (&'static str, Option<Usage>) {
    let res = match pump.client.execute(request).await {
        Ok(res) => res,
        Err(e) => {
            let err = map_reqwest_error(e);
            let label = err.outcome_label();
            let _ = tx.send(AiStreamEvent::Error(err));
            return (label, None);
        }
    };
    let status = res.status().as_u16();
    span.record("status", status);
    if !res.status().is_success() {
        let headers = res.headers().clone();
        let body = read_capped(res, ERROR_BODY_CAP).await;
        let err = map_error(status, &headers, &body);
        let label = err.outcome_label();
        debug!(status, error = %err, "provider call failed");
        let _ = tx.send(AiStreamEvent::Error(err));
        return (label, None);
    }

    let mut acc = CompletionAcc::default();
    let mut buf: Vec<u8> = Vec::with_capacity(16 * 1024);
    let mut stream = res.bytes_stream();
    let mut failure: Option<AiError> = None;

    'outer: while let Some(chunk) = stream.next().await {
        // Receiver dropped: stop talking to the provider. The audit row
        // and metrics still record the call, outcome "abandoned".
        if tx.is_closed() {
            return ("abandoned", acc.usage);
        }
        match chunk {
            Ok(bytes) => buf.extend_from_slice(&bytes),
            Err(e) => {
                failure = Some(map_reqwest_error(e));
                break;
            }
        }
        while let Some(event) = extract_event(&mut buf) {
            match handle_event(&event, &mut acc, tx) {
                Ok(true) => break 'outer, // [DONE]
                Ok(false) => {}
                Err(e) => {
                    failure = Some(e);
                    break 'outer;
                }
            }
        }
    }

    // A provider that closes without [DONE] still has its trailing
    // event parsed — SSE does not require the sentinel.
    if failure.is_none() && !buf.iter().all(|b| b.is_ascii_whitespace()) {
        match handle_event(&buf, &mut acc, tx) {
            Ok(_) => {}
            Err(e) => failure = Some(e),
        }
    }

    if let Some(err) = failure {
        let label = err.outcome_label();
        let _ = tx.send(AiStreamEvent::Error(err));
        return (label, acc.usage);
    }
    let completion = acc.into_completion();
    let usage = completion.usage;
    let _ = tx.send(AiStreamEvent::Done(Box::new(completion)));
    ("ok", usage)
}

/// One SSE event: join its `data:` lines (spec allows several per
/// event), dispatch each payload. `Ok(true)` = `[DONE]` seen. A failed
/// send means the receiver is gone — report done and let the caller
/// unwind.
fn handle_event(
    event: &[u8],
    acc: &mut CompletionAcc,
    tx: &mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<bool, AiError> {
    for line in event.split(|b| *b == b'\n') {
        let line = trim_cr(line);
        let Some(payload) = line.strip_prefix(b"data:") else {
            // `event:`/`id:`/`retry:` fields and `:` comments carry
            // nothing for chat completions.
            continue;
        };
        let payload = trim_ascii_start(payload);
        if payload == b"[DONE]" {
            return Ok(true);
        }
        let chunk: Chunk = serde_json::from_slice(payload).map_err(|e| {
            AiError::Parse(format!(
                "stream chunk is not a completion: {e} ({})",
                String::from_utf8_lossy(&payload[..payload.len().min(200)])
            ))
        })?;
        if let Some(err) = chunk.error {
            return Err(map_stream_error(&err));
        }
        if chunk.id.is_some() {
            acc.id = chunk.id;
        }
        if chunk.model.is_some() {
            acc.model = chunk.model;
        }
        for choice in chunk.choices {
            if let Some(text) = choice.delta.content.filter(|s| !s.is_empty()) {
                acc.content.push_str(&text);
                if tx.send(AiStreamEvent::Delta(text)).is_err() {
                    return Ok(true);
                }
            }
            for call in choice.delta.tool_calls.unwrap_or_default() {
                acc.merge_tool_call(call);
            }
            if let Some(reason) = choice.finish_reason {
                acc.finish_reason = Some(reason);
            }
        }
        if let Some(usage) = chunk.usage {
            acc.usage = Some(usage);
        }
    }
    Ok(false)
}
