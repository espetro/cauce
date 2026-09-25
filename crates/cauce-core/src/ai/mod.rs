//! AI provider surface (W4, parent plan section 4.5 and
//! `.agents/plans/v3/wave-4-ai-mode.md`): the protocol clients the
//! grounded-answer tool loop (W4-02) streams through.
//!
//! Module map (W4-01): `mod.rs` holds the wire-agnostic types every
//! protocol shares — [`ChatMessage`]/[`ToolSpec`]/[`ChatRequest`] in,
//! [`AiStreamEvent`] deltas plus the assembled [`ChatCompletion`] out,
//! the [`Usage`] token counts, [`AiCallCtx`] audit context and the
//! typed [`AiError`]. [`openai`] is the OpenAI-compatible client
//! (`POST {base_url}/chat/completions` streamed over SSE, plus a
//! 60 s-cached `GET {base_url}/models`); W4-05's Anthropic protocol maps
//! onto the same types.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

pub mod openai;

use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

pub use openai::OpenAiClient;

/// Fallback `audit.actor` when a provider call has no inbound request
/// context (`ui | api | mcp:<client> | cli` is the convention
/// [`crate::ClientKind::label`] produces).
pub const DEFAULT_ACTOR: &str = "ai";

/// Audit action of one provider call (the `audit` table's `action`
/// column, beside `cache.delete`, `config.put`, `engine.reset`).
pub const PROVIDER_CALL_ACTION: &str = "ai.provider_call";

/// Per-call context for metrics and the audit row. Carried separately
/// from [`ChatRequest`] so the same request shape serves every surface
/// (`/api/answer`, MCP, `cauce eval`).
#[derive(Debug, Clone, Default)]
pub struct AiCallCtx {
    /// `audit.actor` — the inbound surface's actor label (`ui | api |
    /// mcp:<client> | cli`, plus any `X-Actor` override). Falls back to
    /// [`DEFAULT_ACTOR`] when absent.
    pub actor: Option<String>,
    /// The inbound request's id; stamped on the audit row's
    /// `request_id` column like every other audited action.
    pub request_id: Option<Uuid>,
}

/// Typed provider failure (W4-01 "typed errors"): the variants the
/// answer loop reacts to differently — retry-after on rate limits,
/// auth misconfiguration, an overlong prompt — versus opaque
/// transport/parse noise.
#[derive(Debug, Error)]
pub enum AiError {
    /// 401/403: bad or missing API key.
    #[error("provider authentication failed: {0}")]
    Auth(String),
    /// 429: `retry_after_s` is the provider's hint when sent
    /// (`Retry-After` header or error metadata), `None` otherwise.
    #[error("provider rate limited (retry after {retry_after_s:?}s)")]
    RateLimited { retry_after_s: Option<u64> },
    /// The prompt does not fit the model's context window.
    #[error("request exceeds the model context window: {0}")]
    ContextLength(String),
    /// Any other non-2xx status, with the provider's message.
    #[error("provider returned status {status}: {message}")]
    Provider { status: u16, message: String },
    /// Request budget elapsed.
    #[error("provider request timed out")]
    Timeout,
    /// Connection-level failure (DNS, TLS, reset mid-stream).
    #[error("provider transport error: {0}")]
    Transport(String),
    /// Well-formed transport carrying malformed JSON/SSE.
    #[error("malformed provider response: {0}")]
    Parse(String),
}

impl AiError {
    /// `outcome` label of `cauce_ai_requests_total{model,outcome}`.
    pub fn outcome_label(&self) -> &'static str {
        match self {
            Self::Auth(_) => "auth",
            Self::RateLimited { .. } => "rate_limited",
            Self::ContextLength(_) => "context_length",
            Self::Provider { .. } => "provider",
            Self::Timeout => "timeout",
            Self::Transport(_) => "transport",
            Self::Parse(_) => "parse",
        }
    }
}

/// `usage` block of a chat completion (OpenAI shape, also sent by
/// OpenRouter/Bifrost with `stream_options.include_usage`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

/// One message of a chat request. Serialises to the OpenAI shape:
/// `{"role","content"}` for system/user/assistant text, plus
/// `tool_calls` on assistant turns and `tool_call_id` on `tool` results
/// (the W4-02 loop echoes both back).
#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    fn text(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::text("system", content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::text("user", content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text("assistant", content)
    }

    /// The assistant turn that requested `calls` — echoed back verbatim
    /// before the tool results in the tool loop.
    pub fn assistant_tool_calls(calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(calls),
            tool_call_id: None,
            name: None,
        }
    }

    /// One tool's output (`role: "tool"`), matched to its call by id.
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(call_id.into()),
            name: None,
        }
    }
}

/// A tool the model may call, serialised inside the request's
/// `{"type":"function","function":{name,description,parameters}}`
/// wrapper (the client adds the wrapper).
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the `arguments` object.
    pub parameters: serde_json::Value,
}

/// One tool call, assembled from `tool_calls` deltas over the stream.
/// Serialises to the message shape
/// `{"id","type":"function","function":{"name","arguments"}}` so an
/// assistant turn can be echoed back unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    /// `function.name`.
    pub name: String,
    /// `function.arguments` verbatim — a JSON document once complete.
    pub arguments: String,
}

impl Serialize for ToolCall {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Function<'a> {
            name: &'a str,
            arguments: &'a str,
        }
        #[derive(Serialize)]
        struct Wire<'a> {
            id: &'a str,
            #[serde(rename = "type")]
            kind: &'static str,
            function: Function<'a>,
        }
        Wire {
            id: &self.id,
            kind: "function",
            function: Function {
                name: &self.name,
                arguments: &self.arguments,
            },
        }
        .serialize(serializer)
    }
}

/// One chat completion request. `stream: true` and
/// `stream_options.include_usage` are always sent — the client is
/// streaming-only by contract.
#[derive(Debug, Default)]
pub struct ChatRequest {
    /// Model override; `[ai].model` when `None`.
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    /// Tools offered this turn (`tool_choice: "auto"` unless set).
    pub tools: Vec<ToolSpec>,
    /// Raw OpenAI `tool_choice` value (`"auto"`, `"none"`, a
    /// `{"type":"function",...}` pin); absent = provider default.
    pub tool_choice: Option<serde_json::Value>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

/// One entry of `GET {base_url}/models`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub name: Option<String>,
    pub context_length: Option<u64>,
}

/// A completed streamed turn: every delta folded together.
#[derive(Debug, Clone)]
pub struct ChatCompletion {
    /// Provider generation id (`gen-...`, `chatcmpl-...`).
    pub id: Option<String>,
    /// Model the provider reports it served (may differ from requested).
    pub model: Option<String>,
    /// Concatenated `content` deltas — `""` on a tool-calls-only turn.
    pub content: String,
    /// Tool calls assembled from `tool_calls` deltas, in `index` order.
    pub tool_calls: Vec<ToolCall>,
    /// Last non-null `finish_reason` (`"stop"`, `"tool_calls"`, `"length"`).
    pub finish_reason: Option<String>,
    /// The stream's `usage` block when the provider sent one.
    pub usage: Option<Usage>,
}

/// One item of a [`OpenAiClient::chat_stream`] channel — mirrors the
/// pipeline's `StreamEvent` convention (events are values, errors ride
/// in-band as a terminal [`AiStreamEvent::Error`]).
#[derive(Debug)]
pub enum AiStreamEvent {
    /// A `content` delta as it arrived.
    Delta(String),
    /// Stream closed cleanly; the fully assembled turn. Boxed so
    /// `Delta` stays cheap — it is the common variant.
    Done(Box<ChatCompletion>),
    /// The call failed — a non-2xx status, a mid-stream `{"error":...}`
    /// chunk, or a transport/parse failure. Terminal: nothing follows.
    Error(AiError),
}
