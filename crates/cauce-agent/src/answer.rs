//! The grounded-answer tool loop (W4-02, parent plan section 4.5 and
//! `.agents/plans/v3/wave-4-ai-mode.md`) — moved out of `cauce-core`
//! behind this crate's harness seam (#230).
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

use serde_json::json;
use tokio::sync::mpsc;
use tracing::{Instrument, Span};
use uuid::Uuid;

use cauce_core::ai::{
    AiCallCtx, AiError, AiStreamEvent, AnswerFrame, AnswerRequest, AnswerRole, AnswerTurn,
    ChatCompletion, ChatMessage, ChatProvider, ChatRequest, render_answer_html,
};
use cauce_core::{
    AnswerKey, AnswerPayload, AnswerRow, AnswerSource, SearchPipeline, Store, normalize_query,
};

use crate::config::LoopConfig;
use crate::observer::{AgentObserver, LoopEvent, RunContext, RunKind, TracingObserver};
use crate::tools::{
    SearchArchive, SearchWeb, ToolCtx, ToolExecutor, ToolOutput, ToolRegistry, tool_query,
};

/// `answers` rows are written only at or above this self-reported
/// confidence (settled input; v2 `CACHE_MIN_CONFIDENCE`).
pub const CACHE_MIN_CONFIDENCE: u8 = 4;

/// Bytes held back from `delta` frames until `Done`: the metadata tail
/// (`{"confidence": ..., "related_questions": [...]}`) is parsed out of
/// the final `TAIL_WINDOW` bytes, so the UI never renders a raw JSON
/// footer mid-stream (v2's leak: `test_stream_answer_delta_excludes_json_tail`).
const TAIL_WINDOW: usize = 500;
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

/// The grounded-answer tool loop — the harness this crate ships.
///
/// Clone is cheap (Arc'd parts); `cauce-server` builds one at app
/// construction and shares it across requests (W4-03). The parts are
/// the seam: provider, tool registry, answers store, [`LoopConfig`],
/// [`AgentObserver`] — any of them swappable.
#[derive(Clone)]
pub struct AnswerLoop {
    provider: Arc<dyn ChatProvider>,
    tools: ToolRegistry,
    store: Arc<dyn Store>,
    config: LoopConfig,
    observer: Arc<dyn AgentObserver>,
}

impl AnswerLoop {
    /// The shipped wiring: `search_web` + `search_archive` over
    /// `pipeline`, [`LoopConfig::default`], [`TracingObserver`].
    pub fn new(
        pipeline: SearchPipeline,
        provider: Arc<dyn ChatProvider>,
        store: Arc<dyn Store>,
    ) -> Self {
        Self::with_parts(
            provider,
            vec![
                Arc::new(SearchWeb::new(pipeline.clone())),
                Arc::new(SearchArchive::new(pipeline)),
            ],
            store,
            LoopConfig::default(),
            Arc::new(TracingObserver),
        )
    }

    /// The full-seam constructor — every part injectable. Embedders
    /// replacing the harness wholesale start here.
    pub fn with_parts(
        provider: Arc<dyn ChatProvider>,
        tools: Vec<Arc<dyn ToolExecutor>>,
        store: Arc<dyn Store>,
        config: LoopConfig,
        observer: Arc<dyn AgentObserver>,
    ) -> Self {
        Self {
            provider,
            tools: ToolRegistry::new(tools),
            store,
            config,
            observer,
        }
    }

    /// Replace the loop's knobs ([`LoopConfig`]) in one shot.
    pub fn with_config(mut self, config: LoopConfig) -> Self {
        self.config = config;
        self
    }

    /// Replace the observer — the hook surface eval/observability
    /// plugs in through ([`crate::NoopObserver`] for zero-cost silence).
    pub fn with_observer(mut self, observer: Arc<dyn AgentObserver>) -> Self {
        self.observer = observer;
        self
    }

    /// Replace the tool set the loop may dispatch.
    pub fn with_tools(mut self, tools: Vec<Arc<dyn ToolExecutor>>) -> Self {
        self.tools = ToolRegistry::new(tools);
        self
    }

    /// Per-call provider timeout override.
    pub fn with_provider_budget(mut self, budget: Duration) -> Self {
        self.config.provider_budget = budget;
        self
    }

    /// `answers` row TTL override (tests use seconds, not days).
    pub fn with_answers_ttl(mut self, ttl: Duration) -> Self {
        self.config.answers_ttl = ttl;
        self
    }

    /// Tool-loop cap override (settled default: 5).
    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.config.max_iterations = n;
        self
    }

    /// Final-turn tail-parse override: `(answer, confidence,
    /// related_questions)` from the streamed content. Tests/evals only —
    /// W4-04's gate proof swaps in a parser that leaves the metadata tail
    /// in the body.
    pub fn with_tail_parser(mut self, f: fn(&str) -> (String, u8, Vec<String>)) -> Self {
        self.config.tail_parser = f;
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
    ///
    /// Runs through the same [`turn`]/tail-parse machinery as the tool
    /// loop — the no-tools variant is one turn of it.
    pub fn stream_assist(
        &self,
        req: &AnswerRequest,
        results: Vec<AnswerSource>,
    ) -> mpsc::UnboundedReceiver<AnswerFrame> {
        let run = RunContext {
            request_id: req.request_id.unwrap_or_else(Uuid::now_v7),
            query: normalize_query(&req.q),
            client: req.client.clone(),
            kind: RunKind::Assist {
                sources: results.len(),
            },
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let loop_ = self.clone();
        let req = req.clone();
        let span = loop_.observer.run_span(&run).unwrap_or_else(Span::none);
        tokio::spawn(
            async move {
                loop_.observer.on_event(&run, &LoopEvent::Started);
                loop_.run_assist(req, results, tx, &run).await;
                loop_.observer.on_event(&run, &LoopEvent::Finished);
            }
            .instrument(span),
        );
        rx
    }

    /// Open the frame channel for one answer request. All failures ride
    /// in-band as a terminal `error` frame (the `AiStreamEvent`
    /// convention) — a spawned task holds the loop, so a dropped receiver
    /// still finishes the provider turn without more frames landing.
    pub fn stream_answer(&self, req: &AnswerRequest) -> mpsc::UnboundedReceiver<AnswerFrame> {
        let run = RunContext {
            request_id: req.request_id.unwrap_or_else(Uuid::now_v7),
            query: normalize_query(&req.q),
            client: req.client.clone(),
            kind: RunKind::Answer,
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let loop_ = self.clone();
        let req = req.clone();
        let span = loop_.observer.run_span(&run).unwrap_or_else(Span::none);
        tokio::spawn(
            async move {
                loop_.observer.on_event(&run, &LoopEvent::Started);
                loop_.run(req, tx, &run).await;
                loop_.observer.on_event(&run, &LoopEvent::Finished);
            }
            .instrument(span),
        );
        rx
    }

    async fn run(
        &self,
        req: AnswerRequest,
        tx: mpsc::UnboundedSender<AnswerFrame>,
        run: &RunContext,
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
                    // The replayed `html` is re-rendered from the stored
                    // markdown — deterministic, and keeps
                    // `answers.payload_json` free of a derived field.
                    let html = render_answer_html(&hit.payload.answer, hit.sources.len());
                    if self.emit(
                        &tx,
                        run,
                        AnswerFrame::Sources {
                            sources: hit.sources,
                        },
                    ) {
                        self.emit(
                            &tx,
                            run,
                            AnswerFrame::Done {
                                answer: hit.payload.answer,
                                html,
                                confidence: hit.payload.confidence,
                                model: hit.model,
                                related_questions: hit.payload.related_questions,
                                cached: true,
                                request_id: run.request_id,
                                ungrounded: false,
                            },
                        );
                    }
                    return;
                }
                Ok(None) => {}
                // A failed lookup must not fail the answer — fail open like a
                // cache miss.
                Err(e) => self.observer.on_event(
                    run,
                    &LoopEvent::CacheFailed {
                        op: "lookup",
                        error: &e,
                    },
                ),
            }
        }

        let ctx = AiCallCtx {
            actor: req.actor.clone(),
            request_id: Some(run.request_id),
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
        messages.extend(req.history.iter().map(turn_to_chat));
        messages.push(ChatMessage::user(req.q.as_str()));
        let tools = self.tools.specs();
        let tool_ctx = ToolCtx {
            client: req.client.clone(),
        };
        // Cited sources across all tool calls: deduped by URL in the
        // first-seen (citation) order the model saw them in.
        let mut sources: Vec<AnswerSource> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for iteration in 0..self.config.max_iterations {
            let chat = ChatRequest {
                messages: messages.clone(),
                tools: tools.clone(),
                tool_choice: Some(json!("auto")),
                ..ChatRequest::default()
            };
            let mut text = String::new();
            // Bytes of `text` already emitted as deltas.
            let mut emitted = 0usize;
            let completion = match self
                .turn(&chat, &ctx, run, iteration, &tx, &mut text, &mut emitted)
                .await
            {
                Turn::Completed(c) => c,
                Turn::Aborted => return,
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
                    if !self.emit(
                        &tx,
                        run,
                        AnswerFrame::Step {
                            tool: call.name.clone(),
                            query,
                            label,
                        },
                    ) {
                        return;
                    }
                    self.observer
                        .on_event(run, &LoopEvent::ToolStarted { call });
                    let output = match self.tools.get(&call.name) {
                        Some(executor) => executor.execute(call, &tool_ctx).await,
                        None => ToolOutput {
                            json: json!({"error": format!("unknown tool: {}", call.name)})
                                .to_string(),
                            results: Vec::new(),
                        },
                    };
                    self.observer.on_event(
                        run,
                        &LoopEvent::ToolFinished {
                            call,
                            output: &output,
                        },
                    );
                    for r in output.results {
                        if seen.insert(r.url.as_str().to_string()) {
                            sources.push(AnswerSource {
                                url: r.url,
                                title: r.title,
                                snippet: r.snippet,
                                engine: r.engine,
                            });
                        }
                    }
                    messages.push(ChatMessage::tool_result(call.id.clone(), output.json));
                }
                continue;
            }

            // Final turn: tolerant tail parse, the unemitted remainder of
            // the answer body as the last delta, then sources + done.
            let (answer, confidence, related) = (self.config.tail_parser)(&completion.content);
            if let Some(rest) = answer.get(emitted..)
                && !rest.is_empty()
                && !self.emit(
                    &tx,
                    run,
                    AnswerFrame::Delta {
                        text: rest.to_string(),
                    },
                )
            {
                return;
            }
            if !self.emit(
                &tx,
                run,
                AnswerFrame::Sources {
                    sources: sources.clone(),
                },
            ) {
                return;
            }
            self.emit(
                &tx,
                run,
                AnswerFrame::Done {
                    html: render_answer_html(&answer, sources.len()),
                    answer: answer.clone(),
                    confidence,
                    model: completion.model.clone().unwrap_or_else(|| model.clone()),
                    related_questions: related.clone(),
                    cached: false,
                    request_id: run.request_id,
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
                if let Err(e) = self
                    .store
                    .put_answer(&key, &row, self.config.answers_ttl)
                    .await
                {
                    self.observer.on_event(
                        run,
                        &LoopEvent::CacheFailed {
                            op: "write",
                            error: &e,
                        },
                    );
                }
            }
            return;
        }

        self.emit(
            &tx,
            run,
            AnswerFrame::Error {
                message: format!(
                    "exceeded max iterations ({}) without a final answer",
                    self.config.max_iterations
                ),
                retry_after_s: None,
            },
        );
    }

    /// The assist turn behind [`AnswerLoop::stream_assist`]: cache probe
    /// under the result-set key, the supplied sources up-front, then one
    /// `tool_choice: "none"` provider call through the same [`turn`]
    /// (delta/tool-less fold) machinery as the tool loop.
    async fn run_assist(
        &self,
        req: AnswerRequest,
        results: Vec<AnswerSource>,
        tx: mpsc::UnboundedSender<AnswerFrame>,
        run: &RunContext,
    ) {
        let model = self.provider.model().to_string();
        let results: Vec<AnswerSource> = results.into_iter().take(ASSIST_MAX_SOURCES).collect();
        let key = AnswerKey::assist(&req.q, &model, &results);
        match self.store.get_answer(&key).await {
            Ok(Some(hit)) => {
                let html = render_answer_html(&hit.payload.answer, hit.sources.len());
                if self.emit(
                    &tx,
                    run,
                    AnswerFrame::Sources {
                        sources: hit.sources,
                    },
                ) {
                    self.emit(
                        &tx,
                        run,
                        AnswerFrame::Done {
                            answer: hit.payload.answer,
                            html,
                            confidence: hit.payload.confidence,
                            model: hit.model,
                            related_questions: hit.payload.related_questions,
                            cached: true,
                            request_id: run.request_id,
                            ungrounded: false,
                        },
                    );
                }
                return;
            }
            Ok(None) => {}
            // A failed lookup must not fail the answer — fail open like a
            // cache miss.
            Err(e) => self.observer.on_event(
                run,
                &LoopEvent::CacheFailed {
                    op: "lookup",
                    error: &e,
                },
            ),
        }

        // The grounding set is known before the provider call, so the
        // panel's source chips can render while text streams.
        if !self.emit(
            &tx,
            run,
            AnswerFrame::Sources {
                sources: results.clone(),
            },
        ) {
            return;
        }

        let ctx = AiCallCtx {
            actor: req.actor.clone(),
            request_id: Some(run.request_id),
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
        let mut text = String::new();
        // Bytes of `text` already emitted as deltas.
        let mut emitted = 0usize;
        let completion = match self
            .turn(&chat, &ctx, run, 0, &tx, &mut text, &mut emitted)
            .await
        {
            Turn::Completed(c) => c,
            Turn::Aborted => return,
        };

        // Single turn, no tools: whatever text the turn produced is the
        // answer; a provider that ignores `tool_choice` cannot re-enter
        // the loop from here.
        let (answer, confidence, related) = (self.config.tail_parser)(&completion.content);
        if let Some(rest) = answer.get(emitted..)
            && !rest.is_empty()
            && !self.emit(
                &tx,
                run,
                AnswerFrame::Delta {
                    text: rest.to_string(),
                },
            )
        {
            return;
        }
        self.emit(
            &tx,
            run,
            AnswerFrame::Done {
                html: render_answer_html(&answer, results.len()),
                answer: answer.clone(),
                confidence,
                model: completion.model.clone().unwrap_or_else(|| model.clone()),
                related_questions: related.clone(),
                cached: false,
                request_id: run.request_id,
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
            if let Err(e) = self
                .store
                .put_answer(&key, &row, self.config.answers_ttl)
                .await
            {
                self.observer.on_event(
                    run,
                    &LoopEvent::CacheFailed {
                        op: "write",
                        error: &e,
                    },
                );
            }
        }
    }

    /// One provider turn folded to completion: opens `chat_stream`,
    /// emits deltas (holding back `TAIL_WINDOW` bytes), and resolves
    /// the assembled [`ChatCompletion`]. `ProviderTurn` fires before
    /// the call, `ProviderCompleted`/`ProviderFailed` on resolution;
    /// [`Turn::Aborted`] means a terminal frame was already emitted —
    /// the run ends.
    #[allow(clippy::too_many_arguments)]
    async fn turn(
        &self,
        chat: &ChatRequest,
        ctx: &AiCallCtx,
        run: &RunContext,
        iteration: usize,
        tx: &mpsc::UnboundedSender<AnswerFrame>,
        text: &mut String,
        emitted: &mut usize,
    ) -> Turn {
        self.observer.on_event(
            run,
            &LoopEvent::ProviderTurn {
                iteration,
                request: chat,
            },
        );
        let mut events =
            match self
                .provider
                .chat_stream(chat, self.config.provider_budget, ctx.clone())
            {
                Ok(rx) => rx,
                Err(e) => {
                    self.observer.on_event(
                        run,
                        &LoopEvent::ProviderFailed {
                            iteration,
                            error: &e,
                        },
                    );
                    self.send_error(tx, run, &e);
                    return Turn::Aborted;
                }
            };
        let mut completion: Option<ChatCompletion> = None;
        while let Some(event) = events.recv().await {
            match event {
                AiStreamEvent::Delta(d) => {
                    text.push_str(&d);
                    if !self.emit_deltas(tx, run, text, emitted) {
                        return Turn::Aborted;
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
                    self.flush(tx, run, text, emitted);
                    self.observer.on_event(
                        run,
                        &LoopEvent::ProviderFailed {
                            iteration,
                            error: &e,
                        },
                    );
                    self.send_error(tx, run, &e);
                    return Turn::Aborted;
                }
            }
        }
        match completion {
            Some(completion) => {
                self.observer.on_event(
                    run,
                    &LoopEvent::ProviderCompleted {
                        iteration,
                        completion: &completion,
                    },
                );
                Turn::Completed(completion)
            }
            None => {
                self.emit(
                    tx,
                    run,
                    AnswerFrame::Error {
                        message: "provider stream ended without a completion".to_string(),
                        retry_after_s: None,
                    },
                );
                Turn::Aborted
            }
        }
    }

    /// `emit` reports the frame to the observer, then pushes it —
    /// returns false once the receiver dropped: the loop stops emitting
    /// but the turn's work (search, audit) is already done.
    fn emit(
        &self,
        tx: &mpsc::UnboundedSender<AnswerFrame>,
        run: &RunContext,
        frame: AnswerFrame,
    ) -> bool {
        self.observer
            .on_event(run, &LoopEvent::Frame { frame: &frame });
        tx.send(frame).is_ok()
    }

    fn send_error(&self, tx: &mpsc::UnboundedSender<AnswerFrame>, run: &RunContext, e: &AiError) {
        self.emit(
            tx,
            run,
            AnswerFrame::Error {
                message: e.to_string(),
                retry_after_s: match e {
                    AiError::RateLimited { retry_after_s } => *retry_after_s,
                    _ => None,
                },
            },
        );
    }

    /// Emit the streamable prefix of `text` as deltas — everything but
    /// the last `TAIL_WINDOW` bytes of the rstripped text, where the
    /// metadata tail can begin. Byte indices stay on char boundaries.
    fn emit_deltas(
        &self,
        tx: &mpsc::UnboundedSender<AnswerFrame>,
        run: &RunContext,
        text: &str,
        emitted: &mut usize,
    ) -> bool {
        let mut upto = text.trim_end().len().saturating_sub(TAIL_WINDOW);
        while upto > *emitted && !text.is_char_boundary(upto) {
            upto -= 1;
        }
        if upto > *emitted {
            let delta = text[*emitted..upto].to_string();
            *emitted = upto;
            return self.emit(tx, run, AnswerFrame::Delta { text: delta });
        }
        true
    }

    /// Emit whatever remains of the unemitted stripped text (mid-stream
    /// failure path: no tail parse runs, so the whole remainder goes out).
    fn flush(
        &self,
        tx: &mpsc::UnboundedSender<AnswerFrame>,
        run: &RunContext,
        text: &str,
        emitted: &mut usize,
    ) {
        let rest = text.trim_end();
        if let Some(rest) = rest.get(*emitted..)
            && !rest.is_empty()
        {
            self.emit(
                tx,
                run,
                AnswerFrame::Delta {
                    text: rest.to_string(),
                },
            );
        }
    }
}

/// One provider turn's resolution inside [`AnswerLoop::turn`].
enum Turn {
    /// The stream closed with a `Done` completion.
    Completed(ChatCompletion),
    /// Terminal error already emitted — the run ends.
    Aborted,
}

/// The [`ChatMessage`] a prior [`AnswerTurn`] replays as (W7-04
/// threads); the wire type keeps no tool/system turns.
fn turn_to_chat(t: &AnswerTurn) -> ChatMessage {
    match t.role {
        AnswerRole::User => ChatMessage::user(&t.content),
        AnswerRole::Assistant => ChatMessage::assistant(&t.content),
    }
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
pub(crate) fn parse_final_answer(text: &str) -> (String, u8, Vec<String>) {
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
