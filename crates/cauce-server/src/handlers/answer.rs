//! `POST /api/answer` (W4-03): the grounded-answer SSE endpoint.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::convert::Infallible;

use axum::Extension;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{
    IntoResponse, Response, Sse,
    sse::{Event, KeepAlive},
};
use cauce_core::{AnswerFrame, AnswerRequest, AnswerRole, AnswerSource, AnswerTurn};
use serde::Deserialize;
use tokio_stream::StreamExt;

use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// Bounds on the client-replayed thread (W7-04): enough turns for a
/// real conversation, small enough that the assembled provider prompt
/// stays sane.
const MAX_HISTORY_TURNS: usize = 20;
/// Total `content` bytes across all replayed turns.
const MAX_HISTORY_BYTES: usize = 64 * 1024;

/// The `POST /api/answer` body; unknown fields are rejected like the
/// query-side `deny_unknown_fields` contract.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnswerBody {
    q: String,
    /// W7-02 Search Assist: the result set the caller already has (the
    /// SERP's top rows). Present — even empty — selects
    /// [`cauce_core::AnswerLoop::stream_assist`]: a single no-tools turn
    /// grounded in these results, never an engine re-fetch. Absent runs
    /// the full tool loop.
    context_results: Option<Vec<AnswerSource>>,
    /// W7-04 conversation threads: the prior completed turns of this
    /// `/answer` thread, oldest first — strict `user`/`assistant` pairs
    /// ending on an assistant turn (the new `q` follows). The page owns
    /// the thread; the loop replays it to the provider verbatim.
    /// Mutually exclusive with `context_results`: assist stays
    /// single-turn.
    history: Option<Vec<AnswerTurn>>,
}

/// The `history` contract (W7-04): a bounded list of completed
/// user→assistant exchanges — even length, `user` on even indices,
/// non-blank bounded content. Anything else is a `bad_request`: a
/// malformed replay would otherwise reach the provider as a mangled
/// conversation (Anthropic also hard-requires the alternation).
fn check_history(ctx: &RequestCtx, history: &[AnswerTurn]) -> Result<(), ApiError> {
    if history.len() > MAX_HISTORY_TURNS {
        return Err(ctx.bad_request(format!("history exceeds {MAX_HISTORY_TURNS} turns")));
    }
    if !history.len().is_multiple_of(2) {
        return Err(ctx.bad_request(
            "history must be complete user/assistant pairs (ends on an assistant turn)",
        ));
    }
    let mut bytes = 0usize;
    for (i, turn) in history.iter().enumerate() {
        let want = if i % 2 == 0 {
            AnswerRole::User
        } else {
            AnswerRole::Assistant
        };
        if turn.role != want {
            return Err(ctx.bad_request(format!(
                "history turns must alternate user/assistant; turn {i} is {:?}",
                turn.role
            )));
        }
        if turn.content.trim().is_empty() {
            return Err(ctx.bad_request(format!("history turn {i} has empty content")));
        }
        bytes += turn.content.len();
    }
    if bytes > MAX_HISTORY_BYTES {
        return Err(ctx.bad_request(format!("history exceeds {} bytes total", MAX_HISTORY_BYTES)));
    }
    Ok(())
}

/// `POST /api/answer` — JSON `{"q": "..."}` in, SSE frames out; a body
/// carrying `context_results` (W7-02) takes the no-tools assist turn
/// instead of the tool loop.
///
/// Every [`AnswerFrame`] from [`cauce_core::AnswerLoop::stream_answer`]
/// or `stream_assist` streams as a named event matching its serde tag
/// (`step` / `delta` / `sources` / `done` / `error`), the
/// `/api/search/stream` convention. Pre-stream rejections keep their
/// real status — 400 `bad_request` for an unparsable body or a blank
/// `q`, 503 `ai_disabled` when `[ai]` is off or the provider client
/// failed at startup — while provider failures mid-loop ride the
/// stream as a terminal `error` event.
///
/// `EventSource` cannot POST, so the `/answer` page drives this route
/// with `fetch` and pins `X-Cauce-Client: ui` itself; every other
/// caller keeps the `/api/*` default client kind.
pub async fn answer(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let Some(loop_) = state.answer() else {
        return Err(ctx.err(
            StatusCode::SERVICE_UNAVAILABLE,
            "ai_disabled",
            "AI answers are disabled; configure [ai] and restart",
        ));
    };
    let body: AnswerBody = serde_json::from_slice(&body)
        .map_err(|e| ctx.bad_request(format!("invalid answer body: {e}")))?;
    let q = body.q.trim();
    if q.is_empty() {
        return Err(ctx.bad_request("missing required parameter \"q\""));
    }
    let history = body.history.unwrap_or_default();
    check_history(&ctx, &history)?;
    if !history.is_empty() && body.context_results.is_some() {
        return Err(ctx.bad_request(
            "context_results and history are mutually exclusive (assist is single-turn)",
        ));
    }
    let req = AnswerRequest {
        q: q.to_string(),
        history,
        client: ctx.client.clone(),
        request_id: Some(ctx.request_id.as_uuid()),
        actor: Some(ctx.actor(&headers)),
    };
    let receiver = match body.context_results {
        Some(results) => loop_.stream_assist(&req, results),
        None => loop_.stream_answer(&req),
    };
    let events = tokio_stream::wrappers::UnboundedReceiverStream::new(receiver).map(|frame| {
        let name = match &frame {
            AnswerFrame::Step { .. } => "step",
            AnswerFrame::Delta { .. } => "delta",
            AnswerFrame::Sources { .. } => "sources",
            AnswerFrame::Done { .. } => "done",
            AnswerFrame::Error { .. } => "error",
        };
        Ok::<Event, Infallible>(
            Event::default()
                .event(name)
                .json_data(&frame)
                .expect("answer frame serializes"),
        )
    });
    Ok(Sse::new(events)
        .keep_alive(KeepAlive::default())
        .into_response())
}
