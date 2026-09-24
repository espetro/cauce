//! `declarative` engine runtime (W1-02): a YAML `EngineSpec` (parent plan
//! section 4.3) compiled once into CSS selectors / JSONPaths / regexes,
//! fetched through the shared [`HttpClient`], parsed into
//! [`SearchResult`]s.
//!
//! Submodules: [`spec`] (schema + validation + request templating),
//! [`parse`] (detect + extraction), [`redirect`] (tracking-redirect
//! unwrapping), [`loading`] (embedded `engines/*.yaml` +
//! `$CAUCE_CONFIG_DIR/engines/` overrides), [`fixtures`] (`cauce engine test`
//! fixture-pair machinery), [`canary`] (the `cauce engine test --live`
//! nightly drift checks).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

pub mod canary;
pub mod fixtures;
pub mod loading;
mod parse;
mod redirect;
pub mod spec;

use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::header::HeaderMap;
use url::Url;

use cauce_core::http::HttpClient;
use cauce_core::{
    Engine, EngineError, EngineId, EnginePhase, Metrics, SearchRequest, SearchResult, Tier,
};

pub use loading::{load_specs, resolve_spec_source};
pub use spec::{CompiledSpec, EngineSpec, SpecError};

/// A loaded declarative engine: compiled spec + shared `HttpClient`.
///
/// `Engine::search` renders the URL/headers for the request, `GET`s under
/// `min(budget, request.timeout_ms)`, then maps `(status, body)` through
/// `detect` and `parse` ([`CompiledSpec::parse_response`]).
#[derive(Debug)]
pub struct DeclarativeEngine {
    compiled: CompiledSpec,
    http: HttpClient,
    /// `[[engines]]` `tier`/`page_size` overrides win over the spec.
    tier: Tier,
    page_size: u8,
    /// W1-09 phase timings (`cauce_engine_duration_ms{phase}`) recorded on
    /// the `Engine::search` path only; `fetch`/`parse_response` stay
    /// uninstrumented so `cauce engine test` runs don't pollute stats.
    metrics: Metrics,
}

impl DeclarativeEngine {
    /// Wrap a compiled spec with its HTTP client.
    pub fn new(compiled: CompiledSpec, http: HttpClient) -> Self {
        let tier = compiled.spec().tier;
        let page_size = compiled.spec().page_size;
        Self {
            compiled,
            http,
            tier,
            page_size,
            metrics: Metrics,
        }
    }

    /// The compiled spec.
    pub fn compiled(&self) -> &CompiledSpec {
        &self.compiled
    }

    /// Apply `[[engines]]` `tier`/`page_size` overrides (factory only).
    pub(crate) fn with_overrides(mut self, tier: Option<Tier>, page_size: Option<u8>) -> Self {
        if let Some(t) = tier {
            self.tier = t;
        }
        if let Some(ps) = page_size {
            self.page_size = ps;
        }
        self
    }

    /// Render the request and `GET` it: `(final_url, status, body)`. Split
    /// from [`parse_response`] so `cauce engine test --live --record` can
    /// keep the raw body for the fixture pair.
    pub async fn fetch(
        &self,
        req: &SearchRequest,
        budget: Duration,
    ) -> Result<Fetched, EngineError> {
        let url = self.compiled.render_url(req)?;
        let headers: HeaderMap = self.compiled.render_headers(req)?;
        let budget = self.compiled.effective_budget(budget);
        let res = self
            .http
            .get_with_headers(url.as_str(), budget, headers)
            .await?;
        Ok(Fetched {
            url: res.url,
            status: res.status,
            body: res.body,
        })
    }

    /// `detect` + extraction on an already-fetched response.
    pub fn parse_response(
        &self,
        status: u16,
        body: &[u8],
        base: &Url,
    ) -> Result<Vec<SearchResult>, EngineError> {
        self.compiled.parse_response(status, body, base)
    }
}

/// One fetched upstream response (URL after redirects, status, raw body).
#[derive(Debug)]
pub struct Fetched {
    /// Final URL after redirects; the base for relative result links.
    pub url: Url,
    /// HTTP status as received.
    pub status: u16,
    /// Response body (capped at the policy's `max_response_bytes`).
    pub body: Vec<u8>,
}

#[async_trait]
impl Engine for DeclarativeEngine {
    fn id(&self) -> EngineId {
        self.compiled.id().clone()
    }

    fn tier(&self) -> Tier {
        self.tier
    }

    fn page_size(&self) -> u8 {
        self.page_size
    }

    async fn search(
        &self,
        req: &SearchRequest,
        budget: Duration,
    ) -> Result<Vec<SearchResult>, EngineError> {
        let fetch_t = Instant::now();
        let res = self.fetch(req, budget).await;
        self.metrics
            .record_engine_phase(&self.id(), EnginePhase::Http, fetch_t.elapsed());
        let res = res?;
        let parse_t = Instant::now();
        let out = self.parse_response(res.status, &res.body, &res.url);
        self.metrics
            .record_engine_phase(&self.id(), EnginePhase::Parse, parse_t.elapsed());
        out
    }
}
