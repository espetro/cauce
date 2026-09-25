//! JSON shapes of the OpenAI-compatible wire protocol: what
//! [`OpenAiClient::chat_stream`](super::OpenAiClient::chat_stream)
//! serialises, what the stream and `/models` return, and the shared
//! `{"error": ...}` envelope. Only the fields cauce reads are modelled;
//! provider extensions are ignored.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use crate::ai::{ChatMessage, ToolSpec, Usage};

#[derive(serde::Serialize)]
pub(super) struct WireRequest<'a> {
    pub(super) model: &'a str,
    pub(super) stream: bool,
    pub(super) messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) tools: Vec<WireTool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_choice: Option<&'a serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f32>,
    pub(super) stream_options: WireStreamOptions,
}

#[derive(serde::Serialize)]
pub(super) struct WireTool<'a> {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) function: &'a ToolSpec,
}

#[derive(serde::Serialize)]
pub(super) struct WireStreamOptions {
    pub(super) include_usage: bool,
}

/// `chat.completion.chunk` — only the fields cauce reads; reasoning
/// deltas (`reasoning`, `reasoning_details`) and provider extensions are
/// ignored.
#[derive(serde::Deserialize)]
pub(super) struct Chunk {
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) model: Option<String>,
    #[serde(default)]
    pub(super) choices: Vec<ChunkChoice>,
    #[serde(default)]
    pub(super) usage: Option<Usage>,
    /// A mid-stream provider error arrives as `{"error": {...}}` with no
    /// `choices`.
    #[serde(default)]
    pub(super) error: Option<ProviderError>,
}

#[derive(serde::Deserialize)]
pub(super) struct ChunkChoice {
    #[serde(default)]
    pub(super) delta: ChunkDelta,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

#[derive(serde::Deserialize, Default)]
pub(super) struct ChunkDelta {
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(default)]
    pub(super) tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(serde::Deserialize)]
pub(super) struct ToolCallDelta {
    /// Slot this delta belongs to; providers carry it on every delta
    /// (absent treated as 0).
    #[serde(default)]
    pub(super) index: Option<u32>,
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) function: Option<FunctionDelta>,
}

#[derive(serde::Deserialize)]
pub(super) struct FunctionDelta {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) arguments: Option<String>,
}

/// `GET /models` envelope: `{"data": [{...}]}`.
#[derive(serde::Deserialize)]
pub(super) struct ModelsPage {
    pub(super) data: Vec<ModelEntry>,
}

#[derive(serde::Deserialize)]
pub(super) struct ModelEntry {
    pub(super) id: String,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) context_length: Option<u64>,
}

/// The OpenAI `{"error": {...}}` envelope (OpenRouter adds a `metadata`
/// object with retry hints).
#[derive(serde::Deserialize)]
pub(super) struct ErrorBody {
    pub(super) error: ProviderError,
}

#[derive(serde::Deserialize)]
pub(super) struct ProviderError {
    #[serde(default)]
    pub(super) message: Option<String>,
    /// Providers disagree: numeric HTTP-ish code (OpenRouter) or a
    /// string like `"rate_limit_exceeded"`/`"context_length_exceeded"`.
    #[serde(default)]
    pub(super) code: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) retry_after: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) retry_after_ms: Option<serde_json::Value>,
}
