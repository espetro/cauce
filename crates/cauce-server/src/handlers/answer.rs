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
use cauce_core::{AnswerFrame, AnswerRequest};
use serde::Deserialize;
use tokio_stream::StreamExt;

use crate::app::AppState;
use crate::error::ApiError;
use crate::middleware::RequestCtx;

/// The `POST /api/answer` body; unknown fields are rejected like the
/// query-side `deny_unknown_fields` contract.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnswerBody {
    q: String,
}

/// `POST /api/answer` — JSON `{"q": "..."}` in, SSE frames out.
///
/// Every [`AnswerFrame`] from [`cauce_core::AnswerLoop::stream_answer`]
/// streams as a named event matching its serde tag (`step` / `delta` /
/// `sources` / `done` / `error`), the `/api/search/stream` convention.
/// Pre-stream rejections keep their real status — 400 `bad_request` for
/// an unparsable body or a blank `q`, 503 `ai_disabled` when `[ai]` is
/// off or the provider client failed at startup — while provider
/// failures mid-loop ride the stream as a terminal `error` event.
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
    let receiver = loop_.stream_answer(&AnswerRequest {
        q: q.to_string(),
        client: ctx.client.clone(),
        request_id: Some(ctx.request_id.as_uuid()),
        actor: Some(ctx.actor(&headers)),
    });
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
