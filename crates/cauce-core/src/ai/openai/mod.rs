//! OpenAI-compatible provider client (W4-01): `POST
//! {base_url}/chat/completions` with `stream: true`, parsed as SSE and
//! fanned out to a channel of [`AiStreamEvent`]s — text deltas as they
//! arrive, then one [`ChatCompletion`] with the tool calls assembled
//! from `tool_calls` deltas and the stream's `usage` block. `GET
//! {base_url}/models` is served from a 60 s in-process cache.
//!
//! Module map: `mod.rs` is the public [`OpenAiClient`] — construction,
//! `/models` and `chat_stream`. [`wire`] holds the request/response
//! JSON shapes; [`pump`] is the spawned task that runs the HTTP
//! exchange, parses SSE and folds chunks into a [`ChatCompletion`],
//! then records metrics and the audit row; [`errors`] classifies
//! provider failures into [`AiError`].
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

mod errors;
mod pump;
#[cfg(test)]
mod tests;
mod wire;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use tokio::sync::mpsc;
use url::Url;

use crate::config::AiConfig;
use crate::metrics::Metrics;
use crate::store::Store;

use super::{AiCallCtx, AiError, AiStreamEvent, ChatRequest, ModelInfo};

use errors::{ERROR_BODY_CAP, map_error, map_reqwest_error, read_capped};
use pump::{PumpCtx, drive};
use wire::{ModelsPage, WireRequest, WireStreamOptions, WireTool};

/// `GET /models` listing cache TTL (W4-01 settled: 60 s).
const MODELS_CACHE_TTL: Duration = Duration::from_secs(60);

/// `(fetched_at, listing)` — cloned-out `Arc`s keep the lock short.
type ModelsCache = Arc<Mutex<Option<(Instant, Arc<Vec<ModelInfo>>)>>>;

/// `/models` responses are legitimately large (OpenRouter's full list is
/// ~1 MB); cap at 16 MiB so a misbehaving endpoint cannot exhaust memory.
const MODELS_BODY_CAP: usize = 16 * 1024 * 1024;

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
