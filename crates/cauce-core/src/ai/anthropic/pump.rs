//! The spawned pump behind [`AnthropicClient::chat_stream`]: run the
//! `POST /v1/messages` exchange inside an `ai_http` span, push
//! [`AiStreamEvent`]s as SSE events parse, then record metrics and the
//! audit row exactly once — whether the call ended ok, in error, or by
//! the receiver going away. Structurally identical to the OpenAI pump;
//! only the event grammar differs (`message_start` / `content_block_*`
//! / `message_delta` / `message_stop` instead of chunk `choices` +
//! `[DONE]`).
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
use super::wire::{ContentDelta, StreamEvent};

/// Stream state folded across SSE events.
#[derive(Default)]
struct CompletionAcc {
    id: Option<String>,
    model: Option<String>,
    content: String,
    /// `tool_use` blocks keyed by their `index` slot.
    tool_calls: BTreeMap<u32, ToolCallAcc>,
    finish_reason: Option<String>,
    /// `usage.input_tokens` (arrives on `message_start`).
    input_tokens: Option<u64>,
    /// `usage.output_tokens` — cumulative on each `message_delta`.
    output_tokens: Option<u64>,
}

#[derive(Default)]
struct ToolCallAcc {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl CompletionAcc {
    /// The stream's token counts folded into the shared [`Usage`]
    /// shape: `input_tokens` → prompt, `output_tokens` → completion.
    fn usage(&self) -> Option<Usage> {
        if self.input_tokens.is_none() && self.output_tokens.is_none() {
            return None;
        }
        let prompt_tokens = self.input_tokens.unwrap_or(0);
        let completion_tokens = self.output_tokens.unwrap_or(0);
        Some(Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        })
    }

    fn into_completion(self) -> ChatCompletion {
        let usage = self.usage();
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
            usage,
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
            return ("abandoned", acc.usage());
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
                Ok(true) => break 'outer, // message_stop
                Ok(false) => {}
                Err(e) => {
                    failure = Some(e);
                    break 'outer;
                }
            }
        }
    }

    // A provider that closes without `message_stop` still has its
    // trailing event parsed — SSE does not require the sentinel.
    if failure.is_none() && !buf.iter().all(|b| b.is_ascii_whitespace()) {
        match handle_event(&buf, &mut acc, tx) {
            Ok(_) => {}
            Err(e) => failure = Some(e),
        }
    }

    if let Some(err) = failure {
        let label = err.outcome_label();
        let _ = tx.send(AiStreamEvent::Error(err));
        return (label, acc.usage());
    }
    let completion = acc.into_completion();
    let usage = completion.usage;
    let _ = tx.send(AiStreamEvent::Done(Box::new(completion)));
    ("ok", usage)
}

/// One SSE event: take each `data:` payload (the spec allows several
/// per event), dispatch on the payload's own `type` — which the API
/// guarantees matches the `event:` name, so `event:`/`id:`/`retry:`
/// lines and `:` comments are skipped. `Ok(true)` = `message_stop`
/// seen. A failed send means the receiver is gone — report done and
/// let the caller unwind.
fn handle_event(
    event: &[u8],
    acc: &mut CompletionAcc,
    tx: &mpsc::UnboundedSender<AiStreamEvent>,
) -> Result<bool, AiError> {
    for line in event.split(|b| *b == b'\n') {
        let line = trim_cr(line);
        let Some(payload) = line.strip_prefix(b"data:") else {
            continue;
        };
        let payload = trim_ascii_start(payload);
        let event: StreamEvent = serde_json::from_slice(payload).map_err(|e| {
            AiError::Parse(format!(
                "stream event is not a Messages API event: {e} ({})",
                String::from_utf8_lossy(&payload[..payload.len().min(200)])
            ))
        })?;
        match event {
            StreamEvent::MessageStart { message } => {
                if message.id.is_some() {
                    acc.id = message.id;
                }
                if message.model.is_some() {
                    acc.model = message.model;
                }
                if let Some(usage) = message.usage {
                    if let Some(input) = usage.input_tokens {
                        acc.input_tokens = Some(input);
                    }
                    if let Some(output) = usage.output_tokens {
                        acc.output_tokens = Some(output);
                    }
                }
            }
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                if content_block.kind == "tool_use" {
                    let call = acc.tool_calls.entry(index).or_default();
                    if content_block.id.is_some() {
                        call.id = content_block.id;
                    }
                    if content_block.name.is_some() {
                        call.name = content_block.name;
                    }
                }
            }
            StreamEvent::ContentBlockDelta { index, delta } => match delta {
                ContentDelta::TextDelta { text } => {
                    if !text.is_empty() {
                        acc.content.push_str(&text);
                        if tx.send(AiStreamEvent::Delta(text)).is_err() {
                            return Ok(true);
                        }
                    }
                }
                ContentDelta::InputJsonDelta { partial_json } => {
                    acc.tool_calls
                        .entry(index)
                        .or_default()
                        .arguments
                        .push_str(&partial_json);
                }
                ContentDelta::Other => {}
            },
            StreamEvent::ContentBlockStop => {}
            StreamEvent::MessageDelta { delta, usage } => {
                if let Some(reason) = delta.stop_reason {
                    acc.finish_reason = Some(map_stop_reason(&reason).to_string());
                }
                if let Some(usage) = usage {
                    if let Some(input) = usage.input_tokens {
                        acc.input_tokens = Some(input);
                    }
                    if let Some(output) = usage.output_tokens {
                        acc.output_tokens = Some(output);
                    }
                }
            }
            StreamEvent::MessageStop => return Ok(true),
            StreamEvent::Ping | StreamEvent::Unsupported => {}
            StreamEvent::Error { error } => return Err(map_stream_error(&error)),
        }
    }
    Ok(false)
}

/// `stop_reason` onto the shared finish vocabulary
/// ([`ChatCompletion::finish_reason`]: `"stop"` / `"tool_calls"` /
/// `"length"`); unfamiliar reasons (`pause_turn`, `refusal`, …) pass
/// through verbatim.
fn map_stop_reason(reason: &str) -> &str {
    match reason {
        "tool_use" => "tool_calls",
        "max_tokens" => "length",
        "end_turn" | "stop_sequence" => "stop",
        other => other,
    }
}
