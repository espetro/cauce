//! The [`AgentObserver`] seam — eval/observability middleware plugged
//! in from outside the loop (transcript taps, metrics, tracing).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use tracing::{Span, info, info_span, warn};
use uuid::Uuid;

use cauce_core::ai::{AiError, AnswerFrame, ChatCompletion, ChatRequest, ToolCall};
use cauce_core::{ClientKind, StoreError};

use crate::tools::ToolOutput;

/// Which `stream_*` entrypoint a run came through — mirrors the two
/// span shapes the loop emits ([`TracingObserver`] maps 1:1).
#[derive(Debug, Clone, Copy)]
pub enum RunKind {
    /// `stream_answer` — the iterated tool loop.
    Answer,
    /// `stream_assist` — the single-turn grounded compose.
    Assist {
        /// Size of the supplied grounding set (the `sources` span field).
        sources: usize,
    },
}

/// Run-scoped identity handed to every hook call — the analog of
/// `rig-core`'s `HookContext`. One per `stream_*` invocation; carries
/// the normalized query so observers can correlate with request logs
/// without re-parsing the request.
#[derive(Debug, Clone)]
pub struct RunContext {
    /// Request id the run was minted with (`AnswerRequest::request_id`
    /// or a fresh UUIDv7); also stamped on `done.request_id`.
    pub request_id: Uuid,
    /// The `normalize_query`d question — the same normalization the
    /// cache key and the run span use.
    pub query: String,
    /// The inbound surface's client kind (stamped on tool search
    /// requests).
    pub client: ClientKind,
    /// The `stream_*` entrypoint.
    pub kind: RunKind,
}

/// One observable point in a run — borrowed, so observers pay nothing
/// unless they choose to retain data. The loop calls
/// [`AgentObserver::on_event`] at run start/finish, provider turn
/// boundaries, tool dispatch, and for every frame it produces.
#[derive(Debug)]
pub enum LoopEvent<'a> {
    /// The run has started. First event of every run.
    Started,
    /// The run finished — terminal frame sent or receiver dropped.
    /// Last event of every run.
    Finished,
    /// A provider turn is about to open (`chat_stream` call).
    ProviderTurn {
        /// Zero-based turn index (`stream_assist` runs exactly one turn).
        iteration: usize,
        /// The request about to be sent — transcript taps record it
        /// here.
        request: &'a ChatRequest,
    },
    /// A provider turn completed cleanly.
    ProviderCompleted {
        /// Zero-based turn index.
        iteration: usize,
        /// The assembled turn — transcript taps fold in `usage` and
        /// `finish_reason` from here.
        completion: &'a ChatCompletion,
    },
    /// A provider turn failed — `chat_stream` rejected the call or
    /// an `AiStreamEvent::Error` arrived mid-stream. The terminal
    /// `Error` frame follows as a [`LoopEvent::Frame`].
    ProviderFailed {
        /// Zero-based turn index.
        iteration: usize,
        /// The typed provider error.
        error: &'a AiError,
    },
    /// A tool call is about to execute.
    ToolStarted {
        /// The provider's assembled tool call.
        call: &'a ToolCall,
    },
    /// A tool call finished — success or executor-mapped error.
    ToolFinished {
        /// The provider's assembled tool call.
        call: &'a ToolCall,
        /// What the executor produced for the model + source pool.
        output: &'a ToolOutput,
    },
    /// A tool call was short-circuited before dispatch (#231): the
    /// model gets an in-band `{"error": ...}` result instead of a real
    /// execution — the duplicate-query dedup and the search budget
    /// both report here. No `ToolStarted`/`ToolFinished` fire for it.
    ToolSkipped {
        /// The provider's assembled tool call.
        call: &'a ToolCall,
        /// Why it was skipped (`"duplicate query"` /
        /// `"search budget exhausted"`).
        reason: &'static str,
    },
    /// The `answers` cache lookup or write failed; the run continues
    /// (fail-open policy lives in the loop — the observer only sees it).
    CacheFailed {
        /// `lookup` or `write`.
        op: &'static str,
        /// The store error.
        error: &'a StoreError,
    },
    /// A frame is about to be emitted. Fires before `tx.send`, so a
    /// receiver that already dropped still shows in transcripts — the
    /// event reports the frame *produced*, not delivered.
    Frame {
        /// The frame being emitted.
        frame: &'a AnswerFrame,
    },
}

/// The observer/middleware seam — mirrored on `rig-core`'s
/// `AgentHook` (single `on_event` at every point with a run context),
/// reduced to observe-only: hooks can tap every turn, dispatch, and
/// frame but cannot alter the loop's course. Register via
/// [`AnswerLoop::with_observer`](crate::AnswerLoop::with_observer);
/// all methods default to no-ops so a partial impl is cheap.
pub trait AgentObserver: Send + Sync + 'static {
    /// Span the run is spawned under, in the role of today's
    /// `answer.stream`/`answer.assist` `info_span!`s — the loop
    /// instruments its spawned task with it, so provider `ai_http`
    /// spans and warnings keep their run context as parents.
    /// `None` leaves the run uninstrumented.
    fn run_span(&self, _run: &RunContext) -> Option<Span> {
        None
    }

    /// Called at every [`LoopEvent`] point in order.
    fn on_event(&self, _run: &RunContext, _event: &LoopEvent<'_>) {}
}

/// The zero-cost default: every hook is an inlined no-op.
#[derive(Debug, Default)]
pub struct NoopObserver;

impl AgentObserver for NoopObserver {}

/// The parity observer [`AnswerLoop::new`](crate::AnswerLoop::new)
/// installs: emits the `answer.stream`/`answer.assist` run spans and
/// the answers-cache failure warnings the loop carried before the
/// observer seam existed. Behavior is identical to the pre-extraction
/// loop; swap in [`NoopObserver`] (or your own) to silence it.
#[derive(Debug, Default)]
pub struct TracingObserver;

impl AgentObserver for TracingObserver {
    fn run_span(&self, run: &RunContext) -> Option<Span> {
        Some(match run.kind {
            RunKind::Answer => info_span!(
                "answer.stream",
                request_id = %run.request_id,
                query = %run.query,
                client = %run.client,
            ),
            RunKind::Assist { sources } => info_span!(
                "answer.assist",
                request_id = %run.request_id,
                query = %run.query,
                client = %run.client,
                sources = sources,
            ),
        })
    }

    fn on_event(&self, _run: &RunContext, event: &LoopEvent<'_>) {
        match event {
            LoopEvent::CacheFailed { op, error } => match *op {
                "lookup" => warn!(error = %error, "answers lookup failed; continuing uncached"),
                _ => warn!(error = %error, "answers write failed"),
            },
            LoopEvent::ToolSkipped { call, reason } => {
                info!(tool = %call.name, reason = reason, "tool call short-circuited");
            }
            _ => {}
        }
    }
}
