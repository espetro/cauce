//! OpenAI-compatible provider client (W4-01): `POST
//! {base_url}/chat/completions` with `stream: true`, parsed as SSE and
//! fanned out to a channel of [`AiStreamEvent`]s — text deltas as they
//! arrive, then one [`ChatCompletion`] with the tool calls assembled
//! from `tool_calls` deltas and the stream's `usage` block. `GET
//! {base_url}/models` is served from a 60 s in-process cache.
//!
//! Typed errors ([`AiError`]) classify the failure modes the answer
//! loop reacts to: 401/403 → `Auth`, 429 → `RateLimited` (the
//! `Retry-After` header wins, then the OpenRouter-style
//! `error.metadata.retry_after_seconds`/`headers.Retry-After`
//! envelope), a provider context-length error → `ContextLength`,
//! timeouts → `Timeout`, the rest → `Provider`/`Transport`/`Parse`.
//!
//! Every chat call records `cauce_ai_requests_total{model,outcome}`,
//! `cauce_ai_tokens_total{model,kind}` and `cauce_ai_duration_ms`, and
//! — when the client was built with [`OpenAiClient::with_audit`] — an
//! `audit` row (`ai.provider_call`): model, tokens, ms, request_id. The
//! prompt text never leaves the request body. `/models` is a cached
//! discovery call and is deliberately uninstrumented.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};
use tokio::sync::mpsc;
use tracing::Instrument;
use tracing::{debug, info_span};
use url::Url;
use uuid::Uuid;

use crate::config::AiConfig;
use crate::metrics::Metrics;
use crate::store::{AuditRow, Store};

use super::{
    AiCallCtx, AiError, AiStreamEvent, ChatCompletion, ChatRequest, DEFAULT_ACTOR, ModelInfo,
    PROVIDER_CALL_ACTION, ToolCall, Usage,
};

/// `GET /models` listing cache TTL (W4-01 settled: 60 s).
const MODELS_CACHE_TTL: Duration = Duration::from_secs(60);

/// `(fetched_at, listing)` — cloned-out `Arc`s keep the lock short.
type ModelsCache = Arc<Mutex<Option<(Instant, Arc<Vec<ModelInfo>>)>>>;

/// `/models` responses are legitimately large (OpenRouter's full list is
/// ~1 MB); cap at 16 MiB so a misbehaving endpoint cannot exhaust memory.
const MODELS_BODY_CAP: usize = 16 * 1024 * 1024;

/// Provider error bodies are small; cap the read at 64 KiB.
const ERROR_BODY_CAP: usize = 64 * 1024;

/// OpenAI-compatible streaming client over `chat/completions` +
/// `/models`. Cheap to clone — the reqwest pool, models cache and audit
/// handle are shared.
#[derive(Clone)]
pub struct OpenAiClient {
    client: reqwest::Client,
    /// `AiConfig::base_url`, trailing slash trimmed.
    base_url: String,
    /// `AiConfig::model`, the default model for [`ChatRequest`]s that
    /// don't carry one.
    model: String,
    /// Provider host for audit `details.provider`.
    provider_host: String,
    models_cache: ModelsCache,
    models_ttl: Duration,
    metrics: Metrics,
    audit: Option<Arc<dyn Store>>,
}

impl OpenAiClient {
    /// Build the client from `[ai]` config. `api_key` is already
    /// interpolated at config load (`${env:...}` resolved); an empty key
    /// means no `Authorization` header (loopback providers, Bifrost
    /// without auth). Fails on a non-http(s) or unparsable `base_url`
    /// or a key that cannot be a header value.
    pub fn new(cfg: &AiConfig) -> Result<Self, AiError> {
        let base_url = cfg.base_url.trim_end_matches('/').to_string();
        let parsed = Url::parse(&base_url).map_err(|e| {
            AiError::Parse(format!("invalid [ai].base_url {:?}: {e}", cfg.base_url))
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(AiError::Parse(format!(
                "[ai].base_url {:?}: scheme must be http(s)",
                cfg.base_url
            )));
        }
        let provider_host = parsed.host_str().unwrap_or_default().to_string();

        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            HeaderValue::from_static(concat!("cauce/", env!("CARGO_PKG_VERSION"))),
        );
        if !cfg.api_key.is_empty() {
            let value = HeaderValue::from_str(&format!("Bearer {}", cfg.api_key))
                .map_err(|e| AiError::Parse(format!("invalid [ai].api_key header value: {e}")))?;
            headers.insert(AUTHORIZATION, value);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            // Provider API: a redirect means a misconfigured base_url, so
            // surface the 3xx as a typed error rather than following it.
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(4)
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .map_err(|e| AiError::Transport(format!("http client build failed: {e}")))?;

        Ok(Self {
            client,
            base_url,
            model: cfg.model.clone(),
            provider_host,
            models_cache: Arc::new(Mutex::new(None)),
            models_ttl: MODELS_CACHE_TTL,
            metrics: Metrics,
            audit: None,
        })
    }

    /// Attach the `Store` the audit row per provider call is written to.
    pub fn with_audit(mut self, store: Arc<dyn Store>) -> Self {
        self.audit = Some(store);
        self
    }

    /// Override the 60 s `/models` cache TTL (tests).
    pub fn with_models_ttl(mut self, ttl: Duration) -> Self {
        self.models_ttl = ttl;
        self
    }

    /// The `[ai].model` default.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// `GET {base_url}/models`, served from the 60 s cache while fresh.
    /// Errors are the same typed [`AiError`] set as chat calls.
    pub async fn models(&self, budget: Duration) -> Result<Arc<Vec<ModelInfo>>, AiError> {
        {
            let cache = self.models_cache.lock().unwrap();
            if let Some((fetched_at, models)) = cache.as_ref()
                && fetched_at.elapsed() < self.models_ttl
            {
                return Ok(models.clone());
            }
        }

        let res = self
            .client
            .get(format!("{}/models", self.base_url))
            .timeout(budget)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = res.status().as_u16();
        if !res.status().is_success() {
            let headers = res.headers().clone();
            let body = read_capped(res, ERROR_BODY_CAP).await;
            return Err(map_error(status, &headers, &body));
        }
        if let Some(len) = res.content_length()
            && len > MODELS_BODY_CAP as u64
        {
            return Err(AiError::Parse(format!(
                "/models body exceeds the {MODELS_BODY_CAP} byte cap ({len} bytes announced)"
            )));
        }
        let body = res.bytes().await.map_err(map_reqwest_error)?;
        if body.len() > MODELS_BODY_CAP {
            return Err(AiError::Parse(format!(
                "/models body exceeds the {MODELS_BODY_CAP} byte cap ({} bytes)",
                body.len()
            )));
        }
        let page: ModelsPage = serde_json::from_slice(&body)
            .map_err(|e| AiError::Parse(format!("/models decode failed: {e}")))?;
        let models = Arc::new(
            page.data
                .into_iter()
                .map(|e| ModelInfo {
                    id: e.id,
                    name: e.name,
                    context_length: e.context_length,
                })
                .collect::<Vec<_>>(),
        );
        *self.models_cache.lock().unwrap() = Some((Instant::now(), models.clone()));
        Ok(models)
    }

    /// `POST {base_url}/chat/completions` with `stream: true` +
    /// `stream_options.include_usage`. Returns immediately with the
    /// event channel; a spawned task runs the HTTP exchange, pushes
    /// [`AiStreamEvent::Delta`]s as content arrives, one terminal
    /// [`AiStreamEvent::Done`] or [`AiStreamEvent::Error`], then records
    /// metrics and the audit row. `budget` bounds the whole call —
    /// headers *and* the streamed body.
    ///
    /// Validation failures (no model, empty `messages`, unbuildable
    /// request) return `Err` synchronously; everything after connect is
    /// an in-band `Error` event, mirroring the pipeline's
    /// `StreamEvent::Error` convention.
    pub fn chat_stream(
        &self,
        req: &ChatRequest,
        budget: Duration,
        ctx: AiCallCtx,
    ) -> Result<mpsc::UnboundedReceiver<AiStreamEvent>, AiError> {
        let model = match req.model.as_deref().unwrap_or(&self.model) {
            "" => {
                return Err(AiError::Parse(
                    "no model configured: set [ai].model or ChatRequest.model".to_string(),
                ));
            }
            m => m.to_string(),
        };
        if req.messages.is_empty() {
            return Err(AiError::Parse("ChatRequest.messages is empty".to_string()));
        }

        let tools: Vec<WireTool<'_>> = req
            .tools
            .iter()
            .map(|spec| WireTool {
                kind: "function",
                function: spec,
            })
            .collect();
        let body = WireRequest {
            model: &model,
            stream: true,
            messages: &req.messages,
            tools,
            tool_choice: req.tool_choice.as_ref(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream_options: WireStreamOptions {
                include_usage: true,
            },
        };
        let payload = serde_json::to_vec(&body)
            .map_err(|e| AiError::Parse(format!("request serialise failed: {e}")))?;
        let request = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header(CONTENT_TYPE, "application/json")
            .timeout(budget)
            .body(payload)
            .build()
            .map_err(|e| AiError::Transport(format!("request build failed: {e}")))?;

        let (tx, rx) = mpsc::unbounded_channel();
        let pump = PumpCtx {
            client: self.client.clone(),
            model,
            provider_host: self.provider_host.clone(),
            metrics: self.metrics,
            audit: self.audit.clone(),
            actor: ctx.actor,
            request_id: ctx.request_id,
        };
        tokio::spawn(drive(pump, request, tx));
        Ok(rx)
    }
}

// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    stream: bool,
    messages: &'a [super::ChatMessage],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'a serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream_options: WireStreamOptions,
}

#[derive(serde::Serialize)]
struct WireTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: &'a super::ToolSpec,
}

#[derive(serde::Serialize)]
struct WireStreamOptions {
    include_usage: bool,
}

/// `chat.completion.chunk` — only the fields cauce reads; reasoning
/// deltas (`reasoning`, `reasoning_details`) and provider extensions are
/// ignored.
#[derive(serde::Deserialize)]
struct Chunk {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<Usage>,
    /// A mid-stream provider error arrives as `{"error": {...}}` with no
    /// `choices`.
    #[serde(default)]
    error: Option<ProviderError>,
}

#[derive(serde::Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    delta: ChunkDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(serde::Deserialize)]
struct ToolCallDelta {
    /// Slot this delta belongs to; providers carry it on every delta
    /// (absent treated as 0).
    #[serde(default)]
    index: Option<u32>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<FunctionDelta>,
}

#[derive(serde::Deserialize)]
struct FunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// `GET /models` envelope: `{"data": [{...}]}`.
#[derive(serde::Deserialize)]
struct ModelsPage {
    data: Vec<ModelEntry>,
}

#[derive(serde::Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    context_length: Option<u64>,
}

/// The OpenAI `{"error": {...}}` envelope (OpenRouter adds a `metadata`
/// object with retry hints).
#[derive(serde::Deserialize)]
struct ErrorBody {
    error: ProviderError,
}

#[derive(serde::Deserialize)]
struct ProviderError {
    #[serde(default)]
    message: Option<String>,
    /// Providers disagree: numeric HTTP-ish code (OpenRouter) or a
    /// string like `"rate_limit_exceeded"`/`"context_length_exceeded"`.
    #[serde(default)]
    code: Option<serde_json::Value>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    retry_after: Option<serde_json::Value>,
    #[serde(default)]
    retry_after_ms: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Pump: HTTP exchange -> SSE parse -> channel
// ---------------------------------------------------------------------------

/// Everything the spawned pump task needs once the request is built.
struct PumpCtx {
    client: reqwest::Client,
    model: String,
    provider_host: String,
    metrics: Metrics,
    audit: Option<Arc<dyn Store>>,
    actor: Option<String>,
    request_id: Option<Uuid>,
}

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
async fn drive(pump: PumpCtx, request: reqwest::Request, tx: mpsc::UnboundedSender<AiStreamEvent>) {
    let started = Instant::now();
    let span = info_span!(
        "ai_http",
        model = %pump.model,
        url = %request.url(),
        status = tracing::field::Empty,
        ms = tracing::field::Empty,
        outcome = tracing::field::Empty,
    );
    let (outcome, usage) = exchange(&pump, request, &tx, &span)
        .instrument(span.clone())
        .await;
    let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    span.record("ms", ms);
    span.record("outcome", outcome);
    pump.metrics
        .record_ai_request(&pump.model, outcome, started.elapsed(), usage);
    pump.write_audit(ms, outcome, usage).await;
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

fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
    while let Some((b, rest)) = bytes.split_first() {
        if !b.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    bytes
}

/// Pop one complete SSE event (terminated by a blank line) off `buf`.
fn extract_event(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
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

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

fn map_reqwest_error(e: reqwest::Error) -> AiError {
    if e.is_timeout() {
        AiError::Timeout
    } else {
        AiError::Transport(e.to_string())
    }
}

/// Non-2xx response → typed error: envelope message if the body is a
/// `{"error": ...}` object, else the truncated body text.
fn map_error(status: u16, headers: &HeaderMap, body: &[u8]) -> AiError {
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
fn map_stream_error(err: &ProviderError) -> AiError {
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

fn truncate(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_string();
    }
    let mut end = cap;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// Read an error body under the cap; a body we cannot read maps to an
/// empty message rather than masking the HTTP status.
async fn read_capped(res: reqwest::Response, cap: usize) -> Vec<u8> {
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

impl PumpCtx {
    /// Emit the `cauce.audit` event and write the `ai.provider_call`
    /// row — same emit-then-persist contract as cauce-server's
    /// `observability::audit`, minus the handler layer. The row carries
    /// model/tokens/ms/request_id; never the prompt.
    async fn write_audit(&self, ms: u64, outcome: &'static str, usage: Option<Usage>) {
        let Some(store) = &self.audit else {
            return;
        };
        let row = AuditRow {
            id: None,
            ts: Utc::now(),
            actor: self.actor.clone().unwrap_or_else(|| DEFAULT_ACTOR.into()),
            action: PROVIDER_CALL_ACTION.to_string(),
            target: self.model.clone(),
            details: serde_json::json!({
                "provider": self.provider_host,
                "tokens": usage.map(|u| serde_json::json!({
                    "prompt": u.prompt_tokens,
                    "completion": u.completion_tokens,
                    "total": u.total_tokens,
                })),
                "ms": ms,
                "outcome": outcome,
            }),
            request_id: self.request_id,
        };
        match row.request_id {
            Some(request_id) => tracing::info!(
                target: "cauce.audit",
                audit = true,
                actor = %row.actor,
                action = %row.action,
                audit_target = %row.target,
                request_id = %request_id,
                "audit"
            ),
            None => tracing::info!(
                target: "cauce.audit",
                audit = true,
                actor = %row.actor,
                action = %row.action,
                audit_target = %row.target,
                "audit"
            ),
        }
        if let Err(e) = store.audit(row).await {
            tracing::error!(
                target: "cauce.audit",
                audit = true,
                error = %e,
                "audit write failed"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests: recorded fixtures + wiremock
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ChatMessage, ToolSpec};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Real streamed completion ending in `finish_reason: "tool_calls"`;
    /// `function.arguments` arrives split across four delta chunks.
    const SSE_TOOLCALL: &str = include_str!("../../fixtures/ai/sse_toolcall.raw");
    /// Real streamed text completion ending in `finish_reason: "stop"`.
    const SSE_TEXT: &str = include_str!("../../fixtures/ai/sse_text.raw");
    /// Real `GET /models` (OpenAI `{"data":[...]}` format).
    const MODELS: &str = include_str!("../../fixtures/ai/models.json");
    /// Real OpenRouter-style 429 error envelope (retry hint lives in
    /// `error.metadata`, not an HTTP header).
    const ERR_429_ENVELOPE: &str = include_str!("../../fixtures/ai/err_429.json");
    /// Hand-authored: OpenAI's 429 body shape; the test response carries
    /// the standard `Retry-After` header.
    const ERR_429_BODY: &str = include_str!("../../fixtures/ai/err_429_retry_after.json");
    /// Hand-authored OpenAI auth error.
    const ERR_401: &str = include_str!("../../fixtures/ai/err_401.json");
    /// Hand-authored OpenAI context-length error.
    const ERR_CONTEXT_LENGTH: &str = include_str!("../../fixtures/ai/err_context_length.json");

    fn client_for(server: &MockServer, model: &str) -> OpenAiClient {
        OpenAiClient::new(&AiConfig {
            base_url: server.uri(),
            api_key: "sk-test".to_string(),
            model: model.to_string(),
            enabled: true,
        })
        .unwrap()
    }

    fn request(model: Option<&str>) -> ChatRequest {
        ChatRequest {
            model: model.map(str::to_string),
            messages: vec![
                ChatMessage::system("test system prompt"),
                ChatMessage::user("test question"),
            ],
            ..ChatRequest::default()
        }
    }

    async fn collect(rx: mpsc::UnboundedReceiver<AiStreamEvent>) -> Vec<AiStreamEvent> {
        let mut rx = rx;
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    }

    async fn mount_completion(server: &MockServer, body: &'static str) {
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(body.as_bytes(), "text/event-stream"),
            )
            .mount(server)
            .await;
    }

    /// Acceptance: the recorded stream's `search_web` call assembles
    /// across its four argument deltas.
    #[tokio::test]
    async fn tool_call_deltas_assemble_across_chunks() {
        let server = MockServer::start().await;
        mount_completion(&server, SSE_TOOLCALL).await;
        let client = client_for(&server, "test-openai-toolcall");

        let rx = client
            .chat_stream(
                &request(None),
                Duration::from_secs(10),
                AiCallCtx::default(),
            )
            .unwrap();
        let events = collect(rx).await;

        let done = events.iter().find_map(|e| match e {
            AiStreamEvent::Done(c) => Some(c),
            _ => None,
        });
        let completion = done.expect("stream ended with Done");
        assert_eq!(completion.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(completion.tool_calls.len(), 1);
        let call = &completion.tool_calls[0];
        assert_eq!(call.id, "chatcmpl-tool-b0d6032a45b725d0");
        assert_eq!(call.name, "search_web");
        let args: serde_json::Value =
            serde_json::from_str(&call.arguments).expect("assembled arguments are valid JSON");
        assert_eq!(args["query"], "current weather in Tokyo right now");
        let usage = completion.usage.expect("stream carried usage");
        assert_eq!(usage.prompt_tokens, 102);
        assert_eq!(usage.completion_tokens, 68);
        assert_eq!(usage.total_tokens, 170);
    }

    /// The recorded text stream: content deltas then a `Done` with
    /// `stop` + usage.
    #[tokio::test]
    async fn text_completion_streams_deltas_then_done() {
        let server = MockServer::start().await;
        mount_completion(&server, SSE_TEXT).await;
        let client = client_for(&server, "test-openai-text");

        let rx = client
            .chat_stream(
                &request(None),
                Duration::from_secs(10),
                AiCallCtx::default(),
            )
            .unwrap();
        let events = collect(rx).await;

        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                AiStreamEvent::Delta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas.concat(), "hello world");
        let completion = match events.last() {
            Some(AiStreamEvent::Done(c)) => c,
            other => panic!("expected terminal Done, got {other:?}"),
        };
        assert_eq!(completion.content, "hello world");
        assert_eq!(completion.finish_reason.as_deref(), Some("stop"));
        assert!(completion.tool_calls.is_empty());
        let usage = completion.usage.expect("stream carried usage");
        assert_eq!(usage.prompt_tokens, 16);
        assert_eq!(usage.completion_tokens, 75);
        assert_eq!(usage.total_tokens, 91);
    }

    /// Acceptance: a 429 with the standard `Retry-After` header surfaces
    /// `retry_after_s`.
    #[tokio::test]
    async fn rate_limit_surfaces_retry_after_s() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "30")
                    .set_body_string(ERR_429_BODY),
            )
            .mount(&server)
            .await;
        let client = client_for(&server, "test-openai-429");

        let rx = client
            .chat_stream(
                &request(None),
                Duration::from_secs(10),
                AiCallCtx::default(),
            )
            .unwrap();
        let events = collect(rx).await;

        match events.as_slice() {
            [AiStreamEvent::Error(AiError::RateLimited { retry_after_s })] => {
                assert_eq!(*retry_after_s, Some(30));
            }
            other => panic!("expected one RateLimited error, got {other:?}"),
        }
    }

    /// The recorded OpenRouter envelope carries the retry hint in
    /// `error.metadata` (no `Retry-After` header) — it still surfaces.
    #[tokio::test]
    async fn rate_limit_reads_envelope_metadata() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string(ERR_429_ENVELOPE))
            .mount(&server)
            .await;
        let client = client_for(&server, "test-openai-429-meta");

        let rx = client
            .chat_stream(
                &request(None),
                Duration::from_secs(10),
                AiCallCtx::default(),
            )
            .unwrap();
        let events = collect(rx).await;

        match events.as_slice() {
            [AiStreamEvent::Error(AiError::RateLimited { retry_after_s })] => {
                assert_eq!(*retry_after_s, Some(14));
            }
            other => panic!("expected one RateLimited error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn auth_error_is_typed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string(ERR_401))
            .mount(&server)
            .await;
        let client = client_for(&server, "test-openai-401");

        let rx = client
            .chat_stream(
                &request(None),
                Duration::from_secs(10),
                AiCallCtx::default(),
            )
            .unwrap();
        let events = collect(rx).await;

        match events.as_slice() {
            [AiStreamEvent::Error(AiError::Auth(message))] => {
                assert!(message.contains("API key"), "{message}");
            }
            other => panic!("expected one Auth error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn context_length_error_is_typed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_string(ERR_CONTEXT_LENGTH))
            .mount(&server)
            .await;
        let client = client_for(&server, "test-openai-ctx");

        let rx = client
            .chat_stream(
                &request(None),
                Duration::from_secs(10),
                AiCallCtx::default(),
            )
            .unwrap();
        let events = collect(rx).await;

        match events.as_slice() {
            [AiStreamEvent::Error(AiError::ContextLength(message))] => {
                assert!(message.contains("context length"), "{message}");
            }
            other => panic!("expected one ContextLength error, got {other:?}"),
        }
    }

    /// `GET /models` parses the OpenAI `{"data": [...]}` shape and the
    /// second call inside the TTL is served from cache (one upstream
    /// request total).
    #[tokio::test]
    async fn models_listing_parses_and_caches() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_string(MODELS))
            .mount(&server)
            .await;
        let client = client_for(&server, "test-openai-models");

        let first = client.models(Duration::from_secs(10)).await.unwrap();
        assert!(
            first.iter().any(|m| m.id == "fireworks/ember-1"),
            "fixture models parsed"
        );
        let second = client.models(Duration::from_secs(10)).await.unwrap();
        assert_eq!(first.len(), second.len());

        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1, "cached /models hit the wire again: {seen:?}");
    }

    /// TTL expiry refetches.
    #[tokio::test]
    async fn models_cache_expires() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_string(MODELS))
            .mount(&server)
            .await;
        let client = client_for(&server, "test-openai-models-ttl").with_models_ttl(Duration::ZERO);

        client.models(Duration::from_secs(10)).await.unwrap();
        client.models(Duration::from_secs(10)).await.unwrap();

        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 2, "expired cache must refetch");
    }

    /// The request body is the OpenAI shape: stream + include_usage,
    /// auth header, no tools block when empty.
    #[tokio::test]
    async fn request_body_is_openai_shape() {
        let server = MockServer::start().await;
        mount_completion(&server, SSE_TEXT).await;
        let client = client_for(&server, "test-openai-wire");

        let rx = client
            .chat_stream(
                &ChatRequest {
                    model: Some("test-openai-wire".to_string()),
                    messages: vec![ChatMessage::user("hello")],
                    tools: vec![ToolSpec {
                        name: "search_web".to_string(),
                        description: "Search the web".to_string(),
                        parameters: serde_json::json!({"type": "object"}),
                    }],
                    tool_choice: Some(serde_json::json!("auto")),
                    max_tokens: Some(64),
                    temperature: Some(0.2),
                },
                Duration::from_secs(10),
                AiCallCtx::default(),
            )
            .unwrap();
        let _ = collect(rx).await;

        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1);
        let req = &seen[0];
        assert_eq!(
            req.headers
                .get("authorization")
                .and_then(|v| v.to_str().ok()),
            Some("Bearer sk-test")
        );
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["model"], "test-openai-wire");
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "search_web");
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["max_tokens"], 64);
    }

    #[tokio::test]
    async fn empty_model_and_messages_rejected() {
        let server = MockServer::start().await;
        let client = OpenAiClient::new(&AiConfig {
            base_url: server.uri(),
            api_key: String::new(),
            model: String::new(),
            enabled: true,
        })
        .unwrap();
        assert!(matches!(
            client.chat_stream(&request(None), Duration::from_secs(1), AiCallCtx::default()),
            Err(AiError::Parse(_))
        ));
        let client = client_for(&server, "test-model");
        assert!(matches!(
            client.chat_stream(
                &ChatRequest::default(),
                Duration::from_secs(1),
                AiCallCtx::default()
            ),
            Err(AiError::Parse(_))
        ));
    }
}
