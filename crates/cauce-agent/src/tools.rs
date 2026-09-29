//! The tool side of the harness seam: [`ToolExecutor`], the
//! [`ToolCtx`] run context each call gets, and the [`ToolOutput`] the
//! loop reads back — plus the cauce tools [`AnswerLoop::new`]
//! installs by default.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use cauce_core::ai::{ToolCall, ToolSpec};
use cauce_core::{ClientKind, EngineId, SafeSearch, SearchPipeline, SearchRequest, SearchResult};

/// Results sent back to the model per `search_web` call (v2's
/// `num_results` ceiling); also the cited-source pool per call.
const TOOL_RESULT_LIMIT: usize = 10;

/// Run-scoped context handed to every [`ToolExecutor::execute`] —
/// what the tools may know about the caller without seeing the whole
/// request.
pub struct ToolCtx {
    /// The inbound surface's client kind; stamped on the
    /// `SearchRequest`s tools make (drives `search_log.client`).
    pub client: ClientKind,
}

/// What an executor produced for one call: the model-facing JSON body
/// and the hits it saw (the `sources` frame's dedup pool lives in the
/// loop, so every executor feeds the same pool).
#[derive(Debug)]
pub struct ToolOutput {
    /// Verbatim JSON document appended as the `tool` message content.
    pub json: String,
    /// Results the call saw — merged into the run's cited-source pool
    /// (deduped by URL, first-seen order) by the loop.
    pub results: Vec<SearchResult>,
}

/// One callable tool — the seam's executor side. `spec` advertises
/// the tool to the provider (name/description/parameters in the
/// provider-agnostic [`ToolSpec`] shape); `execute` performs the call.
/// A registry miss produces an `{"error": "unknown tool"}` result the
/// model can read, so a tool loop tolerates calls it cannot run.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// The provider-facing spec, read once when the registry is built.
    fn spec(&self) -> ToolSpec;

    /// Run one call. Errors the model can act on (missing args,
    /// backend failure) are returned as `{"error": ...}` JSON in the
    /// output, not thrown — the loop keeps the turn going either way.
    async fn execute(&self, call: &ToolCall, ctx: &ToolCtx) -> ToolOutput;
}

/// The executor set + their specs, aligned by index — built once so
/// the loop never re-evaluates `spec()` per call.
#[derive(Clone)]
pub(crate) struct ToolRegistry {
    entries: Vec<(ToolSpec, Arc<dyn ToolExecutor>)>,
}

impl ToolRegistry {
    pub(crate) fn new(executors: Vec<Arc<dyn ToolExecutor>>) -> Self {
        let entries = executors.into_iter().map(|e| (e.spec(), e)).collect();
        Self { entries }
    }

    /// Specs for the turn's `ChatRequest.tools`.
    pub(crate) fn specs(&self) -> Vec<ToolSpec> {
        self.entries.iter().map(|(s, _)| s.clone()).collect()
    }

    /// The executor registered under `name`.
    pub(crate) fn get(&self, name: &str) -> Option<&Arc<dyn ToolExecutor>> {
        self.entries
            .iter()
            .find(|(s, _)| s.name == name)
            .map(|(_, e)| e)
    }
}

/// `search_web` — live search through the shared pipeline (TTL cache,
/// admission, and politeness come free). [`AnswerLoop::new`] installs
/// it alongside [`SearchArchive`].
pub struct SearchWeb {
    pipeline: SearchPipeline,
}

impl SearchWeb {
    pub fn new(pipeline: SearchPipeline) -> Self {
        Self { pipeline }
    }
}

#[async_trait]
impl ToolExecutor for SearchWeb {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "search_web".to_string(),
            description: "Search the web through the cauce metasearch pipeline (TTL cache + \
                          engine fan-out). Args: query (required). Returns {query, results: \
                          [{title, url, snippet, engine}]}."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "the search query"},
                },
                "required": ["query"],
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, ctx: &ToolCtx) -> ToolOutput {
        let Some(query) = checked_query(call) else {
            return missing_query();
        };
        let req = SearchRequest {
            q: query,
            page: 1,
            lang: None,
            time_range: None,
            safesearch: SafeSearch::default(),
            engines: None,
            client: ctx.client.clone(),
        };
        match self.pipeline.search(&req).await {
            Ok(resp) => {
                let results: Vec<SearchResult> =
                    resp.results.into_iter().take(TOOL_RESULT_LIMIT).collect();
                let payload: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        json!({
                            "title": r.title,
                            "url": r.url.as_str(),
                            "snippet": r.snippet,
                            "engine": r.engine.as_str(),
                        })
                    })
                    .collect();
                ToolOutput {
                    json: json!({"query": resp.query, "results": payload}).to_string(),
                    results,
                }
            }
            Err(e) => ToolOutput {
                json: json!({"error": format!("search failed: {e}")}).to_string(),
                results: Vec::new(),
            },
        }
    }
}

/// `search_archive` (W5-03): the pipeline's hybrid RRF read over
/// `pages_fts` + `cache_fts`. Advertised unconditionally —
/// `SearchPipeline::search_archive` works on any build (the `pages`
/// and `cache_fts` tables exist on every migration), so a server
/// built without the `archive` feature still answers archive queries.
pub struct SearchArchive {
    pipeline: SearchPipeline,
}

impl SearchArchive {
    pub fn new(pipeline: SearchPipeline) -> Self {
        Self { pipeline }
    }
}

#[async_trait]
impl ToolExecutor for SearchArchive {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "search_archive".to_string(),
            description:
                "Search the local archive: indexed pages and cached result snippets fused \
                          by RRF. Args: query (required), limit (max results, default 10). Returns \
                          {query, results: [{url, title, snippet, source ('page'|'cached_result'), \
                          score}], request_id}."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "the search query"},
                    "limit": {"type": "integer", "description": "max results (default 10, capped at 10)"},
                },
                "required": ["query"],
            }),
        }
    }

    async fn execute(&self, call: &ToolCall, _ctx: &ToolCtx) -> ToolOutput {
        let Some(query) = checked_query(call) else {
            return missing_query();
        };
        let limit = tool_limit(call);
        match self.pipeline.search_archive(&query, limit).await {
            Ok(hits) => {
                let payload: Vec<serde_json::Value> = hits
                    .iter()
                    .map(|h| {
                        json!({
                            "url": h.url.as_str(),
                            "title": h.title,
                            "snippet": h.snippet,
                            "source": h.source,
                            "score": h.score,
                        })
                    })
                    .collect();
                let results: Vec<SearchResult> = hits
                    .into_iter()
                    .map(|h| SearchResult {
                        url: h.url,
                        title: h.title,
                        snippet: h.snippet,
                        engine: EngineId::from("archive"),
                        published: None,
                        score: h.score,
                    })
                    .collect();
                ToolOutput {
                    json: json!({
                        "query": query,
                        "results": payload,
                        "request_id": Uuid::now_v7(),
                    })
                    .to_string(),
                    results,
                }
            }
            Err(e) => ToolOutput {
                json: json!({"error": format!("archive search failed: {e}")}).to_string(),
                results: Vec::new(),
            },
        }
    }
}

/// The `query` argument of a tool call (`""` on absent or malformed
/// arguments — the loop still echoes a step label and a tool error).
pub(crate) fn tool_query(call: &ToolCall) -> String {
    serde_json::from_str::<serde_json::Value>(&call.arguments)
        .ok()
        .and_then(|v| v.get("query")?.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The `query` argument, `None` when absent/empty — the shared
/// missing-`query` gate both shipped tools apply.
fn checked_query(call: &ToolCall) -> Option<String> {
    let q = tool_query(call);
    (!q.is_empty()).then_some(q)
}

/// `{"error": "missing 'query' argument"}` — the model-readable
/// result for a call with no usable `query`.
fn missing_query() -> ToolOutput {
    ToolOutput {
        json: json!({"error": "missing 'query' argument"}).to_string(),
        results: Vec::new(),
    }
}

/// The optional `limit` argument of a `search_archive` call — absent
/// or malformed defaults to `TOOL_RESULT_LIMIT`, and the cap applies
/// either way so the model cannot widen its own source pool.
fn tool_limit(call: &ToolCall) -> u32 {
    serde_json::from_str::<serde_json::Value>(&call.arguments)
        .ok()
        .and_then(|v| v.get("limit")?.as_u64())
        .map(|n| n.clamp(1, TOOL_RESULT_LIMIT as u64) as u32)
        .unwrap_or(TOOL_RESULT_LIMIT as u32)
}
