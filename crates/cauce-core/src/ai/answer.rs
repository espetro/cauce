//! The `/answer` wire contract (W4-02, parent plan section 4.5 and
//! `.agents/plans/v3/wave-4-ai-mode.md`): the [`AnswerFrame`] stream
//! union the SSE layer serializes — `step` per tool call, `delta` as
//! answer text streams, `sources` once the cited set is known, then
//! terminal `done` or `error` — plus the [`AnswerRequest`]/
//! [`AnswerTurn`] inputs.
//!
//! The loop that produces these frames lives in the `cauce-agent`
//! crate (#230), behind its provider/tool/observer seam; the shapes
//! here stay in cauce-core because they are the settled wire contract
//! and are shared with `evals` (`score_frames` consumes
//! [`AnswerFrame`]s, `TranscriptProvider`/`RecordingProvider`
//! implement [`ChatProvider`](super::ChatProvider)).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::request::ClientKind;
use crate::store::AnswerSource;

/// One item of the `stream_answer` channel — the settled W4-02 wire
/// shapes, serde-tagged on `type` so SSE is `data: {"type": ...}`.
#[derive(Debug, Clone, Serialize, TS)]
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
        /// `answer` rendered server-side to sanitized HTML (#226): the
        /// web bundle injects it verbatim and retargets its
        /// `<a class="cite" data-cite="n">` placeholders to that
        /// turn's source cards; `answer` stays as the markdown source
        /// (thread replay, debug, text clients).
        html: String,
        confidence: u8,
        model: String,
        related_questions: Vec<String>,
        /// `true` only on the `answers`-table replay.
        cached: bool,
        /// The inbound request id (supplied or minted UUIDv7).
        request_id: Uuid,
        /// `true` when the answer drew on no sources; absent otherwise.
        /// (`default` is inert — `AnswerFrame` is never deserialized — but
        /// tells ts-rs the field may be absent on the wire.)
        #[serde(default, skip_serializing_if = "is_false")]
        ungrounded: bool,
        /// The durable `answer_log` row id written for this run (#254) —
        /// the page `replaceState`s `/answer/{log_id}` so back/forward
        /// re-reads the stored row instead of re-running the loop.
        /// Absent when the log write failed open.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(type = "number")]
        log_id: Option<i64>,
    },
    /// Terminal frame for failures that abort the stream before a `done`
    /// can be built — provider error, iterations exhausted.
    Error {
        message: String,
        /// Provider retry hint, only on rate limits.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(type = "number")]
        retry_after_s: Option<u64>,
        /// The `answer_log` row written for this failed run (#254) —
        /// same `/answer/{id}` swap contract as `done.log_id`. Absent
        /// on `stream_assist` (which never logs) and on failed writes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(type = "number")]
        log_id: Option<i64>,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum AnswerRole {
    User,
    Assistant,
}

/// One prior turn of an `/answer` thread (W7-04): `{role, content}` as
/// the page echoes it back — `content` is the submitted question for
/// `User`, the `done.answer` text (metadata tail already stripped) for
/// `Assistant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AnswerTurn {
    pub role: AnswerRole,
    pub content: String,
}
