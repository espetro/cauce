//! cauce-agent: the grounded-answer agent loop (W4-02, parent plan
//! section 4.5 and `.agents/plans/v3/wave-4-ai-mode.md`), extracted
//! behind the harness seam (#230).
//!
//! This crate runs agent workflows and nothing else: per-iteration
//! turn orchestration, the tool registry and dispatch, and
//! termination. It depends on traits and on cauce-core's wire
//! vocabulary — never on axum, SSE, or the HTTP layer.
//!
//! ## The seam
//!
//! This is the point where a third-party agent harness
//! (`rig-core`, `genai`, …) could drop in behind the same public
//! surface:
//!
//! - **In** — [`ChatProvider`] streams one model turn at a time;
//!   [`ToolExecutor`] implementations supply the domain tools the loop
//!   may call (advertised via [`ToolSpec`]); [`LoopConfig`] holds the
//!   loop's knobs (per-turn provider budget, answers-cache TTL,
//!   iteration cap, final-turn tail parser).
//! - **Out** — [`AnswerFrame`], the wire type the SSE layer already
//!   serializes, plus [`AgentObserver`], the hook surface through
//!   which eval/observability concerns plug in.
//!
//! ## The hook model
//!
//! [`AgentObserver::on_event`] mirrors `rig-core`'s `AgentHook`: a
//! single method invoked at every observable point in a run — run
//! start/finish, provider turn boundaries, tool dispatch, and every
//! produced frame — with the [`LoopEvent`] variant and the run-scoped
//! [`RunContext`] (rig's `HookContext` analog: request id, normalized
//! query, client kind). Unlike rig's `Flow` return, events here are
//! observe-only — today nothing needs to veto or rewrite the loop's
//! course, so hooks cannot change behavior by construction.
//! Transcript capture for evals, metrics, and request tracing plug in
//! from outside through this trait; the default [`NoopObserver`] is
//! zero-cost, while [`AnswerLoop::new`] installs [`TracingObserver`]
//! to keep the existing `answer.stream`/`answer.assist` spans and
//! cache-failure warnings.
//!
//! Provider-level observability (`cauce_ai_*` metrics, the
//! `ai.provider_call` audit rows, `ai_http` spans) stays where it
//! lives — inside the provider clients in `cauce-core::ai` — so it
//! keeps working under any harness swapped in here.
//!
//! ## Loop shape
//!
//! [`AnswerLoop`] is already model-driven rather than a scripted
//! pipeline: each iteration the provider decides whether to emit text
//! or tool calls, the loop executes the calls it chose, and the cycle
//! repeats until the model answers plainly or a bound trips — the
//! ReAct shape (reason → act → observe → repeat). The loop itself
//! only orchestrates and enforces safety bounds: [`LoopConfig`]'s
//! per-turn provider budget, the #231 guards (split turn/search
//! budgets, per-tool query dedup, a forced-synthesize turn on
//! exhaustion), and termination on a tool-free completion. The
//! architecture does not preclude deeper autonomy. `run_assist` is
//! deliberately single-turn: the SERP grounding set is fixed inline,
//! so there are no tools to reason over.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod answer;
mod config;

mod observer;
mod tools;

pub use answer::AnswerLoop;
pub use cauce_core::ai::grounded::groundedness;
pub use config::{
    DEFAULT_ANSWERS_TTL, DEFAULT_MAX_SEARCH_EXECUTIONS, DEFAULT_MAX_TURNS, DEFAULT_PROVIDER_BUDGET,
    LoopConfig,
};
pub use observer::{AgentObserver, LoopEvent, NoopObserver, RunContext, RunKind, TracingObserver};
pub use tools::{SearchArchive, SearchWeb, ToolCtx, ToolExecutor, ToolOutput};

// The seam vocabulary, re-exported so embedders have a single import
// site behind which a third-party harness could swap.
pub use cauce_core::ai::{
    AiCallCtx, AiError, AiStreamEvent, AnswerFrame, AnswerRequest, AnswerRole, AnswerTurn,
    ChatCompletion, ChatMessage, ChatProvider, ChatRequest, ToolCall, ToolSpec, Usage,
};
pub use cauce_core::{AnswerSource, ClientKind, SearchResult, Store};
