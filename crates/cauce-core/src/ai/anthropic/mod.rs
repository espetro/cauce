//! Anthropic Messages API provider client (W4-05): `POST
//! {base_url}/v1/messages` with `stream: true`, parsed as SSE and
//! fanned out to a channel of [`AiStreamEvent`]s — `text_delta`s as
//! they arrive, `input_json_delta` fragments folded into [`ToolCall`]s,
//! one terminal [`ChatCompletion`]. `GET {base_url}/v1/models` is
//! served from a 60 s in-process cache, same convention as
//! [`OpenAiClient`](super::OpenAiClient).
//!
//! Module map: `mod.rs` is the public [`AnthropicClient`] —
//! construction, `/v1/models` and `chat_stream`. [`wire`] holds the
//! request/stream JSON shapes and the [`ChatMessage`] → Messages-API
//! mapping (system texts join into the top-level `system` field,
//! assistant `tool_calls` become `tool_use` blocks, `tool` results ride
//! back inside a `user` turn as `tool_result` blocks). [`pump`] is the
//! spawned task that runs the HTTP exchange, parses SSE and folds
//! events into a [`ChatCompletion`], then records metrics and the audit
//! row; [`errors`] classifies provider failures into [`AiError`].
//!
//! Typed errors classify the failure modes the answer loop reacts to:
//! 401/403 or `authentication_error`/`permission_error` → `Auth`, 429
//! `rate_limit_error` and 529 `overloaded_error` → `RateLimited` (the
//! `Retry-After` header is the only hint source), a provider
//! context-length error → `ContextLength`, timeouts → `Timeout`, the
//! rest → `Provider`/`Transport`/`Parse`.
//!
//! Every chat call records `cauce_ai_requests_total{model,outcome}`,
//! `cauce_ai_tokens_total{model,kind}` and `cauce_ai_duration_ms`, and
//! — when the client was built with [`AnthropicClient::with_audit`] —
//! an `audit` row (`ai.provider_call`): model, tokens, ms, request_id.
//! The prompt text never leaves the request body. `/v1/models` is a
//! cached discovery call and is deliberately uninstrumented.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod errors;
mod pump;
#[cfg(test)]
mod tests;
mod wire;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use tokio::sync::mpsc;
use url::Url;

use crate::config::AiConfig;
use crate::metrics::Metrics;
use crate::store::Store;

use super::http::{ERROR_BODY_CAP, map_reqwest_error, read_capped};
use super::pump::PumpCtx;
use super::{AiCallCtx, AiError, AiStreamEvent, ChatRequest, ModelInfo};

use errors::map_error;
use pump::drive;
use wire::{ModelsPage, WireRequest, WireTool, map_tool_choice, wire_messages};

/// `GET /v1/models` listing cache TTL (the W4-01 convention).
const MODELS_CACHE_TTL: Duration = Duration::from_secs(60);

/// `(fetched_at, listing)` — cloned-out `Arc`s keep the lock short.
type ModelsCache = Arc<Mutex<Option<(Instant, Arc<Vec<ModelInfo>>)>>>;

/// `/v1/models` responses are legitimately large; cap at 16 MiB so a
/// misbehaving endpoint cannot exhaust memory.
const MODELS_BODY_CAP: usize = 16 * 1024 * 1024;

/// `anthropic-version` the client pins — the Messages API GA version.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// `max_tokens` is required on `/v1/messages`; when `ChatRequest`
/// doesn't carry one (the answer loop never does), the client sends
/// this ceiling — the model may still stop earlier.
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Anthropic Messages API streaming client over `/v1/messages` +
/// `/v1/models`. Cheap to clone — the reqwest pool, models cache and
/// audit handle are shared.
#[derive(Clone)]
pub struct AnthropicClient {
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

impl AnthropicClient {
    /// Build the client from `[ai]` config. `api_key` is already
    /// interpolated at config load (`${env:...}` resolved); an empty
    /// key means no `x-api-key` header (loopback gateways, proxies
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
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        if !cfg.api_key.is_empty() {
            let value = HeaderValue::from_str(&cfg.api_key)
                .map_err(|e| AiError::Parse(format!("invalid [ai].api_key header value: {e}")))?;
            headers.insert("x-api-key", value);
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

    /// Override the 60 s `/v1/models` cache TTL (tests).
    pub fn with_models_ttl(mut self, ttl: Duration) -> Self {
        self.models_ttl = ttl;
        self
    }

    /// The `[ai].model` default.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// `GET {base_url}/v1/models`, served from the 60 s cache while
    /// fresh. Errors are the same typed [`AiError`] set as chat calls.
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
            .get(format!("{}/v1/models", self.base_url))
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
                "/v1/models body exceeds the {MODELS_BODY_CAP} byte cap ({len} bytes announced)"
            )));
        }
        let body = res.bytes().await.map_err(map_reqwest_error)?;
        if body.len() > MODELS_BODY_CAP {
            return Err(AiError::Parse(format!(
                "/v1/models body exceeds the {MODELS_BODY_CAP} byte cap ({} bytes)",
                body.len()
            )));
        }
        let page: ModelsPage = serde_json::from_slice(&body)
            .map_err(|e| AiError::Parse(format!("/v1/models decode failed: {e}")))?;
        let models = Arc::new(
            page.data
                .into_iter()
                .map(|e| ModelInfo {
                    id: e.id,
                    name: e.display_name,
                    // The Anthropic listing doesn't carry a context
                    // length.
                    context_length: None,
                })
                .collect::<Vec<_>>(),
        );
        *self.models_cache.lock().unwrap() = Some((Instant::now(), models.clone()));
        Ok(models)
    }

    /// `POST {base_url}/v1/messages` with `stream: true`. Returns
    /// immediately with the event channel; a spawned task runs the
    /// HTTP exchange, pushes [`AiStreamEvent::Delta`]s as `text_delta`s
    /// arrive, folds `input_json_delta` fragments into the
    /// [`ChatCompletion`]'s tool calls, sends one terminal
    /// [`AiStreamEvent::Done`] or [`AiStreamEvent::Error`], then
    /// records metrics and the audit row. `budget` bounds the whole
    /// call — headers *and* the streamed body.
    ///
    /// Validation failures (no model, empty `messages`, unbuildable
    /// request, non-object tool-call arguments) return `Err`
    /// synchronously; everything after connect is an in-band `Error`
    /// event, mirroring the pipeline's `StreamEvent::Error` convention.
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

        let (system, messages) = wire_messages(&req.messages)?;
        let tools: Vec<WireTool<'_>> = req
            .tools
            .iter()
            .map(|spec| WireTool {
                name: &spec.name,
                description: &spec.description,
                input_schema: &spec.parameters,
            })
            .collect();
        let body = WireRequest {
            model: &model,
            stream: true,
            max_tokens: req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            system,
            messages,
            tools,
            tool_choice: req.tool_choice.as_ref().map(map_tool_choice),
            temperature: req.temperature,
        };
        let payload = serde_json::to_vec(&body)
            .map_err(|e| AiError::Parse(format!("request serialise failed: {e}")))?;
        let request = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
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
