//! JSON shapes of the Anthropic Messages API: what
//! [`AnthropicClient::chat_stream`](super::AnthropicClient::chat_stream)
//! serialises to `POST /v1/messages`, what the SSE stream returns, what
//! `GET /v1/models` lists, and the `{"type":"error","error":{...}}`
//! envelope. Only the fields cauce reads are modelled; thinking blocks,
//! citations and future event types degrade to ignored rather than a
//! parse error (the API adds event/block kinds over time).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use serde_json::Value;

use crate::ai::ChatMessage;

/// Request body of `POST /v1/messages`. Differences from the OpenAI
/// shape: `max_tokens` is required, the system prompt is the top-level
/// `system` field (there is no `system` message role), tools are flat
/// `{name, description, input_schema}`, and `tool_choice` is a
/// `{"type": ...}` object.
#[derive(serde::Serialize)]
pub(super) struct WireRequest<'a> {
    pub(super) model: &'a str,
    pub(super) stream: bool,
    /// Required by the API; `ChatRequest::max_tokens` or the client
    /// default.
    pub(super) max_tokens: u32,
    /// Joined `system` messages — omitted when empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) system: Option<String>,
    pub(super) messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) tools: Vec<WireTool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f32>,
}

/// One `messages[]` entry: `{"role": "user"|"assistant", "content":
/// [<blocks>]}`. `tool` results ride inside a `user` turn as
/// `tool_result` blocks — consecutive ones fold into one message.
#[derive(serde::Serialize)]
pub(super) struct WireMessage {
    pub(super) role: String,
    pub(super) content: Vec<WireBlock>,
}

/// One `content[]` block — the subset cauce emits: `text`, an
/// assistant `tool_use`, or a `tool_result` carrying one tool output.
#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum WireBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// `tools[]` entry: flat `{name, description, input_schema}` — no
/// `{"type":"function"}` wrapper; that's the OpenAI shape.
#[derive(serde::Serialize)]
pub(super) struct WireTool<'a> {
    pub(super) name: &'a str,
    pub(super) description: &'a str,
    pub(super) input_schema: &'a Value,
}

/// Fold [`ChatRequest::messages`](crate::ai::ChatRequest) into the
/// Messages-API shape: `system` texts join into the top-level field,
/// `tool` results group into the next `user` message as `tool_result`
/// blocks, assistant `tool_calls` become `tool_use` blocks (their
/// `arguments` JSON string re-parses into `input`).
///
/// Returns `(system, messages)`. Fails on tool-call `arguments` that
/// are not a JSON object — `input` is an object on the wire.
pub(super) fn wire_messages(
    messages: &[ChatMessage],
) -> Result<(Option<String>, Vec<WireMessage>), crate::ai::AiError> {
    let mut system: Vec<String> = Vec::new();
    let mut out: Vec<WireMessage> = Vec::new();
    // `tool_result` blocks awaiting their `user` message.
    let mut pending: Vec<WireBlock> = Vec::new();
    for m in messages {
        match m.role.as_str() {
            "system" => {
                if let Some(text) = &m.content {
                    system.push(text.clone());
                }
            }
            "tool" => {
                pending.push(WireBlock::ToolResult {
                    tool_use_id: m.tool_call_id.clone().unwrap_or_default(),
                    content: m.content.clone().unwrap_or_default(),
                });
            }
            role => {
                if !pending.is_empty() {
                    out.push(WireMessage {
                        role: "user".to_string(),
                        content: std::mem::take(&mut pending),
                    });
                }
                let mut content = Vec::new();
                if let Some(text) = m.content.as_ref().filter(|t| !t.is_empty()) {
                    content.push(WireBlock::Text { text: text.clone() });
                }
                if role == "assistant" {
                    for call in m.tool_calls.as_deref().unwrap_or_default() {
                        content.push(WireBlock::ToolUse {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            input: tool_input(&call.arguments)?,
                        });
                    }
                }
                out.push(WireMessage {
                    role: role.to_string(),
                    content,
                });
            }
        }
    }
    if !pending.is_empty() {
        out.push(WireMessage {
            role: "user".to_string(),
            content: pending,
        });
    }
    let system = if system.is_empty() {
        None
    } else {
        Some(system.join("\n\n"))
    };
    Ok((system, out))
}

/// `tool_use.input` must be a JSON object; an empty `arguments` string
/// means no arguments (`{}`), a non-object parse is a caller bug —
/// surfaced, not silently wrapped.
fn tool_input(arguments: &str) -> Result<Value, crate::ai::AiError> {
    if arguments.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    let value: Value = serde_json::from_str(arguments)
        .map_err(|e| crate::ai::AiError::Parse(format!("tool call arguments are not JSON: {e}")))?;
    if !value.is_object() {
        return Err(crate::ai::AiError::Parse(
            "tool call arguments must be a JSON object".to_string(),
        ));
    }
    Ok(value)
}

/// `ChatRequest.tool_choice` carries the OpenAI vocabulary
/// (`"auto"`, `"none"`, a `{"type":"function","function":{"name"}}` pin);
/// map it onto Anthropic's `{"type": "auto"|"any"|"none"|"tool"}`.
/// Values already in Anthropic shape pass through.
pub(super) fn map_tool_choice(choice: &Value) -> Value {
    match choice {
        Value::String(s) => match s.as_str() {
            "auto" => serde_json::json!({"type": "auto"}),
            "none" => serde_json::json!({"type": "none"}),
            // OpenAI's `"required"` forces *some* tool — Anthropic's
            // `any`.
            "any" | "required" => serde_json::json!({"type": "any"}),
            other => serde_json::json!({"type": other}),
        },
        Value::Object(map) => {
            let function_pin = map.get("type").and_then(Value::as_str) == Some("function");
            if function_pin
                && let Some(name) = map
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(Value::as_str)
            {
                return serde_json::json!({"type": "tool", "name": name});
            }
            choice.clone()
        }
        other => other.clone(),
    }
}

/// One `data:` payload, tagged on its own `type` field — which the API
/// guarantees matches the SSE `event:` name, so the `event:` line is
/// redundant and skipped.
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum StreamEvent {
    /// Carries the `Message` skeleton: id, model and `usage` with
    /// `input_tokens` (output arrives later, cumulative).
    MessageStart {
        message: MessageStart,
    },
    ContentBlockStart {
        index: u32,
        content_block: ContentBlock,
    },
    ContentBlockDelta {
        index: u32,
        delta: ContentDelta,
    },
    /// `index` isn't needed: nothing is folded at block end — the
    /// payload's other fields are ignored.
    ContentBlockStop,
    /// Top-level deltas to the Message: `delta.stop_reason` plus
    /// cumulative `usage.output_tokens`.
    MessageDelta {
        delta: StopDelta,
        #[serde(default)]
        usage: Option<WireUsage>,
    },
    /// Terminal event; Anthropic sends no `[DONE]` sentinel.
    MessageStop,
    /// Keepalive.
    Ping,
    /// Mid-stream failure, e.g. `{"type":"error","error":{"type":
    /// "overloaded_error","message":"Overloaded"}}`.
    Error {
        error: ApiError,
    },
    /// Event types added after this client was written — ignored, as
    /// the API versioning policy requires.
    #[serde(other)]
    Unsupported,
}

/// The `message` of `message_start` — only the fields folded into the
/// completion (`content` is always `[]` here).
#[derive(serde::Deserialize)]
pub(super) struct MessageStart {
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) model: Option<String>,
    #[serde(default)]
    pub(super) usage: Option<WireUsage>,
}

/// The `content_block` of `content_block_start` — `kind` is matched by
/// name (`"text"`, `"tool_use"`, `thinking`, …) so new block kinds
/// degrade to ignored.
#[derive(serde::Deserialize)]
pub(super) struct ContentBlock {
    #[serde(rename = "type")]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
}

/// `content_block_delta.delta`: `text_delta` chunks feed `Delta`
/// events; `input_json_delta` chunks are raw `tool_use.input` JSON
/// fragments accumulated until `content_block_stop`.
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ContentDelta {
    TextDelta {
        text: String,
    },
    InputJsonDelta {
        partial_json: String,
    },
    /// `thinking_delta`, `signature_delta`, `citations_delta`, …
    #[serde(other)]
    Other,
}

/// `message_delta.delta`.
#[derive(serde::Deserialize)]
pub(super) struct StopDelta {
    /// `end_turn` | `tool_use` | `max_tokens` | `stop_sequence` |
    /// `pause_turn` | `refusal`.
    #[serde(default)]
    pub(super) stop_reason: Option<String>,
}

/// Anthropic `usage`: `input_tokens` arrives on `message_start`;
/// `output_tokens` is cumulative on each `message_delta`.
#[derive(serde::Deserialize)]
pub(super) struct WireUsage {
    #[serde(default)]
    pub(super) input_tokens: Option<u64>,
    #[serde(default)]
    pub(super) output_tokens: Option<u64>,
}

/// The `error` object shared by non-2xx bodies
/// (`{"type":"error","error":{...}}`) and the `error` stream event.
#[derive(serde::Deserialize)]
pub(super) struct ApiError {
    /// `invalid_request_error` | `authentication_error` |
    /// `permission_error` | `not_found_error` | `request_too_large` |
    /// `rate_limit_error` | `api_error` | `overloaded_error` | …
    #[serde(rename = "type", default)]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) message: Option<String>,
}

/// The non-2xx / stream-error envelope.
#[derive(serde::Deserialize)]
pub(super) struct ErrorBody {
    pub(super) error: ApiError,
}

/// `GET /v1/models` envelope: `{"data": [{"type":"model","id",
/// "display_name","created_at"}], "has_more", "first_id", "last_id"}`.
#[derive(serde::Deserialize)]
pub(super) struct ModelsPage {
    pub(super) data: Vec<ModelEntry>,
}

#[derive(serde::Deserialize)]
pub(super) struct ModelEntry {
    pub(super) id: String,
    #[serde(default)]
    pub(super) display_name: Option<String>,
}
