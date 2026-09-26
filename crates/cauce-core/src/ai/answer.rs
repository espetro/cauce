//! The grounded-answer tool loop (W4-02, parent plan section 4.5 and
//! `.agents/plans/v3/wave-4-ai-mode.md`).
//!
//! [`AnswerLoop::stream_answer`] opens an [`AnswerFrame`] channel per
//! request — `step` per tool call, `delta` as answer text streams (the
//! metadata tail is held back and never reaches the client), `sources`
//! once the cited set is known, then terminal `done` or `error`. A fresh
//! `answers` row replays as `sources` then `done{cached:true}`; a row is
//! written only when the caching rule holds (at least one source,
//! confidence at or above [`CACHE_MIN_CONFIDENCE`], no error).
//! `parse_final_answer` ports v2's tolerant tail parse (`oxe/ai.py`),
//! and the loop shape mirrors `SearchPipeline::search_stream` (spawned
//! task, unbounded channel, terminal frame on close).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{Instrument, info_span, warn};
use uuid::Uuid;

use crate::cache::normalize_query;
use crate::engine::EngineId;
use crate::pipeline::SearchPipeline;
use crate::request::{ClientKind, SafeSearch, SearchRequest};
use crate::response::SearchResult;
use crate::store::{AnswerKey, AnswerPayload, AnswerRow, AnswerSource, Store};

use super::{
    AiCallCtx, AiError, AiStreamEvent, ChatCompletion, ChatMessage, ChatRequest, OpenAiClient,
    ToolCall, ToolSpec,
};

/// `answers` row TTL (settled input: 24 h).
pub const DEFAULT_ANSWERS_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// Per-call provider budget, matching the engine `DEFAULT_DEADLINE` scale
/// and v2's `PROVIDER_TIMEOUT_S` (60 s).
pub const DEFAULT_PROVIDER_BUDGET: Duration = Duration::from_secs(60);
/// Tool-loop cap (settled input; v2 `MAX_ITERATIONS`).
pub const DEFAULT_MAX_ITERATIONS: usize = 5;
/// `answers` rows are written only at or above this self-reported
/// confidence (settled input; v2 `CACHE_MIN_CONFIDENCE`).
pub const CACHE_MIN_CONFIDENCE: u8 = 4;

/// Bytes held back from `delta` frames until `Done`: the metadata tail
/// (`{"confidence": ..., "related_questions": [...]}`) is parsed out of
/// the final `TAIL_WINDOW` bytes, so the UI never renders a raw JSON
/// footer mid-stream (v2's leak: `test_stream_answer_delta_excludes_json_tail`).
const TAIL_WINDOW: usize = 500;
/// Results sent back to the model per `search_web` call (v2's
/// `num_results` ceiling); also the cited-source pool per call.
const TOOL_RESULT_LIMIT: usize = 10;
/// v2's tail contract caps `related_questions` at 5 entries of 200 chars.
const RELATED_LIMIT: usize = 5;
const RELATED_MAX_LEN: usize = 200;

/// Sources serialized into the assist prompt and echoed as the
/// `sources` frame: the SERP's top rows only — a client cannot widen
/// the grounding set past this ceiling.
const ASSIST_MAX_SOURCES: usize = 10;

/// W7-02 Search Assist system prompt: same `[n]` citation + metadata
/// tail contract as [`SYSTEM_PROMPT`], but the results are provided
/// inline and no tools exist — the answer must come from the supplied
/// set alone (a tool-mentioning prompt on a no-tools turn would invite
/// the model to narrate searches it cannot run).
const ASSIST_SYSTEM_PROMPT: &str = "You are a metasearch answer engine running in search-assist \
mode. The user message contains a question and the search results already on the page, as a \
numbered list. Answer the question briefly using ONLY those results, citing inline as [n], \
where n is the 1-based index of the result you drew it from. If the results do not contain the \
answer, say so in one sentence instead of guessing. After your answer text, append a metadata \
JSON object in exactly this form: {\"confidence\": <1-10>, \"related_questions\": [...]}. Report \
confidence >= 8 only when the results well support the answer; report lower when the answer is \
partially grounded. Never fabricate sources or citations.";

/// System prompt of every answer request (v2 `SYSTEM_PROMPT`, extended
/// to both shipped tools): cite inline as `[n]`, append the
/// metadata JSON tail, report high confidence only when grounded.
const SYSTEM_PROMPT: &str = "You are a metasearch answer engine. Answer the user's question \
briefly and cite sources inline as [n], where n is the 1-based index of the source in the \
search results you drew it from. Use the search_web tool whenever the question needs \
current or external information, and search_archive for what the local archive already \
holds (indexed pages and cached results). After your answer text, append a metadata JSON \
object in exactly this form: {\"confidence\": <1-10>, \"related_questions\": [...]}. Report \
confidence >= 8 only when the sources well support the answer; report lower when the \
answer is partially grounded. Never fabricate sources or citations.";

/// W7-04: appended to [`SYSTEM_PROMPT`] when the request carries prior
/// turns. Earlier answers' `[n]` markers index THEIR turn's source
/// list, not this turn's — without the clause the model happily cites
/// numbers that point at the wrong cards.
const THREAD_PROMPT_SUFFIX: &str = " Earlier messages are prior turns of this conversation — \
use them as context, but their [n] markers referred to those turns' own source lists. In \
your reply, cite only the sources your tool calls return this turn.";

/// The `search_web` tool: same argument shape as the MCP surface's
/// `search_web`.
fn search_web_spec() -> ToolSpec {
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

/// The `search_archive` tool (W5-03): the pipeline's hybrid RRF read
/// over `pages_fts` + `cache_fts`. Advertised unconditionally —
/// `SearchPipeline::search_archive` works on any build (the `pages` and
/// `cache_fts` tables exist on every migration), so a server built
/// without the `archive` feature still answers archive queries.
fn search_archive_spec() -> ToolSpec {
    ToolSpec {
        name: "search_archive".to_string(),
        description: "Search the local archive: indexed pages and cached result snippets fused \
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

/// One item of the `stream_answer` channel — the settled W4-02 wire
/// shapes, serde-tagged on `type` so SSE is `data: {"type": ...}`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnswerFrame {
    /// A tool call is starting (`label` is the human-readable form the UI
    /// renders: `Searching: <query>`).
    Step {
        tool: String,
        query: String,
        label: String,
    },
    /// One chunk of answer text; concatenated deltas equal `done.answer`.
    Delta { text: String },
    /// The cited sources, once the final turn completes (empty when the
    /// model never searched — `done.ungrounded` then reads true).
    Sources { sources: Vec<AnswerSource> },
    /// Terminal frame on success.
    Done {
        answer: String,
        confidence: u8,
        model: String,
        related_questions: Vec<String>,
        /// `true` only on the `answers`-table replay.
        cached: bool,
        /// The inbound request id (supplied or minted UUIDv7).
        request_id: Uuid,
        /// `true` when the answer drew on no sources; absent otherwise.
        #[serde(skip_serializing_if = "is_false")]
        ungrounded: bool,
    },
    /// Terminal frame for failures that abort the stream before a `done`
    /// can be built — provider error, iterations exhausted.
    Error {
        message: String,
        /// Provider retry hint, only on rate limits.
        #[serde(skip_serializing_if = "Option::is_none")]
        retry_after_s: Option<u64>,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The provider half of the loop, kept behind a trait so the loop is
/// testable without a live endpoint and W4-05's Anthropic protocol can
/// implement the same shape.
pub trait ChatProvider: Send + Sync {
    /// Model id used for the `answers` key and the `done.model` fallback.
    fn model(&self) -> &str;
    /// One streamed turn; see [`OpenAiClient::chat_stream`].
    fn chat_stream(
        &self,
        req: &ChatRequest,
        budget: Duration,
        ctx: AiCallCtx,
    ) -> Result<mpsc::UnboundedReceiver<AiStreamEvent>, AiError>;
}

impl ChatProvider for OpenAiClient {
    fn model(&self) -> &str {
        self.model()
    }

    fn chat_stream(
        &self,
        req: &ChatRequest,
        budget: Duration,
        ctx: AiCallCtx,
    ) -> Result<mpsc::UnboundedReceiver<AiStreamEvent>, AiError> {
        OpenAiClient::chat_stream(self, req, budget, ctx)
    }
}

impl ChatProvider for crate::ai::AnthropicClient {
    fn model(&self) -> &str {
        self.model()
    }

    fn chat_stream(
        &self,
        req: &ChatRequest,
        budget: Duration,
        ctx: AiCallCtx,
    ) -> Result<mpsc::UnboundedReceiver<AiStreamEvent>, AiError> {
        crate::ai::AnthropicClient::chat_stream(self, req, budget, ctx)
    }
}

/// Inbound parameters of one `stream_answer` call. `request_id`/`actor`
/// follow the `RequestCtx`/audit conventions of `cauce-server` handlers:
/// the inbound surface's id is stamped on `done.request_id` and on the
/// provider-call audit rows (`AiCallCtx`).
#[derive(Debug, Clone)]
pub struct AnswerRequest {
    /// The user's question, as submitted.
    pub q: String,
    /// W7-04 conversation threads: the prior completed turns, oldest
    /// first — the client owns the thread and replays it verbatim
    /// (ephemeral page state; there is no server-side threads table
    /// yet). Each turn becomes one `user`/`assistant` message ahead of
    /// `q`. A request with history skips the `answers` cache on both
    /// sides: a replay keyed on `q` alone would answer follow-ups with
    /// the first turn's text, and a history-keyed row would only hit on
    /// a byte-identical replayed context. Ignored by `stream_assist`.
    pub history: Vec<AnswerTurn>,
    /// The inbound surface; stamped on the `search_web` tool's
    /// `search_log.client` rows.
    pub client: ClientKind,
    /// Inbound request id; a UUIDv7 is minted when absent.
    pub request_id: Option<Uuid>,
    /// `audit.actor` override for provider calls (`None` → `DEFAULT_ACTOR`).
    pub actor: Option<String>,
}

/// The `role` of a client-replayed [`AnswerTurn`] — only completed
/// user/assistant exchanges exist; tool-call and system turns are never
/// part of the wire shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnswerRole {
    User,
    Assistant,
}

/// One prior turn of an `/answer` thread (W7-04): `{role, content}` as
/// the page echoes it back — `content` is the submitted question for
/// `User`, the `done.answer` text (metadata tail already stripped) for
/// `Assistant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerTurn {
    pub role: AnswerRole,
    pub content: String,
}

impl AnswerTurn {
    /// The [`ChatMessage`] this turn replays as.
    fn to_chat(&self) -> ChatMessage {
        match self.role {
            AnswerRole::User => ChatMessage::user(&self.content),
            AnswerRole::Assistant => ChatMessage::assistant(&self.content),
        }
    }
}

/// The grounded-answer tool loop: provider + pipeline + store.
///
/// Clone is cheap (Arc'd parts); `cauce-server` will build one at app
/// construction and share it across requests (W4-03).
#[derive(Clone)]
pub struct AnswerLoop {
    provider: Arc<dyn ChatProvider>,
    pipeline: SearchPipeline,
    store: Arc<dyn Store>,
    provider_budget: Duration,
    answers_ttl: Duration,
    max_iterations: usize,
    /// Tail-parse of the final assistant turn — `parse_final_answer` in
    /// production. `eval ai` tests inject a deliberately broken parser to
    /// prove the gate catches a metadata-tail leak; the seam changes no
    /// `stream_answer` behaviour for anyone else.
    tail_parser: fn(&str) -> (String, u8, Vec<String>),
}

impl AnswerLoop {
    pub fn new(
        pipeline: SearchPipeline,
        provider: Arc<dyn ChatProvider>,
        store: Arc<dyn Store>,
    ) -> Self {
        Self {
            provider,
            pipeline,
            store,
            provider_budget: DEFAULT_PROVIDER_BUDGET,
            answers_ttl: DEFAULT_ANSWERS_TTL,
            max_iterations: DEFAULT_MAX_ITERATIONS,
            tail_parser: parse_final_answer,
        }
    }

    /// Per-call provider timeout override.
    pub fn with_provider_budget(mut self, budget: Duration) -> Self {
        self.provider_budget = budget;
        self
    }

    /// `answers` row TTL override (tests use seconds, not days).
    pub fn with_answers_ttl(mut self, ttl: Duration) -> Self {
        self.answers_ttl = ttl;
        self
    }

    /// Tool-loop cap override (settled default: 5).
    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    /// Final-turn tail-parse override: `(answer, confidence,
    /// related_questions)` from the streamed content. Tests/evals only —
    /// W4-04's gate proof swaps in a parser that leaves the metadata tail
    /// in the body.
    pub fn with_tail_parser(mut self, f: fn(&str) -> (String, u8, Vec<String>)) -> Self {
        self.tail_parser = f;
        self
    }

    /// W7-02 Search Assist: a single no-tools turn over the result set
    /// the SERP already returned (`context_results` on
    /// `POST /api/answer`). Frame order is `sources` (the supplied
    /// results, up-front so the panel's chips render before text lands)
    /// → `delta`s → `done`; caching follows the grounded-only rule under
    /// an [`AnswerKey::assist`] key that folds in the result set, so an
    /// assist row never collides with a tool-loop answer for the same
    /// `q`. Nothing here touches the pipeline — no engine re-fetch.
    pub fn stream_assist(
        &self,
        req: &AnswerRequest,
        results: Vec<AnswerSource>,
    ) -> mpsc::UnboundedReceiver<AnswerFrame> {
        let request_id = req.request_id.unwrap_or_else(Uuid::now_v7);
        let span = info_span!(
            "answer.assist",
            request_id = %request_id,
            query = %normalize_query(&req.q),
            client = %req.client,
            sources = results.len(),
        );
        let (tx, rx) = mpsc::unbounded_channel();
        let loop_ = self.clone();
        let req = req.clone();
        tokio::spawn(
            async move { loop_.run_assist(req, results, request_id, tx).await }.instrument(span),
        );
        rx
    }

    /// Open the frame channel for one answer request. All failures ride
    /// in-band as a terminal `error` frame (the `AiStreamEvent`
    /// convention) — a spawned task holds the loop, so a dropped receiver
    /// still finishes the provider turn without more frames landing.
    pub fn stream_answer(&self, req: &AnswerRequest) -> mpsc::UnboundedReceiver<AnswerFrame> {
        let request_id = req.request_id.unwrap_or_else(Uuid::now_v7);
        let span = info_span!(
            "answer.stream",
            request_id = %request_id,
            query = %normalize_query(&req.q),
            client = %req.client,
        );
        let (tx, rx) = mpsc::unbounded_channel();
        let loop_ = self.clone();
        let req = req.clone();
        tokio::spawn(async move { loop_.run(req, request_id, tx).await }.instrument(span));
        rx
    }

    async fn run(
        &self,
        req: AnswerRequest,
        request_id: Uuid,
        tx: mpsc::UnboundedSender<AnswerFrame>,
    ) {
        let model = self.provider.model().to_string();
        // W7-04: multi-turn requests never touch the `answers` cache —
        // the key preimage is `q` alone, so a lookup could replay the
        // first turn's answer at a follow-up, and a write would poison
        // single-turn lookups with context-dependent text.
        let cacheable = req.history.is_empty();
        let key = AnswerKey::new(&req.q, &model);
        if cacheable {
            match self.store.get_answer(&key).await {
                Ok(Some(hit)) => {
                    if send(
                        &tx,
                        AnswerFrame::Sources {
                            sources: hit.sources,
                        },
                    ) {
                        send(
                            &tx,
                            AnswerFrame::Done {
                                answer: hit.payload.answer,
                                confidence: hit.payload.confidence,
                                model: hit.model,
                                related_questions: hit.payload.related_questions,
                                cached: true,
                                request_id,
                                ungrounded: false,
                            },
                        );
                    }
                    return;
                }
                Ok(None) => {}
                // A failed lookup must not fail the answer — fail open like a
                // cache miss.
                Err(e) => warn!(error = %e, "answers lookup failed; continuing uncached"),
            }
        }

        let ctx = AiCallCtx {
            actor: req.actor.clone(),
            request_id: Some(request_id),
        };
        // W7-04: replay the client-owned thread ahead of `q` so the
        // provider sees the whole exchange; the suffix keeps this
        // turn's `[n]` citations pointing at this turn's sources.
        let system = if req.history.is_empty() {
            SYSTEM_PROMPT.to_string()
        } else {
            format!("{SYSTEM_PROMPT}{THREAD_PROMPT_SUFFIX}")
        };
        let mut messages = Vec::with_capacity(req.history.len() + 2);
        messages.push(ChatMessage::system(system));
        messages.extend(req.history.iter().map(AnswerTurn::to_chat));
        messages.push(ChatMessage::user(req.q.as_str()));
        let tools = vec![search_web_spec(), search_archive_spec()];
        // Cited sources across all tool calls: deduped by URL in the
        // first-seen (citation) order the model saw them in.
        let mut sources: Vec<AnswerSource> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for _ in 0..self.max_iterations {
            let chat = ChatRequest {
                messages: messages.clone(),
                tools: tools.clone(),
                tool_choice: Some(json!("auto")),
                ..ChatRequest::default()
            };
            let mut events =
                match self
                    .provider
                    .chat_stream(&chat, self.provider_budget, ctx.clone())
                {
                    Ok(rx) => rx,
                    Err(e) => {
                        send_error(&tx, &e);
                        return;
                    }
                };
            let mut completion: Option<ChatCompletion> = None;
            let mut text = String::new();
            // Bytes of `text` already emitted as deltas.
            let mut emitted = 0usize;
            while let Some(event) = events.recv().await {
                match event {
                    AiStreamEvent::Delta(d) => {
                        text.push_str(&d);
                        if !emit_deltas(&tx, &text, &mut emitted) {
                            return;
                        }
                    }
                    AiStreamEvent::Done(c) => {
                        completion = Some(*c);
                        break;
                    }
                    AiStreamEvent::Error(e) => {
                        // Mid-stream failure: flush what the model already
                        // produced (no tail parse runs on a failed turn),
                        // then the terminal error frame.
                        flush(&tx, &text, &mut emitted);
                        send_error(&tx, &e);
                        return;
                    }
                }
            }
            let Some(completion) = completion else {
                send(
                    &tx,
                    AnswerFrame::Error {
                        message: "provider stream ended without a completion".to_string(),
                        retry_after_s: None,
                    },
                );
                return;
            };

            if !completion.tool_calls.is_empty() {
                messages.push(ChatMessage::assistant_tool_calls(
                    completion.tool_calls.clone(),
                ));
                for call in &completion.tool_calls {
                    let query = tool_query(call);
                    let label = format!(
                        "Searching: {}",
                        if query.is_empty() { &call.name } else { &query }
                    );
                    if !send(
                        &tx,
                        AnswerFrame::Step {
                            tool: call.name.clone(),
                            query,
                            label,
                        },
                    ) {
                        return;
                    }
                    let result = self.run_tool(call, &req.client).await;
                    for r in result.results {
                        if seen.insert(r.url.as_str().to_string()) {
                            sources.push(AnswerSource {
                                url: r.url,
                                title: r.title,
                                snippet: r.snippet,
                                engine: r.engine,
                            });
                        }
                    }
                    messages.push(ChatMessage::tool_result(call.id.clone(), result.json));
                }
                continue;
            }

            // Final turn: tolerant tail parse, the unemitted remainder of
            // the answer body as the last delta, then sources + done.
            let (answer, confidence, related) = (self.tail_parser)(&completion.content);
            if let Some(rest) = answer.get(emitted..)
                && !rest.is_empty()
                && !send(
                    &tx,
                    AnswerFrame::Delta {
                        text: rest.to_string(),
                    },
                )
            {
                return;
            }
            if !send(
                &tx,
                AnswerFrame::Sources {
                    sources: sources.clone(),
                },
            ) {
                return;
            }
            send(
                &tx,
                AnswerFrame::Done {
                    answer: answer.clone(),
                    confidence,
                    model: completion.model.clone().unwrap_or_else(|| model.clone()),
                    related_questions: related.clone(),
                    cached: false,
                    request_id,
                    ungrounded: sources.is_empty(),
                },
            );
            // Grounded-only caching (settled input): >= 1 source,
            // confidence >= CACHE_MIN_CONFIDENCE, no error — and only
            // for single-turn requests (see `cacheable` above).
            if cacheable && !sources.is_empty() && confidence >= CACHE_MIN_CONFIDENCE {
                let row = AnswerRow {
                    query: normalize_query(&req.q),
                    model: model.clone(),
                    payload: AnswerPayload {
                        answer,
                        confidence,
                        related_questions: related,
                    },
                    sources,
                };
                if let Err(e) = self.store.put_answer(&key, &row, self.answers_ttl).await {
                    warn!(error = %e, "answers write failed");
                }
            }
            return;
        }

        send(
            &tx,
            AnswerFrame::Error {
                message: format!(
                    "exceeded max iterations ({}) without a final answer",
                    self.max_iterations
                ),
                retry_after_s: None,
            },
        );
    }

    /// The assist turn behind [`AnswerLoop::stream_assist`]: cache probe
    /// under the result-set key, the supplied sources up-front, then one
    /// `tool_choice: "none"` provider call through the same
    /// `emit_deltas`/tail-parse/`send_error` machinery as the tool loop.
    async fn run_assist(
        &self,
        req: AnswerRequest,
        results: Vec<AnswerSource>,
        request_id: Uuid,
        tx: mpsc::UnboundedSender<AnswerFrame>,
    ) {
        let model = self.provider.model().to_string();
        let results: Vec<AnswerSource> = results.into_iter().take(ASSIST_MAX_SOURCES).collect();
        let key = AnswerKey::assist(&req.q, &model, &results);
        match self.store.get_answer(&key).await {
            Ok(Some(hit)) => {
                if send(
                    &tx,
                    AnswerFrame::Sources {
                        sources: hit.sources,
                    },
                ) {
                    send(
                        &tx,
                        AnswerFrame::Done {
                            answer: hit.payload.answer,
                            confidence: hit.payload.confidence,
                            model: hit.model,
                            related_questions: hit.payload.related_questions,
                            cached: true,
                            request_id,
                            ungrounded: false,
                        },
                    );
                }
                return;
            }
            Ok(None) => {}
            // A failed lookup must not fail the answer — fail open like a
            // cache miss.
            Err(e) => warn!(error = %e, "answers lookup failed; continuing uncached"),
        }

        // The grounding set is known before the provider call, so the
        // panel's source chips can render while text streams.
        if !send(
            &tx,
            AnswerFrame::Sources {
                sources: results.clone(),
            },
        ) {
            return;
        }

        let ctx = AiCallCtx {
            actor: req.actor.clone(),
            request_id: Some(request_id),
        };
        let chat = ChatRequest {
            messages: vec![
                ChatMessage::system(ASSIST_SYSTEM_PROMPT),
                ChatMessage::user(assist_user_prompt(&req.q, &results)),
            ],
            tools: Vec::new(),
            tool_choice: Some(json!("none")),
            ..ChatRequest::default()
        };
        let mut events = match self.provider.chat_stream(&chat, self.provider_budget, ctx) {
            Ok(rx) => rx,
            Err(e) => {
                send_error(&tx, &e);
                return;
            }
        };
        let mut completion: Option<ChatCompletion> = None;
        let mut text = String::new();
        // Bytes of `text` already emitted as deltas.
        let mut emitted = 0usize;
        while let Some(event) = events.recv().await {
            match event {
                AiStreamEvent::Delta(d) => {
                    text.push_str(&d);
                    if !emit_deltas(&tx, &text, &mut emitted) {
                        return;
                    }
                }
                AiStreamEvent::Done(c) => {
                    completion = Some(*c);
                    break;
                }
                AiStreamEvent::Error(e) => {
                    // Mid-stream failure: flush what the model already
                    // produced (no tail parse runs on a failed turn),
                    // then the terminal error frame.
                    flush(&tx, &text, &mut emitted);
                    send_error(&tx, &e);
                    return;
                }
            }
        }
        let Some(completion) = completion else {
            send(
                &tx,
                AnswerFrame::Error {
                    message: "provider stream ended without a completion".to_string(),
                    retry_after_s: None,
                },
            );
            return;
        };

        // Single turn, no tools: whatever text the turn produced is the
        // answer; a provider that ignores `tool_choice` cannot re-enter
        // the loop from here.
        let (answer, confidence, related) = (self.tail_parser)(&completion.content);
        if let Some(rest) = answer.get(emitted..)
            && !rest.is_empty()
            && !send(
                &tx,
                AnswerFrame::Delta {
                    text: rest.to_string(),
                },
            )
        {
            return;
        }
        send(
            &tx,
            AnswerFrame::Done {
                answer: answer.clone(),
                confidence,
                model: completion.model.clone().unwrap_or_else(|| model.clone()),
                related_questions: related.clone(),
                cached: false,
                request_id,
                ungrounded: results.is_empty(),
            },
        );
        // Grounded-only caching (settled input): >= 1 source,
        // confidence >= CACHE_MIN_CONFIDENCE, no error — under the
        // assist key, so a tool-loop answer for the same `q` cannot be
        // replayed as an assist answer or vice versa.
        if !results.is_empty() && confidence >= CACHE_MIN_CONFIDENCE {
            let row = AnswerRow {
                query: normalize_query(&req.q),
                model: model.clone(),
                payload: AnswerPayload {
                    answer,
                    confidence,
                    related_questions: related,
                },
                sources: results,
            };
            if let Err(e) = self.store.put_answer(&key, &row, self.answers_ttl).await {
                warn!(error = %e, "answers write failed");
            }
        }
    }

    /// One tool call: `search_web` runs the shared pipeline (cache,
    /// admission and politeness come free), `search_archive` (W5-03)
    /// reads the local archive's hybrid RRF; anything else is an
    /// `{"error": ...}` result the model can read and retry.
    async fn run_tool(&self, call: &ToolCall, client: &ClientKind) -> ToolOutcome {
        if call.name != "search_web" && call.name != "search_archive" {
            return ToolOutcome {
                json: json!({"error": format!("unknown tool: {}", call.name)}).to_string(),
                results: Vec::new(),
            };
        }
        let query = tool_query(call);
        if query.is_empty() {
            return ToolOutcome {
                json: json!({"error": "missing 'query' argument"}).to_string(),
                results: Vec::new(),
            };
        }
        if call.name == "search_archive" {
            return self.run_search_archive(call, query).await;
        }
        let req = SearchRequest {
            q: query,
            page: 1,
            lang: None,
            time_range: None,
            safesearch: SafeSearch::default(),
            engines: None,
            client: client.clone(),
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
                ToolOutcome {
                    json: json!({"query": resp.query, "results": payload}).to_string(),
                    results,
                }
            }
            Err(e) => ToolOutcome {
                json: json!({"error": format!("search failed: {e}")}).to_string(),
                results: Vec::new(),
            },
        }
    }

    /// `search_archive` (W5-03): the `pages_fts` + `cache_fts` RRF
    /// fusion, `limit` honored up to `TOOL_RESULT_LIMIT`. The response
    /// payload mirrors the MCP wire shape including a fresh
    /// `request_id`; the hits join the cited-source pool with the
    /// `"archive"` engine label.
    async fn run_search_archive(&self, call: &ToolCall, query: String) -> ToolOutcome {
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
                ToolOutcome {
                    json: json!({
                        "query": query,
                        "results": payload,
                        "request_id": Uuid::now_v7(),
                    })
                    .to_string(),
                    results,
                }
            }
            Err(e) => ToolOutcome {
                json: json!({"error": format!("archive search failed: {e}")}).to_string(),
                results: Vec::new(),
            },
        }
    }
}

/// A tool call's model-facing JSON plus the results it saw (the source
/// pool the `sources` frame dedupes from).
struct ToolOutcome {
    json: String,
    results: Vec<SearchResult>,
}

/// `send` returns false once the receiver dropped — the loop stops
/// emitting but the turn's work (search, audit) is already done.
fn send(tx: &mpsc::UnboundedSender<AnswerFrame>, frame: AnswerFrame) -> bool {
    tx.send(frame).is_ok()
}

fn send_error(tx: &mpsc::UnboundedSender<AnswerFrame>, e: &AiError) {
    send(
        tx,
        AnswerFrame::Error {
            message: e.to_string(),
            retry_after_s: match e {
                AiError::RateLimited { retry_after_s } => *retry_after_s,
                _ => None,
            },
        },
    );
}

/// The assist user message: the question plus the supplied results as a
/// numbered JSON list — the `[n]` citation indices in
/// [`ASSIST_SYSTEM_PROMPT`] point into this ordering.
fn assist_user_prompt(q: &str, results: &[AnswerSource]) -> String {
    let list: Vec<serde_json::Value> = results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "n": i + 1,
                "title": r.title,
                "url": r.url.as_str(),
                "snippet": r.snippet,
                "engine": r.engine.as_str(),
            })
        })
        .collect();
    format!(
        "Question: {q}\n\nSearch results:\n{}",
        serde_json::to_string_pretty(&list).expect("results serialize")
    )
}

/// The `query` argument of a tool call (`""` on absent or malformed
/// arguments — the loop still echoes a step label and a tool error).
fn tool_query(call: &ToolCall) -> String {
    serde_json::from_str::<serde_json::Value>(&call.arguments)
        .ok()
        .and_then(|v| v.get("query")?.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The optional `limit` argument of a `search_archive` call — absent or
/// malformed defaults to `TOOL_RESULT_LIMIT`, and the cap applies
/// either way so the model cannot widen its own source pool.
fn tool_limit(call: &ToolCall) -> u32 {
    serde_json::from_str::<serde_json::Value>(&call.arguments)
        .ok()
        .and_then(|v| v.get("limit")?.as_u64())
        .map(|n| n.clamp(1, TOOL_RESULT_LIMIT as u64) as u32)
        .unwrap_or(TOOL_RESULT_LIMIT as u32)
}

/// Emit the streamable prefix of `text` as deltas — everything but the
/// last `TAIL_WINDOW` bytes of the rstripped text, where the metadata
/// tail can begin. Byte indices stay on char boundaries.
fn emit_deltas(tx: &mpsc::UnboundedSender<AnswerFrame>, text: &str, emitted: &mut usize) -> bool {
    let mut upto = text.trim_end().len().saturating_sub(TAIL_WINDOW);
    while upto > *emitted && !text.is_char_boundary(upto) {
        upto -= 1;
    }
    if upto > *emitted {
        let delta = text[*emitted..upto].to_string();
        *emitted = upto;
        return send(tx, AnswerFrame::Delta { text: delta });
    }
    true
}

/// Emit whatever remains of the unemitted stripped text (mid-stream
/// failure path: no tail parse runs, so the whole remainder goes out).
fn flush(tx: &mpsc::UnboundedSender<AnswerFrame>, text: &str, emitted: &mut usize) {
    let rest = text.trim_end();
    if let Some(rest) = rest.get(*emitted..)
        && !rest.is_empty()
    {
        send(
            tx,
            AnswerFrame::Delta {
                text: rest.to_string(),
            },
        );
    }
}

fn parse_confidence(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn parse_related(v: Option<&serde_json::Value>) -> Vec<String> {
    let Some(serde_json::Value::Array(items)) = v else {
        return Vec::new();
    };
    items
        .iter()
        .take(RELATED_LIMIT)
        .map(|item| {
            let s = match item {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            s.chars().take(RELATED_MAX_LEN).collect()
        })
        .collect()
}

/// v2's `parse_final_answer` (oxe/ai.py), rewritten: split the assistant
/// text into (answer body, confidence, related_questions). The last
/// `TAIL_WINDOW` bytes of the rstripped text are scanned for `{`
/// positions, newest first; the first whose JSON parses to an object
/// containing a usable `confidence` wins and the body ends just before
/// it. A garbled or absent tail yields `(whole text, 0, [])` — tolerant,
/// never raising on what the model sent.
fn parse_final_answer(text: &str) -> (String, u8, Vec<String>) {
    let stripped = text.trim_end();
    let mut start = stripped.len().saturating_sub(TAIL_WINDOW);
    while !stripped.is_char_boundary(start) {
        start += 1;
    }
    let candidates: Vec<usize> = stripped
        .char_indices()
        .filter(|(i, c)| *i >= start && *c == '{')
        .map(|(i, _)| i)
        .collect();
    for idx in candidates.iter().rev() {
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stripped[*idx..]) else {
            continue;
        };
        let Some(conf) = parsed.get("confidence").and_then(parse_confidence) else {
            continue;
        };
        let answer = stripped[..*idx].trim_end().to_string();
        let related = parse_related(parsed.get("related_questions"));
        return (answer, conf.clamp(0, 10) as u8, related);
    }
    (stripped.to_string(), 0, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::parse_final_answer;

    #[test]
    fn no_tail_keeps_whole_text() {
        let (answer, confidence, related) = parse_final_answer("just an answer");
        assert_eq!(answer, "just an answer");
        assert_eq!(confidence, 0);
        assert!(related.is_empty());
    }

    #[test]
    fn trailing_metadata_is_parsed_off() {
        let (answer, confidence, related) = parse_final_answer(
            "the answer [1]\n{\"confidence\": 8, \"related_questions\": [\"q1\", \"q2\"]}",
        );
        assert_eq!(answer, "the answer [1]");
        assert_eq!(confidence, 8);
        assert_eq!(related, ["q1", "q2"]);
    }

    #[test]
    fn garbled_tail_keeps_raw_text() {
        // The trailing `{` does not parse — v2 keeps the raw text in
        // the body rather than dropping the dangling brace.
        let (answer, confidence, _) = parse_final_answer("an answer {not json");
        assert_eq!(answer, "an answer {not json");
        assert_eq!(confidence, 0);
    }

    #[test]
    fn non_confidence_json_in_tail_is_kept() {
        let (answer, confidence, _) = parse_final_answer("uses {\"a\": 1} as data");
        assert_eq!(answer, "uses {\"a\": 1} as data");
        assert_eq!(confidence, 0);
    }

    #[test]
    fn confidence_accepts_numeric_strings_rejects_floats() {
        let (_, confidence, _) = parse_final_answer("a\n{\"confidence\": \"7\"}");
        assert_eq!(confidence, 7);
        let (answer, confidence, _) = parse_final_answer("a\n{\"confidence\": 7.5}");
        assert_eq!(answer, "a\n{\"confidence\": 7.5}");
        assert_eq!(confidence, 0);
    }

    #[test]
    fn confidence_clamps_to_ten() {
        let (_, confidence, _) = parse_final_answer("a\n{\"confidence\": 99}");
        assert_eq!(confidence, 10);
        let (_, confidence, _) = parse_final_answer("a\n{\"confidence\": -3}");
        assert_eq!(confidence, 0);
    }

    #[test]
    fn related_questions_are_capped_and_truncated() {
        let seven = (0..7)
            .map(|i| format!("\"q{i}\""))
            .collect::<Vec<_>>()
            .join(",");
        let text = format!("a\n{{\"confidence\": 5, \"related_questions\": [{seven}, 42]}}");
        let (_, _, related) = parse_final_answer(&text);
        assert_eq!(related.len(), 5, "v2 caps at 5 items");

        let long = "x".repeat(300);
        let text = format!("a\n{{\"confidence\": 5, \"related_questions\": [\"{long}\"]}}");
        let (_, _, related) = parse_final_answer(&text);
        assert_eq!(related[0].len(), 200, "v2 truncates each item at 200 chars");
    }
}
