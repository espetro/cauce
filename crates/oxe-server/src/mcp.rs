//! MCP surface (W1-08): the agent-facing half of oxe.
//!
//! Four tools over `rmcp` (settled arg shapes, wave-1 subplan):
//!
//! - `search_web(query, page?, engines?, lang?, ttl_s?)` — the canonical
//!   [`SearchResponse`] JSON, `meta.request_id` included.
//! - `cache_status()` — `Store::stats` snapshot plus `request_id`.
//! - `cache_invalidate(key? | expired? | all?)` — exactly one selector;
//!   mirrors the audited `DELETE /api/cache` semantics.
//! - `exa_search(query, num_results?, type?, source?, exclude_domains?,
//!   category?)` — the frozen Exa wire shape from `v2-legacy:oxe/api/exa.py`
//!   (`{requestId, searchType, results, costDollars, tool_source}`); the v2
//!   `source = "history"` short-circuit maps to the `clicks` table.
//!
//! Transports: streamable HTTP mounted at `/mcp` ([`streamable_service`]) and
//! stdio for `oxe mcp` ([`serve_stdio`]), both against the same [`AppState`]
//! (same DB, same pipeline). Every tool call mints a UUIDv7 `request_id`,
//! lands in a `mcp.tool` span, and appends an `mcp.<tool>` audit row (parent
//! plan 6.1). The MCP client's `initialize.client_info.name` becomes
//! [`ClientKind::Mcp(name)`] on every `SearchRequest`.
//!
//! Rate limiting (until W1-07 lands `PipelineError::RateLimited`): an
//! `AllEnginesFailed` whose every failure is [`EngineError::RateLimited`] maps
//! to a JSON-RPC server error carrying `rate_limited` + `retry_after_s`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use oxe_core::{
    AuditRow, CacheKey, ClientKind, EngineError, EngineId, HistoryFilter, HistoryItem,
    PipelineError, SafeSearch, SearchOpts, SearchRequest, SearchResponse, Store, TimeRange,
};
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ErrorCode, ErrorData};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService, stdio};
use rmcp::{RoleServer, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::app::AppState;
use crate::observability::audit;

/// Server-defined JSON-RPC error code for rate limiting (inside the reserved
/// -32099..=-32000 range; `…029` echoes HTTP 429). `data.error` carries the
/// settled `rate_limited` label and `data.retry_after_s` the hint.
pub const MCP_RATE_LIMITED: ErrorCode = ErrorCode(-32029);

/// `retry_after_s` hint until W1-07 admission supplies a real budget. One
/// minute is well under the 15-minute breaker window a `RateLimited` engine
/// trip opens in W1-06.
const RATE_LIMIT_RETRY_AFTER_S: u64 = 60;

/// Exa `num_results` bounds, frozen from `v2-legacy` (`_NUM_RESULTS_BOUNDS`).
const NUM_RESULTS_MIN: u32 = 1;
const NUM_RESULTS_MAX: u32 = 30;

/// `highlights` per Exa item, frozen from `v2-legacy` (`_MAX_HIGHLIGHTS`,
/// `_HIGHLIGHT_SCORE`): first three sentences of the snippet, each scored 0.5.
const MAX_HIGHLIGHTS: usize = 3;
const HIGHLIGHT_SCORE: f32 = 0.5;

/// History-mode click cap (v2 `_HISTORY_LIMIT_BOUNDS` upper).
const HISTORY_LIMIT: u32 = 200;

/// The oxe MCP server: thin tool layer over [`AppState`].
///
/// `Clone` is cheap (Arc'd state + router): the streamable-HTTP transport
/// builds one handler per session via the factory in [`streamable_service`].
#[derive(Clone)]
pub struct OxeMcp {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

impl OxeMcp {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    /// `ClientKind::Mcp(<client_info.name>)`; `"unknown"` when the client did
    /// not identify itself (pre-initialize calls, bare `()` handlers).
    fn client_kind(ctx: &RequestContext<RoleServer>) -> ClientKind {
        let name = ctx
            .client_info()
            .map(|info| info.name)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        ClientKind::Mcp(name)
    }

    /// `store` access for tool bodies.
    fn store(&self) -> &Arc<dyn Store> {
        self.state.store()
    }

    /// Append the `mcp.<tool>` audit row (parent plan 6.1: MCP tool
    /// invocations are audited). Read-only tools swallow store errors — the
    /// event is already in the JSONL stream and a broken audit table must not
    /// fail a search; destructive tools propagate via the `?` at call sites
    /// marked `strict`.
    async fn audit_tool(
        &self,
        client: &ClientKind,
        tool: &'static str,
        request_id: Uuid,
        details: Value,
        strict: bool,
    ) -> Result<(), ErrorData> {
        let result = audit(
            self.store().as_ref(),
            AuditRow {
                id: None,
                ts: Utc::now(),
                actor: client.label(),
                action: format!("mcp.{tool}"),
                target: tool.to_string(),
                details,
                request_id: Some(request_id),
            },
        )
        .await;
        match (result, strict) {
            (Err(e), true) => Err(ErrorData::internal_error(
                format!("audit write failed: {e}"),
                Some(json!({ "request_id": request_id })),
            )),
            (Err(_), false) => Ok(()), // `audit` already logged the failure
            (Ok(()), _) => Ok(()),
        }
    }
}

/// `search_web` arguments (settled input shape).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchWebArgs {
    /// Query text.
    pub query: String,
    /// 1-based result page (default 1).
    pub page: Option<u8>,
    /// Engine pin, e.g. `["replay"]` or `["wikipedia"]` while iterating.
    /// Omitted or empty = the configured default fan-out.
    pub engines: Option<Vec<String>>,
    /// BCP-47 language hint (`en`, `de`, ...).
    pub lang: Option<String>,
    /// Per-call cache TTL override in seconds (capped at the configured
    /// `ttl_cap`, default 24 h).
    pub ttl_s: Option<u64>,
}

/// `cache_invalidate` arguments: exactly one selector.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CacheInvalidateArgs {
    /// Hex `CacheKey` of a single entry (the `/api/cache/{key}` form).
    pub key: Option<String>,
    /// Remove every entry past `expires_at`.
    pub expired: Option<bool>,
    /// Remove every cache entry.
    pub all: Option<bool>,
}

/// `exa_search` arguments (settled input shape; frozen v2 surface).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExaSearchArgs {
    /// Query text.
    pub query: String,
    /// Results to return, 1-30 (default 10). Only ever slices *down*: fewer
    /// engine results than `num_results` means fewer rows, not extra fetches.
    pub num_results: Option<u32>,
    /// Exa search type echo (`auto` | `instant`; deep variants ignored),
    /// surfaced verbatim as `searchType`.
    #[serde(rename = "type")]
    pub search_type: Option<String>,
    /// `web` (default), `cache` (same search, tagged `tool_source=cache`) or
    /// `history` (short-circuits to the `clicks` table).
    pub source: Option<String>,
    /// Domains to exclude; folded into the query as `-site:<domain>` (the v2
    /// rule — there is no native exclude-domains field on `SearchRequest`).
    pub exclude_domains: Option<Vec<String>>,
    /// `news` maps to a day freshness window; anything else is ignored.
    pub category: Option<String>,
}

#[tool_router]
impl OxeMcp {
    /// `search_web(query, page?, engines?, lang?, ttl_s?)`.
    #[tool(
        description = "Search the web through the oxe metasearch pipeline (TTL cache + engine fan-out). Args: query (required), page (1-based, default 1), engines (pin e.g. [\"replay\"] or [\"wikipedia\"], default = configured fan-out), lang, ttl_s (per-call cache TTL seconds). Returns the canonical SearchResponse JSON including meta.request_id and meta.source (cache tier vs network)."
    )]
    async fn search_web(
        &self,
        Parameters(args): Parameters<SearchWebArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let request_id = Uuid::now_v7();
        let client = Self::client_kind(&ctx);
        let span = info_span!(
            "mcp.tool",
            tool = "search_web",
            request_id = %request_id,
            client = %client.label(),
        );
        async move {
            if args.query.trim().is_empty() {
                return Err(invalid_params("query must be non-empty", request_id));
            }
            let page = args.page.unwrap_or(1);
            if page == 0 {
                return Err(invalid_params("page is 1-based", request_id));
            }
            let engines = args
                .engines
                .filter(|v| !v.is_empty())
                .map(|v| v.iter().map(EngineId::from).collect::<Vec<_>>());
            let pinned = engines.is_some();
            let req = SearchRequest {
                q: args.query,
                page,
                lang: args.lang,
                time_range: None,
                safesearch: SafeSearch::default(),
                engines,
                client: client.clone(),
            };
            let resp = self
                .state
                .pipeline()
                .search_opts(
                    &req,
                    SearchOpts {
                        request_id: Some(request_id),
                        ttl: args.ttl_s.map(Duration::from_secs),
                    },
                )
                .await
                .map_err(|e| pipeline_error(&e, pinned, request_id))?;
            self.audit_tool(
                &client,
                "search_web",
                request_id,
                json!({
                    "query": resp.query,
                    "page": page,
                    "result_count": resp.results.len(),
                    "source": resp.meta.source,
                }),
                false,
            )
            .await?;
            structured(&resp, request_id)
        }
        .instrument(span)
        .await
    }

    /// `cache_status()`.
    #[tool(
        description = "Cache and engine-health snapshot for this oxe instance. No arguments. Returns request_id plus the 7-day StatsSnapshot (searches, cache_hits, hit_rate, cache_entries, cache_entries_expired, per-engine health rows)."
    )]
    async fn cache_status(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let request_id = Uuid::now_v7();
        let client = Self::client_kind(&ctx);
        let span = info_span!(
            "mcp.tool",
            tool = "cache_status",
            request_id = %request_id,
            client = %client.label(),
        );
        async move {
            let stats = self
                .store()
                .stats(7)
                .await
                .map_err(|e| store_error(&e, request_id))?;
            let mut value = serde_json::to_value(&stats)
                .map_err(|e| internal_error(e.to_string(), request_id))?;
            value["request_id"] = json!(request_id);
            self.audit_tool(&client, "cache_status", request_id, json!({}), false)
                .await?;
            Ok(CallToolResult::structured(value))
        }
        .instrument(span)
        .await
    }

    /// `cache_invalidate(key? | expired? | all?)`.
    #[tool(
        description = "Invalidate cache entries. Exactly one selector: key (64-hex CacheKey of one entry), expired=true (entries past expires_at), or all=true (everything). Returns request_id plus deleted/removed. Audited."
    )]
    async fn cache_invalidate(
        &self,
        Parameters(args): Parameters<CacheInvalidateArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let request_id = Uuid::now_v7();
        let client = Self::client_kind(&ctx);
        let span = info_span!(
            "mcp.tool",
            tool = "cache_invalidate",
            request_id = %request_id,
            client = %client.label(),
        );
        async move {
            let (action, removed) = match (
                args.key.as_deref(),
                args.expired.unwrap_or(false),
                args.all.unwrap_or(false),
            ) {
                (Some(raw), false, false) => {
                    let key: CacheKey = raw
                        .parse()
                        .map_err(|e: String| invalid_params(e, request_id))?;
                    let deleted = self
                        .store()
                        .delete_cache(&key)
                        .await
                        .map_err(|e| store_error(&e, request_id))?;
                    if !deleted {
                        return Err(ErrorData::resource_not_found(
                            format!("no cache entry for key {key}"),
                            Some(json!({ "request_id": request_id })),
                        ));
                    }
                    (
                        "cache.delete",
                        json!({ "deleted": true, "key": key.as_str() }),
                    )
                }
                (None, true, false) => (
                    "cache.evict_expired",
                    json!({
                        "removed": self
                            .store()
                            .evict_expired()
                            .await
                            .map_err(|e| store_error(&e, request_id))?
                    }),
                ),
                (None, false, true) => (
                    "cache.clear",
                    json!({
                        "removed": self
                            .store()
                            .clear_cache()
                            .await
                            .map_err(|e| store_error(&e, request_id))?
                    }),
                ),
                _ => {
                    return Err(invalid_params(
                        "exactly one of key, expired=true or all=true is required",
                        request_id,
                    ));
                }
            };
            // Destructive: the audit row is part of the contract (strict).
            self.audit_tool(
                &client,
                "cache_invalidate",
                request_id,
                json!({ "action": action, "result": removed }),
                true,
            )
            .await?;
            let mut value = removed;
            value["request_id"] = json!(request_id);
            Ok(CallToolResult::structured(value))
        }
        .instrument(span)
        .await
    }

    /// `exa_search(query, num_results?, type?, source?, exclude_domains?,
    /// category?)` — the frozen Exa wire shape.
    #[tool(
        description = "Search the web and return Exa-shaped JSON. Args: query (required), num_results (1-30, default 10), type ('auto'|'instant'; deep variants ignored), source ('web'|'history'|'cache'; default 'web' — 'history' returns the user's recorded clicks instead of a search), exclude_domains, category ('news' for last 24h). Returns {requestId, searchType, results, costDollars, tool_source}."
    )]
    async fn exa_search(
        &self,
        Parameters(args): Parameters<ExaSearchArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let request_id = Uuid::now_v7();
        let client = Self::client_kind(&ctx);
        let span = info_span!(
            "mcp.tool",
            tool = "exa_search",
            request_id = %request_id,
            client = %client.label(),
        );
        async move {
            let num_results = args.num_results.unwrap_or(10);
            if !(NUM_RESULTS_MIN..=NUM_RESULTS_MAX).contains(&num_results) {
                return Err(invalid_params(
                    format!("num_results must be {NUM_RESULTS_MIN}-{NUM_RESULTS_MAX}"),
                    request_id,
                ));
            }
            let source = match args.source.as_deref().unwrap_or("web") {
                s @ ("history" | "cache") => s,
                _ => "web",
            };
            // `history` mode allows an empty query (all clicks); web and
            // cache modes need real query text.
            if source != "history" && args.query.trim().is_empty() {
                return Err(invalid_params("query must be non-empty", request_id));
            }

            if source == "history" {
                let results = self
                    .history_clicks(&args.query, num_results, request_id)
                    .await?;
                self.audit_tool(
                    &client,
                    "exa_search",
                    request_id,
                    json!({ "source": "history", "result_count": results.len() }),
                    false,
                )
                .await?;
                return structured(
                    &ExaHistoryResult {
                        request_id: request_id.to_string(),
                        query: args.query,
                        tool_source: "history",
                        results,
                    },
                    request_id,
                );
            }

            let req = SearchRequest {
                q: build_query_text(&args.query, args.exclude_domains.as_deref()),
                page: 1,
                lang: None,
                // `category == "news"` keeps the v2 day-only freshness window.
                time_range: (args.category.as_deref() == Some("news")).then_some(TimeRange::Day),
                safesearch: SafeSearch::default(),
                engines: None,
                client: client.clone(),
            };
            let resp = self
                .state
                .pipeline()
                .search_with_id(&req, request_id)
                .await
                .map_err(|e| pipeline_error(&e, false, request_id))?;
            let out = exa_response(&resp, &args, source, num_results, request_id);
            self.audit_tool(
                &client,
                "exa_search",
                request_id,
                json!({
                    "query": resp.query,
                    "source": source,
                    "result_count": out.results.len(),
                }),
                false,
            )
            .await?;
            structured(&out, request_id)
        }
        .instrument(span)
        .await
    }
}

impl OxeMcp {
    /// `source = "history"`: the `clicks` table (v2 `get_clicks`). When
    /// `query` is non-empty it narrows to clicks whose `query_hash` matches
    /// the canonical key of that query text — v3 clicks carry no query text,
    /// so the hash is the honest equivalent of v2's substring match.
    async fn history_clicks(
        &self,
        query: &str,
        num_results: u32,
        request_id: Uuid,
    ) -> Result<Vec<ExaHistoryClick>, ErrorData> {
        let query_hash = (!query.trim().is_empty()).then(|| {
            CacheKey::from(&SearchRequest {
                q: query.to_string(),
                page: 1,
                lang: None,
                time_range: None,
                safesearch: SafeSearch::default(),
                engines: None,
                client: ClientKind::Api,
            })
        });
        let items = self
            .store()
            .list_history(&HistoryFilter {
                since: None,
                q: None,
                limit: HISTORY_LIMIT,
            })
            .await
            .map_err(|e| store_error(&e, request_id))?;
        Ok(items
            .into_iter()
            .filter_map(|item| match item {
                HistoryItem::Click(row) => Some(row),
                HistoryItem::Search(_) => None,
            })
            .filter(|row| {
                query_hash
                    .as_ref()
                    .is_none_or(|qh| row.query_hash.as_ref() == Some(qh))
            })
            .take(num_results as usize)
            .map(|row| ExaHistoryClick {
                id: row.id,
                query_hash: row.query_hash.as_ref().map(|k| k.as_str().to_string()),
                url: row.url.to_string(),
                title: row.title,
                position: row.position,
                clicked_at: row.ts.to_rfc3339(),
                client: row.client.label(),
            })
            .collect())
    }
}

#[tool_handler(
    name = "oxe",
    version = "0.0.0",
    router = self.tool_router.clone(),
    instructions = "Local metasearch backed by a TTL cache. `search_web` returns the canonical oxe SearchResponse (meta.request_id, meta.source cache/network); `exa_search` returns the Exa-compatible shape for existing wiring; `cache_status`/`cache_invalidate` manage the shared cache. While iterating, pin engines=[\"replay\"] (deterministic, offline) or engines=[\"wikipedia\"] (keyless, gentle rate limits)."
)]
impl ServerHandler for OxeMcp {}

// ---------------------------------------------------------------------------
// Transports
// ---------------------------------------------------------------------------

/// The `/mcp` streamable-HTTP endpoint as a `tower::Service`, mounted by
/// `app::build_router` via `axum::routing::any_service`.
///
/// `LocalSessionManager` keeps per-session state in-process (v3.0 is a
/// single-binary loopback service; cross-instance session restore is the
/// `session_store` seam for later waves). `allowed_hosts` stays at the rmcp
/// loopback default: oxe is a loopback server, so DNS-rebinding protection
/// comes free.
pub fn streamable_service(state: AppState) -> StreamableHttpService<OxeMcp, LocalSessionManager> {
    StreamableHttpService::new(
        move || Ok(OxeMcp::new(state.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    )
}

/// `oxe mcp`: serve the same tools over stdio (no HTTP listener). Runs until
/// the client closes stdin (EOF) or cancels; the process exits cleanly on
/// transport close.
///
/// stdout is the JSON-RPC channel — nothing else may write to it, which is
/// why the observability init for this mode logs to stderr/files only.
pub async fn serve_stdio(state: AppState) -> Result<(), ErrorData> {
    let running = OxeMcp::new(state)
        .serve(stdio())
        .await
        .map_err(|e| ErrorData::internal_error(format!("stdio serve failed: {e}"), None))?;
    running
        .waiting()
        .await
        .map_err(|e| ErrorData::internal_error(format!("stdio task failed: {e}"), None))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Frozen Exa wire shape (v2-legacy `oxe/api/exa.py` + `oxe/api/mcp.py`)
// ---------------------------------------------------------------------------

/// `results[]` item of the Exa response (frozen).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExaResultItem {
    title: String,
    url: String,
    /// Exa's opaque id; v2 used the URL.
    id: String,
    /// `contents.text` defaults to true in the v2 MCP tool, so the snippet.
    text: String,
    highlights: Vec<String>,
    highlight_scores: Vec<f32>,
    /// RFC 3339 when the engine reports a publication date.
    published_date: Option<String>,
    author: Option<String>,
    image: Option<String>,
    favicon: Option<String>,
    extras: ExaExtras,
}

#[derive(Debug, Clone, Serialize)]
struct ExaExtras {
    links: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExaCostDollars {
    total: f64,
}

/// `exa_search` web/cache response (frozen `McpExaSearchResult`: Exa's
/// `ExaSearchResponse` plus the oxe-specific `tool_source`). The aliased
/// fields are camelCase; `tool_source` stays snake (no alias in v2).
#[derive(Debug, Serialize)]
struct ExaSearchResult {
    #[serde(rename = "requestId")]
    request_id: String,
    #[serde(rename = "searchType")]
    search_type: String,
    results: Vec<ExaResultItem>,
    #[serde(rename = "costDollars")]
    cost_dollars: ExaCostDollars,
    tool_source: String,
}

/// `exa_search` `source = "history"` response (v2 `McpExaHistoryResult`,
/// plus `request_id` — v3's every-tool-result contract).
#[derive(Debug, Serialize)]
struct ExaHistoryResult {
    request_id: String,
    query: String,
    tool_source: &'static str,
    results: Vec<ExaHistoryClick>,
}

/// One `clicks` row on the wire (v3 `ClickRow` fields, v2-compatible names).
#[derive(Debug, Serialize)]
struct ExaHistoryClick {
    id: Option<i64>,
    query_hash: Option<String>,
    url: String,
    title: String,
    position: u32,
    clicked_at: String,
    client: String,
}

/// v2 `_build_query_text`: `query` plus `-site:` operators for
/// `exclude_domains` (v3 has no `include_domains` arg — it is not in the
/// settled tool shape).
fn build_query_text(query: &str, exclude_domains: Option<&[String]>) -> String {
    let mut parts = vec![query.trim().to_string()];
    if let Some(domains) = exclude_domains {
        parts.extend(
            domains
                .iter()
                .filter(|d| !d.is_empty())
                .map(|d| format!("-site:{d}")),
        );
    }
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// v2 `_extract_highlights`: first `_MAX_HIGHLIGHTS` sentences of the
/// snippet, each scored `_HIGHLIGHT_SCORE`.
fn extract_highlights(snippet: &str) -> Vec<String> {
    split_sentences(snippet)
        .into_iter()
        .take(MAX_HIGHLIGHTS)
        .collect()
}

fn split_sentences(text: &str) -> Vec<String> {
    // The v2 regex `(?<=[.!?])\s+` without lookbehind: split on whitespace
    // that follows a sentence-final `.`, `!` or `?`.
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if matches!(c, '.' | '!' | '?') && chars.peek().is_some_and(|n| n.is_whitespace()) {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        sentences.push(trimmed.to_string());
    }
    sentences
}

/// Canonical `SearchResponse` -> frozen Exa shape (v2
/// `searx_response_to_exa` + `_result_to_exa` with both `contents` flags on).
fn exa_response(
    resp: &SearchResponse,
    args: &ExaSearchArgs,
    source: &str,
    num_results: u32,
    request_id: Uuid,
) -> ExaSearchResult {
    let results = resp
        .results
        .iter()
        .take(num_results as usize)
        .map(|r| {
            let url = r.url.to_string();
            let highlights = extract_highlights(&r.snippet);
            ExaResultItem {
                title: r.title.clone(),
                id: url.clone(),
                url,
                text: r.snippet.clone(),
                highlight_scores: vec![HIGHLIGHT_SCORE; highlights.len()],
                highlights,
                published_date: r.published.map(|d| d.to_rfc3339()),
                author: None,
                image: None,
                favicon: None,
                extras: ExaExtras { links: Vec::new() },
            }
        })
        .collect();
    ExaSearchResult {
        request_id: request_id.to_string(),
        search_type: args
            .search_type
            .clone()
            .unwrap_or_else(|| "auto".to_string()),
        results,
        cost_dollars: ExaCostDollars { total: 0.0 },
        tool_source: source.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

/// Serialize `value` into a structured tool result (`content` + `structuredContent`).
fn structured<T: Serialize>(value: &T, request_id: Uuid) -> Result<CallToolResult, ErrorData> {
    serde_json::to_value(value)
        .map(CallToolResult::structured)
        .map_err(|e| internal_error(e.to_string(), request_id))
}

/// `data` always carries `request_id` so even failures are traceable.
fn with_request_id(request_id: Uuid) -> Option<Value> {
    Some(json!({ "request_id": request_id }))
}

fn invalid_params(message: impl Into<String>, request_id: Uuid) -> ErrorData {
    ErrorData::invalid_params(message.into(), with_request_id(request_id))
}

fn internal_error(message: impl Into<String>, request_id: Uuid) -> ErrorData {
    ErrorData::internal_error(message.into(), with_request_id(request_id))
}

fn store_error(e: &oxe_core::StoreError, request_id: Uuid) -> ErrorData {
    ErrorData::internal_error(
        format!("store: {e}"),
        Some(json!({ "error": "store_error", "request_id": request_id })),
    )
}

/// `PipelineError` -> MCP error. `NoEngines` splits on whether the caller
/// pinned `engines` (bad pin = `invalid_params`, unconfigured = internal);
/// an all-`RateLimited` `AllEnginesFailed` is the settled `rate_limited`
/// error with `retry_after_s` (W1-07 will route the real budget through the
/// same shape).
fn pipeline_error(e: &PipelineError, pinned: bool, request_id: Uuid) -> ErrorData {
    match e {
        PipelineError::NoEngines if pinned => {
            invalid_params("engines pin matched no configured engine", request_id)
        }
        PipelineError::NoEngines => internal_error("no search engines configured", request_id),
        PipelineError::AllEnginesFailed(failures)
            if !failures.is_empty()
                && failures
                    .iter()
                    .all(|(_, err)| matches!(err, EngineError::RateLimited)) =>
        {
            ErrorData::new(
                MCP_RATE_LIMITED,
                "rate_limited",
                Some(json!({
                    "error": "rate_limited",
                    "retry_after_s": RATE_LIMIT_RETRY_AFTER_S,
                    "request_id": request_id,
                })),
            )
        }
        PipelineError::AllEnginesFailed(_) => ErrorData::internal_error(
            e.to_string(),
            Some(json!({ "error": "upstream_failed", "request_id": request_id })),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_text_folds_exclude_domains() {
        assert_eq!(
            build_query_text("rust async", Some(&["pinterest.com".to_string()])),
            "rust async -site:pinterest.com"
        );
        assert_eq!(build_query_text("  rust  ", None), "rust");
        assert_eq!(
            build_query_text("rust", Some(&[String::new(), "x.dev".to_string()])),
            "rust -site:x.dev"
        );
    }

    #[test]
    fn highlights_take_first_three_sentences() {
        let text = "One. Two! Three? Four.";
        assert_eq!(extract_highlights(text), vec!["One.", "Two!", "Three?"]);
        assert!(extract_highlights("").is_empty());
        assert_eq!(extract_highlights("no terminator"), vec!["no terminator"]);
    }

    #[test]
    fn all_rate_limited_maps_to_retry_hint() {
        let err = PipelineError::AllEnginesFailed(vec![(
            EngineId::from("bing"),
            EngineError::RateLimited,
        )]);
        let mcp = pipeline_error(&err, false, Uuid::nil());
        assert_eq!(mcp.code, MCP_RATE_LIMITED);
        assert_eq!(mcp.message.as_ref(), "rate_limited");
        assert_eq!(
            mcp.data.as_ref().unwrap()["retry_after_s"],
            json!(RATE_LIMIT_RETRY_AFTER_S)
        );
    }

    #[test]
    fn mixed_failures_are_upstream_error() {
        let err = PipelineError::AllEnginesFailed(vec![
            (EngineId::from("bing"), EngineError::RateLimited),
            (EngineId::from("brave"), EngineError::Timeout),
        ]);
        let mcp = pipeline_error(&err, false, Uuid::nil());
        assert_eq!(mcp.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(
            mcp.data.as_ref().unwrap()["error"],
            json!("upstream_failed")
        );
    }
}
