//! The grounded-answer tool loop (W4-02, parent plan section 4.5 and
//! `.agents/plans/v3/wave-4-ai-mode.md`) — moved out of `cauce-core`
//! behind this crate's harness seam (#230).
//!
//! [`AnswerLoop::stream_answer`] opens an [`AnswerFrame`] channel per
//! request — `step` per tool call, `delta` as answer text streams (the
//! metadata tail is held back and never reaches the client), `sources`
//! once the cited set is known, then terminal `done` or `error`. A fresh
//! `answers` row replays as `sources` then `done{cached:true}`; a row is
//! written only when the caching rule holds (#232: the answer is
//! [`grounded`](crate::groundedness) — at least one source and every
//! sentence cited in-range — with no error; the verbalized confidence
//! score is display-only and never gates).
//! `parse_final_answer` ports v2's tolerant tail parse (`oxe/ai.py`),
//! and the loop shape mirrors `SearchPipeline::search_stream` (spawned
//! task, unbounded channel, terminal frame on close).
//!
//! The #231 guards bound the loop: `max_turns` caps provider calls and
//! `max_search_executions` the tool calls actually run (repeated
//! queries and over-budget calls resolve in-band as `{"error": ...}`
//! tool results — free). Exhaustion runs ONE forced-synthesize turn
//! (`tool_choice: "none"`), so a spent budget degrades to
//! `done{ungrounded|low-confidence}` instead of a bare `error` that
//! discards the paid searches; `sources` ships on every terminal path
//! once known.
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

use cauce_core::ai::grounded::groundedness;
use cauce_core::ai::{
    AiCallCtx, AiError, AiStreamEvent, AnswerFrame, AnswerRequest, AnswerRole, AnswerTurn,
    ChatCompletion, ChatMessage, ChatProvider, ChatRequest, render_answer_html,
};
use cauce_core::{
    AnswerKey, AnswerLogRow, AnswerPayload, AnswerRow, AnswerSource, AnswerStatus, ClientKind,
    SearchOrigin, SearchPipeline, Store, normalize_query,
};

use crate::config::LoopConfig;
use crate::observer::{AgentObserver, LoopEvent, RunContext, RunKind, TracingObserver};
use crate::tools::{
    SearchArchive, SearchWeb, ToolCtx, ToolExecutor, ToolOutput, ToolRegistry, tool_query,
};

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
/// to both shipped tools and — #231 — the fan-out discipline): cite
/// inline as `[n]`, fan a multi-part question out as parallel tool
/// calls in ONE response, answer as soon as the results suffice,
/// never re-issue a query already run; then append the metadata JSON
/// tail with honest confidence.
const SYSTEM_PROMPT: &str = "You are a metasearch answer engine. Answer the user's question \
briefly and cite sources inline as [n], where n is the 1-based index of the source in the \
search results you drew it from. Use the search_web tool whenever the question needs \
current or external information, and search_archive for what the local archive already \
holds (indexed pages and cached results). For a multi-part question, emit one search call \
per sub-question in a single response — parallel calls run together in one step. If the \
results already returned cover the question, answer now instead of searching again, and \
never re-issue a query you already ran (repeat queries fail). After your answer text, \
append a metadata JSON object in exactly this form: {\"confidence\": <1-10>, \
\"related_questions\": [...]}. Report confidence >= 8 only when the sources well support \
the answer; report lower when the answer is partially grounded. Never fabricate sources \
or citations.";

/// #231 forced-synthesize turn: appended to the system prompt once the
/// turn budget is spent. `tool_choice: "none"` forbids further calls —
/// the model must answer from the sources it already paid for, with
/// confidence reflecting how thin they are.
const FORCED_SYNTH_SUFFIX: &str = " Your search budget is exhausted — answer now from the \
results already gathered above. Cite them inline as [n] and report honest confidence \
(low when the results only partially cover the question).";

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

    /// Turn cap override (#231; settled default: 8 provider calls —
    /// the forced-synthesize turn rides on top).
    pub fn with_max_turns(mut self, n: usize) -> Self {
        self.config.max_turns = n;
        self
    }

    /// Search-execution cap override (#231; settled default: 6 tool
    /// calls actually run — repeats and over-budget calls resolve
    /// in-band for free).
    pub fn with_max_search_executions(mut self, n: usize) -> Self {
        self.config.max_search_executions = n;
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
                    // #254: a cached replay is a terminal run — log it
                    // before the frames move `hit`'s fields out.
                    let mut log = self.answer_log_row(run, &req);
                    log.status = AnswerStatus::Cached;
                    log.model = hit.model.clone();
                    log.answer = hit.payload.answer.clone();
                    log.confidence = Some(hit.payload.confidence);
                    log.sources = hit.sources.clone();
                    log.related_questions = hit.payload.related_questions.clone();
                    let log_id = self.log_answer(run, log).await;
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
                                log_id,
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
        // #231 guards: `(tool, normalized query)` pairs already run
        // this run — a repeat resolves in-band instead of re-hitting
        // the pipeline — and the count of tool calls actually
        // executed, the budget the model cannot see but must not burn.
        let mut visited: HashSet<(String, String)> = HashSet::new();
        let mut searches_used = 0usize;

        // The last provider turn of the run — the model's own
        // tool-free answer, or the forced-synthesize turn after the
        // turn budget is spent. Either way the finish below is shared.
        let (completion, emitted) = 'finish: {
            for iteration in 0..self.config.max_turns {
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
                    Turn::Failed(e) => {
                        self.emit_collected(&tx, run, &sources);
                        let mut log = self.answer_log_row(run, &req);
                        log.status = AnswerStatus::Error;
                        log.sources = sources.clone();
                        log.error = Some(e.to_string());
                        let log_id = self.log_answer(run, log).await;
                        self.send_error(&tx, run, &e, log_id);
                        return;
                    }
                };

                if !completion.tool_calls.is_empty() {
                    messages.push(ChatMessage::assistant_tool_calls(
                        completion.tool_calls.clone(),
                    ));
                    for call in &completion.tool_calls {
                        let query = tool_query(call);
                        // Repeat of a query this run already ran: the
                        // in-band error costs nothing and breaks the
                        // circular-search trap (#231). Keyed per tool —
                        // the same words against `search_archive` are a
                        // different corpus, not a repeat.
                        let visited_key = (call.name.clone(), normalize_query(&query));
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
                        if !visited.insert(visited_key) {
                            self.observer.on_event(
                                run,
                                &LoopEvent::ToolSkipped {
                                    call,
                                    reason: "duplicate query",
                                },
                            );
                            messages.push(ChatMessage::tool_result(
                                call.id.clone(),
                                json!({"error": "query already searched; refine or answer"})
                                    .to_string(),
                            ));
                            continue;
                        }
                        // The real budget: only calls that dispatch
                        // count. Past the cap the model gets an
                        // in-band error so it can still answer.
                        if searches_used >= self.config.max_search_executions {
                            self.observer.on_event(
                                run,
                                &LoopEvent::ToolSkipped {
                                    call,
                                    reason: "search budget exhausted",
                                },
                            );
                            messages.push(ChatMessage::tool_result(
                                call.id.clone(),
                                json!({"error": "search budget exhausted; answer from what you have"})
                                    .to_string(),
                            ));
                            continue;
                        }
                        self.observer
                            .on_event(run, &LoopEvent::ToolStarted { call });
                        let output = match self.tools.get(&call.name) {
                            Some(executor) => {
                                searches_used += 1;
                                executor.execute(call, &tool_ctx).await
                            }
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

                break 'finish (completion, emitted);
            }

            // Turn budget spent: ONE forced-synthesize turn — the
            // `tool_choice: "none"` shape `run_assist` already proves
            // on both protocols. Worst case degrades to
            // `done{ungrounded|low-confidence}`; the collected sources
            // still ship.
            if let Some(sys) = messages.first_mut() {
                let content = sys.content.take().unwrap_or_default();
                sys.content = Some(format!("{content}{FORCED_SYNTH_SUFFIX}"));
            }
            let chat = ChatRequest {
                messages: messages.clone(),
                tools: Vec::new(),
                tool_choice: Some(json!("none")),
                ..ChatRequest::default()
            };
            let mut text = String::new();
            let mut emitted = 0usize;
            match self
                .turn(
                    &chat,
                    &ctx,
                    run,
                    self.config.max_turns,
                    &tx,
                    &mut text,
                    &mut emitted,
                )
                .await
            {
                Turn::Completed(c) => break 'finish (c, emitted),
                Turn::Aborted => return,
                Turn::Failed(e) => {
                    self.emit_collected(&tx, run, &sources);
                    let mut log = self.answer_log_row(run, &req);
                    log.status = AnswerStatus::Error;
                    log.sources = sources.clone();
                    log.error = Some(e.to_string());
                    let log_id = self.log_answer(run, log).await;
                    self.send_error(&tx, run, &e, log_id);
                    return;
                }
            }
        };

        // Final turn (the model's own answer or the forced synthesis):
        // tolerant tail parse, the unemitted remainder of the answer
        // body as the last delta, then sources + done.
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
        // #254: the durable log row rides every terminal path — a fresh
        // `done` here (the `ungrounded` flag mirrors the frame's). Write
        // it before the frames so `done.log_id` carries the
        // `/answer/{id}` URL's id.
        let mut log = self.answer_log_row(run, &req);
        log.model = completion.model.clone().unwrap_or_else(|| model.clone());
        log.answer = answer.clone();
        log.confidence = Some(confidence);
        log.sources = sources.clone();
        log.related_questions = related.clone();
        log.ungrounded = sources.is_empty();
        let log_id = self.log_answer(run, log).await;
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
                log_id,
            },
        );
        // Grounded-only caching (#232): the deterministic groundedness
        // check — >= 1 source, every sentence carrying an in-range [n],
        // every [n] resolving to a real source — replaces the
        // verbalized-confidence gate, which was uncalibrated and
        // post-hoc. `confidence` still flows to `done` (display only).
        // Only single-turn requests cache (see `cacheable` above).
        if cacheable && groundedness(&answer, &sources) {
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
    }

    /// `answer_log` row skeleton (#254): the fields every terminal
    /// path of `run` shares — normalized + raw query, run id, the
    /// inbound client and its `origin` (the `user`/`agent` convention
    /// `search_log` uses: `user` iff `client == Ui`). Callers set
    /// `status`, `model`, `answer`, `confidence`, `sources`,
    /// `related_questions`, `ungrounded`, `error` per outcome.
    fn answer_log_row(&self, run: &RunContext, req: &AnswerRequest) -> AnswerLogRow {
        AnswerLogRow {
            id: None,
            ts: chrono::Utc::now(),
            query: run.query.clone(),
            query_raw: Some(req.q.clone()),
            model: self.provider.model().to_string(),
            answer: String::new(),
            confidence: None,
            sources: Vec::new(),
            related_questions: Vec::new(),
            request_id: Some(run.request_id),
            client: req.client.clone(),
            origin: if matches!(req.client, ClientKind::Ui) {
                SearchOrigin::User
            } else {
                SearchOrigin::Agent
            },
            status: AnswerStatus::Done,
            ungrounded: false,
            error: None,
        }
    }

    /// Append the run's `answer_log` row (#254); fail-open like the
    /// answers cache — a logging outage must not break the answer.
    /// Returns the row id so the terminal frame can carry it as
    /// `log_id` (`None` when the write failed).
    async fn log_answer(&self, run: &RunContext, row: AnswerLogRow) -> Option<i64> {
        match self.store.log_answer(row).await {
            Ok(id) => Some(id),
            Err(e) => {
                self.observer
                    .on_event(run, &LoopEvent::LogFailed { error: &e });
                None
            }
        }
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
                            // `stream_assist` never logs a row (#254).
                            log_id: None,
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
            Turn::Failed(e) => {
                // The supplied `sources` frame already went out before
                // the call — only the terminal error is left.
                self.send_error(&tx, run, &e, None);
                return;
            }
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
                // `stream_assist` never logs a row (#254).
                log_id: None,
                request_id: run.request_id,
                ungrounded: results.is_empty(),
            },
        );
        // Grounded-only caching (#232, same rule as the tool loop):
        // the supplied result set must exist and every answer sentence
        // must carry an in-range [n] — under the assist key, so a
        // tool-loop answer for the same `q` cannot be replayed as an
        // assist answer or vice versa.
        if groundedness(&answer, &results) {
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
    /// the call, `ProviderCompleted`/`ProviderFailed` on resolution.
    /// [`Turn::Failed`] hands the error to the caller — it owns the
    /// terminal frames, so collected `sources` ride ahead of `error`
    /// (#231); [`Turn::Aborted`] means the receiver is gone and the
    /// run just ends.
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
                    return Turn::Failed(e);
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
                    // produced (no tail parse runs on a failed turn) —
                    // the caller emits the terminal frames.
                    self.flush(tx, run, text, emitted);
                    self.observer.on_event(
                        run,
                        &LoopEvent::ProviderFailed {
                            iteration,
                            error: &e,
                        },
                    );
                    return Turn::Failed(e);
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
            None => Turn::Failed(AiError::Parse(
                "provider stream ended without a completion".to_string(),
            )),
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

    /// `sources` ahead of a terminal `error` — paid searches are never
    /// silently discarded (#231: the frame ships on every terminal
    /// path once known). An empty pool emits nothing: a first-turn
    /// failure keeps its bare `error` shape.
    fn emit_collected(
        &self,
        tx: &mpsc::UnboundedSender<AnswerFrame>,
        run: &RunContext,
        sources: &[AnswerSource],
    ) {
        if !sources.is_empty() {
            self.emit(
                tx,
                run,
                AnswerFrame::Sources {
                    sources: sources.to_vec(),
                },
            );
        }
    }

    fn send_error(
        &self,
        tx: &mpsc::UnboundedSender<AnswerFrame>,
        run: &RunContext,
        e: &AiError,
        log_id: Option<i64>,
    ) {
        self.emit(
            tx,
            run,
            AnswerFrame::Error {
                message: e.to_string(),
                retry_after_s: match e {
                    AiError::RateLimited { retry_after_s } => *retry_after_s,
                    _ => None,
                },
                log_id,
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
    /// The receiver dropped mid-stream — stop emitting, run ends.
    Aborted,
    /// The provider call failed — the caller emits the terminal
    /// `sources`/`error` frames.
    Failed(AiError),
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

/// `confidence` from the metadata tail: ints, floats (#232 — models
/// do emit `7.5`, and dropping the parse leaked raw JSON into
/// `done.answer`), and numeric strings — rounded to the nearest int
/// and clamped by the caller. Non-finite or non-numeric values fail.
fn parse_confidence(v: &serde_json::Value) -> Option<i64> {
    let to_int = |f: f64| f.is_finite().then(|| f.round() as i64);
    match v {
        serde_json::Value::Number(n) => n.as_f64().and_then(to_int),
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok().and_then(to_int),
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
/// it. A tail wrapped in a ```json fence (#217) is peeled first; a
/// trailing block that LOOKS like the metadata object but won't parse
/// is dropped rather than leaked into `done.answer` as raw JSON. A
/// garbled or absent tail yields `(whole text, 0, [])` — tolerant,
/// never raising on what the model sent.
pub(crate) fn parse_final_answer(text: &str) -> (String, u8, Vec<String>) {
    let stripped = text.trim_end();

    // #217: the metadata tail may arrive inside a closed fenced block
    // ("```json\n{…}\n```") — the `{`-scan alone cannot see past the
    // trailing ```. When the fence contents look like the metadata
    // object, the whole fence is metadata: parse it, or drop it when
    // unrecoverable — raw JSON must not leak into `done.answer`.
    if let Some((body, inner)) = split_trailing_fence(stripped)
        && looks_like_metadata(inner)
    {
        return match find_metadata_tail(inner) {
            Some((_, conf, related)) => (body.to_string(), conf, related),
            None => (body.to_string(), 0, Vec::new()),
        };
    }

    if let Some((idx, conf, related)) = find_metadata_tail(stripped) {
        // A dangling fence opener right before the tail ("…```json\n{…}")
        // is part of the metadata wrapper — never answer text.
        let body = strip_dangling_fence_opener(&stripped[..idx]);
        return (body.to_string(), conf, related);
    }

    // No parseable tail: an UNCLOSED trailing fence whose contents look
    // like metadata is still intent — drop it too rather than leak raw
    // JSON into the answer.
    if let Some(body) = drop_unrecoverable_metadata_block(stripped) {
        return (body.to_string(), 0, Vec::new());
    }
    (stripped.to_string(), 0, Vec::new())
}

/// Scan the last `TAIL_WINDOW` bytes of `stripped` for the metadata
/// object: `{` positions newest-first; the first whose trailing slice
/// parses to a JSON object holding a usable `confidence` wins. Returns
/// the `{` index, the clamped confidence, and `related_questions`.
fn find_metadata_tail(stripped: &str) -> Option<(usize, u8, Vec<String>)> {
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
        let related = parse_related(parsed.get("related_questions"));
        return Some((*idx, conf.clamp(0, 10) as u8, related));
    }
    None
}

/// If `stripped` ends with a closed fenced block ("```…\n<inner>\n```"),
/// return `(body before the fence opener, inner)`. Fence lines are
/// tracked by parity so an earlier balanced code block cannot claim the
/// final ``` as its own opener.
fn split_trailing_fence(stripped: &str) -> Option<(&str, &str)> {
    let closer_start = stripped.rfind('\n').map(|i| i + 1).unwrap_or(0);
    if stripped[closer_start..].trim() != "```" {
        return None;
    }
    let mut opener: Option<usize> = None;
    let mut pos = 0;
    for line in stripped[..closer_start].split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            opener = if opener.is_none() { Some(pos) } else { None };
        }
        pos += line.len();
    }
    let opener = opener?;
    let inner_start = stripped[opener..].find('\n').map(|i| opener + i + 1)?;
    Some((
        stripped[..opener].trim_end(),
        stripped[inner_start..closer_start].trim(),
    ))
}

/// The body cut just before a bare (unfenced) metadata tail may still
/// end on the opener the model wrapped it in ("…```json\n{…}"). When the
/// body's fence lines are unbalanced the last line is that opener —
/// drop it so the fence never leaks into `done.answer`.
fn strip_dangling_fence_opener(body: &str) -> &str {
    let body = body.trim_end();
    let fences = body
        .lines()
        .filter(|l| l.trim_start().starts_with("```"))
        .count();
    if fences % 2 == 0 {
        return body;
    }
    match body.rfind('\n') {
        Some(nl) if body[nl + 1..].trim_start().starts_with("```") => body[..nl].trim_end(),
        None if body.trim_start().starts_with("```") => "",
        _ => body,
    }
}

/// An unclosed trailing fence whose contents look like the metadata
/// object is intent, not answer text — return the body before its
/// opener so the raw JSON is dropped rather than leaked.
fn drop_unrecoverable_metadata_block(stripped: &str) -> Option<&str> {
    let mut opener: Option<(usize, usize)> = None;
    let mut pos = 0;
    for line in stripped.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            opener = match opener {
                None => Some((pos, pos + line.len())),
                Some(_) => None,
            };
        }
        pos += line.len();
    }
    let (start, inner_start) = opener?;
    if looks_like_metadata(&stripped[inner_start..]) {
        Some(stripped[..start].trim_end())
    } else {
        None
    }
}

/// Fence contents that were meant to be the metadata tail: they start
/// with `{` and name a `confidence` field (quoted or not — a broken
/// tail may have already lost its quotes). The `starts_with` keeps a
/// real code block that merely mentions confidence (e.g. a JSON
/// literal inside ```rust) from being eaten.
fn looks_like_metadata(inner: &str) -> bool {
    let inner = inner.trim_start();
    inner.starts_with('{') && inner.contains("confidence")
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
    fn confidence_accepts_numeric_strings_and_floats() {
        // #232: models emit `7.5`; rejecting floats dropped the tail and
        // leaked raw JSON into the answer.
        let (_, confidence, _) = parse_final_answer("a\n{\"confidence\": \"7\"}");
        assert_eq!(confidence, 7);
        let (answer, confidence, _) = parse_final_answer("a\n{\"confidence\": 7.5}");
        assert_eq!(answer, "a");
        assert_eq!(confidence, 8, "float confidence rounds to the nearest int");
        let (answer, confidence, _) = parse_final_answer("a\n{\"confidence\": 7.4}");
        assert_eq!(answer, "a");
        assert_eq!(confidence, 7);
        let (answer, confidence, _) = parse_final_answer("a\n{\"confidence\": \"7.5\"}");
        assert_eq!(answer, "a");
        assert_eq!(confidence, 8);
    }

    #[test]
    fn fenced_metadata_tail_is_parsed_off() {
        // #217: models wrap the tail in a ```json fence — the fence
        // peels, the object parses, no fence text survives in the body.
        let (answer, confidence, related) = parse_final_answer(
            "the answer [1]\n```json\n{\"confidence\": 8, \"related_questions\": [\"q1\"]}\n```",
        );
        assert_eq!(answer, "the answer [1]");
        assert_eq!(confidence, 8);
        assert_eq!(related, ["q1"]);
    }

    #[test]
    fn bare_fence_tail_is_parsed_off() {
        let (answer, confidence, _) =
            parse_final_answer("the answer\n```\n{\"confidence\": 6}\n```");
        assert_eq!(answer, "the answer");
        assert_eq!(confidence, 6);
    }

    #[test]
    fn unclosed_fence_opener_before_tail_is_stripped() {
        // The model opened ```json but never closed it: the bare-JSON
        // scan still finds the tail and the dangling opener goes too.
        let (answer, confidence, _) =
            parse_final_answer("the answer [1]\n```json\n{\"confidence\": 6}");
        assert_eq!(answer, "the answer [1]");
        assert_eq!(confidence, 6);
    }

    #[test]
    fn unrecoverable_fenced_tail_does_not_leak_json() {
        // Clearly metadata intent (starts with `{`, names `confidence`)
        // but unparseable — drop the whole fence rather than leak raw
        // JSON into `done.answer`.
        let (answer, confidence, _) =
            parse_final_answer("the answer\n```json\n{confidence: broken}\n```");
        assert_eq!(answer, "the answer");
        assert_eq!(confidence, 0);
    }

    #[test]
    fn unclosed_unrecoverable_metadata_block_is_dropped() {
        let (answer, confidence, _) =
            parse_final_answer("the answer\n```json\n{confidence: still broken");
        assert_eq!(answer, "the answer");
        assert_eq!(confidence, 0);
    }

    #[test]
    fn code_fence_without_metadata_is_kept() {
        // A real code block that merely contains a JSON literal is
        // answer text — the fence survives.
        let text = "example\n```rust\nlet x = {\"a\": 1};\n```";
        let (answer, confidence, _) = parse_final_answer(text);
        assert_eq!(answer, text);
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
