//! W4-05 acceptance: the W4-02 `stream_answer` scenarios replayed
//! through the Anthropic Messages-protocol client — the same
//! `AnswerFrame` stream over `POST /v1/messages` SSE (`tool_use` /
//! `text_delta` / `stop_reason`) instead of `/chat/completions`.
//! Covers: two `search_web` turns → 2 `step` frames + deltas +
//! `sources` (2 URLs) + `done`; a no-tool-call stream →
//! `ungrounded=true`; a mid-stream `error` event → partial deltas then
//! `error`; the grounded confident path caching and replaying.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cauce_core::{
    AiConfig, AiProtocol, AnswerFrame, AnswerKey, AnswerLoop, AnswerRequest, AnthropicClient,
    ChatProvider, ClientKind, Engine, EngineError, EngineId, SearchPipeline, SearchRequest,
    SearchResult, Store, Tier,
};
use url::Url;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[allow(dead_code)]
mod support;
use support::*;

const MODEL: &str = "test-answer-anthropic";

const SSE_TOOLCALL: &str = include_str!("../fixtures/ai/anthropic/sse_toolcall.raw");
const SSE_TOOLCALL_2: &str = include_str!("../fixtures/ai/anthropic/sse_toolcall_2.raw");
const SSE_ANSWER: &str = include_str!("../fixtures/ai/anthropic/sse_answer.raw");
const SSE_NOTOOLS: &str = include_str!("../fixtures/ai/anthropic/sse_notools.raw");
const SSE_ERROR: &str = include_str!("../fixtures/ai/anthropic/sse_error.raw");
const SSE_CONFIDENT: &str = include_str!("../fixtures/ai/anthropic/sse_confident.raw");

/// An engine returning the same two canned results on every call —
/// the two sources carried by the `sse_answer` tool results. Distinct
/// `search_web` queries therefore dedupe to exactly these 2 URLs.
struct FixedEngine {
    results: Vec<SearchResult>,
}

#[async_trait]
impl Engine for FixedEngine {
    fn id(&self) -> EngineId {
        EngineId::from("fixed")
    }
    fn tier(&self) -> Tier {
        Tier::T1
    }
    fn page_size(&self) -> u8 {
        10
    }
    async fn search(
        &self,
        _: &SearchRequest,
        _: Duration,
    ) -> Result<Vec<SearchResult>, EngineError> {
        Ok(self.results.clone())
    }
}

fn fixed_engine() -> FixedEngine {
    let result = |url: &str, title: &str, snippet: &str, engine: &str| SearchResult {
        url: Url::parse(url).expect("fixture url parses"),
        title: title.to_string(),
        snippet: snippet.to_string(),
        engine: EngineId::from(engine),
        published: None,
        score: 1.0,
    };
    FixedEngine {
        results: vec![
            result(
                "https://www.jma.go.jp/tokyo",
                "Tokyo Weather - JMA",
                "Tokyo: 22C, clear skies, light wind",
                "jma",
            ),
            result(
                "https://weather.com/tokyo",
                "Weather Tokyo - weather.com",
                "Currently 22 degrees and clear in Tokyo",
                "weather",
            ),
        ],
    }
}

/// wiremock matches in mount order and skips mocks past `up_to_n_times`,
/// so mounts run in call order.
async fn mount_sse(server: &MockServer, body: &'static str, times: u64) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .up_to_n_times(times)
        .mount(server)
        .await;
}

fn answer_loop(server: &MockServer, store: Arc<StubStore>) -> AnswerLoop {
    let pipeline = SearchPipeline::new(
        store.clone() as Arc<dyn Store>,
        vec![Arc::new(fixed_engine())],
    );
    let provider: Arc<dyn ChatProvider> = Arc::new(
        AnthropicClient::new(&AiConfig {
            base_url: server.uri(),
            api_key: "sk-ant-test".to_string(),
            model: MODEL.to_string(),
            enabled: true,
            protocol: AiProtocol::Anthropic,
        })
        .expect("client builds"),
    );
    AnswerLoop::new(pipeline, provider, store)
}

fn req(q: &str, request_id: Uuid) -> AnswerRequest {
    AnswerRequest {
        q: q.to_string(),
        history: Vec::new(),
        client: ClientKind::Api,
        request_id: Some(request_id),
        actor: None,
    }
}

async fn drain(rx: tokio::sync::mpsc::UnboundedReceiver<AnswerFrame>) -> Vec<AnswerFrame> {
    let mut frames = Vec::new();
    let mut rx = rx;
    while let Some(frame) = rx.recv().await {
        frames.push(frame);
    }
    frames
}

fn steps(frames: &[AnswerFrame]) -> Vec<(String, String, String)> {
    frames
        .iter()
        .filter_map(|f| match f {
            AnswerFrame::Step { tool, query, label } => {
                Some((tool.clone(), query.clone(), label.clone()))
            }
            _ => None,
        })
        .collect()
}

fn delta_text(frames: &[AnswerFrame]) -> String {
    frames
        .iter()
        .filter_map(|f| match f {
            AnswerFrame::Delta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Acceptance (W4-02 replayed over the Anthropic protocol):
/// `tool_use` turn + second tool call + grounded answer ⇒ 2 `step`
/// frames, deltas, `sources` with the 2 fixture URLs, `done` — and the
/// second request's body carries the `tool_use`/`tool_result` echo in
/// the Messages-API shape.
#[tokio::test]
async fn tool_loop_yields_steps_deltas_sources_and_done() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_TOOLCALL, 1).await;
    mount_sse(&server, SSE_TOOLCALL_2, 1).await;
    mount_sse(&server, SSE_ANSWER, 1).await;

    let store = Arc::new(StubStore::default());
    let loop_ = answer_loop(&server, store.clone());
    let request_id = Uuid::now_v7();
    let frames =
        drain(loop_.stream_answer(&req("current weather in Tokyo right now", request_id))).await;

    let steps = steps(&frames);
    assert_eq!(
        steps.len(),
        2,
        "two tool calls = two step frames: {frames:?}"
    );
    assert_eq!(steps[0].0, "search_web");
    assert_eq!(steps[0].1, "current weather in Tokyo right now");
    assert!(steps[0].2.contains("current weather in Tokyo right now"));
    assert_eq!(steps[1].1, "tokyo hourly weather forecast");

    let text = delta_text(&frames);
    assert!(
        text.contains("Tokyo's current weather is **22°C**"),
        "streamed answer text: {text:?}"
    );

    let sources = frames
        .iter()
        .find_map(|f| match f {
            AnswerFrame::Sources { sources } => Some(sources.clone()),
            _ => None,
        })
        .expect("a sources frame must precede done");
    let urls: Vec<_> = sources.iter().map(|s| s.url.as_str()).collect();
    assert_eq!(
        urls,
        ["https://www.jma.go.jp/tokyo", "https://weather.com/tokyo"],
        "deduped union of both tool calls, first-seen order"
    );

    let done = frames
        .iter()
        .find_map(|f| match f {
            AnswerFrame::Done { .. } => Some(f.clone()),
            _ => None,
        })
        .expect("terminal done frame");
    let AnswerFrame::Done {
        answer,
        confidence,
        model,
        related_questions,
        cached,
        request_id: got_id,
        ungrounded,
    } = done
    else {
        unreachable!()
    };
    assert!(!cached && !ungrounded);
    assert_eq!(got_id, request_id);
    assert_eq!(confidence, 0, "fixture answer carries no metadata tail");
    assert_eq!(model, "claude-sonnet-4-20250514");
    assert!(related_questions.is_empty());
    assert!(answer.contains("22°C"));

    // Three provider calls; the second request echoes the first turn in
    // Messages-API shape: assistant `tool_use` block + `tool_result`
    // inside the next `user` message.
    let requests = server.received_requests().await.expect("request log");
    assert_eq!(requests.len(), 3);
    let second: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(second["messages"][1]["role"], "assistant");
    assert_eq!(second["messages"][1]["content"][0]["type"], "tool_use");
    assert_eq!(
        second["messages"][1]["content"][0]["id"],
        "toolu_01A09q90qw90lq917835lq9"
    );
    assert_eq!(second["messages"][2]["role"], "user");
    assert_eq!(second["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(
        second["messages"][2]["content"][0]["tool_use_id"],
        "toolu_01A09q90qw90lq917835lq9"
    );

    // Confidence 0 is below the cache floor — nothing was written.
    let key = AnswerKey::new("current weather in Tokyo right now", MODEL);
    assert!(
        store.get_answer(&key).await.unwrap().is_none(),
        "sub-floor confidence must not be cached"
    );
}

/// A stream with no `tool_use` block ⇒ `done.ungrounded` and no
/// `answers` row.
#[tokio::test]
async fn no_tool_calls_yields_ungrounded_and_no_answers_row() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_NOTOOLS, 1).await;

    let store = Arc::new(StubStore::default());
    let loop_ = answer_loop(&server, store.clone());
    let frames = drain(loop_.stream_answer(&req("what is rust?", Uuid::now_v7()))).await;

    assert!(steps(&frames).is_empty(), "no tool calls = no step frames");
    let sources = frames
        .iter()
        .find_map(|f| match f {
            AnswerFrame::Sources { sources } => Some(sources.len()),
            _ => None,
        })
        .expect("a sources frame must precede done");
    assert_eq!(sources, 0);

    let done = frames
        .iter()
        .find_map(|f| match f {
            AnswerFrame::Done { .. } => Some(f.clone()),
            _ => None,
        })
        .expect("terminal done frame");
    let AnswerFrame::Done {
        answer,
        confidence,
        ungrounded,
        cached,
        related_questions,
        ..
    } = done
    else {
        unreachable!()
    };
    assert!(ungrounded, "no sources ⇒ ungrounded=true");
    assert!(!cached);
    assert_eq!(confidence, 6, "metadata tail parsed");
    assert_eq!(
        answer,
        "Rust is a systems programming language focused on safety and concurrency."
    );
    assert_eq!(
        related_questions,
        [
            "What is ownership in Rust?",
            "How does Rust prevent data races?"
        ]
    );

    let key = AnswerKey::new("what is rust?", MODEL);
    assert!(
        store.get_answer(&key).await.unwrap().is_none(),
        "ungrounded answers must not be cached"
    );
}

/// A mid-stream `error` event yields the partial `text_delta`s emitted
/// so far, then an `error` frame.
#[tokio::test]
async fn mid_stream_error_event_yields_partial_deltas_then_error() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_ERROR, 1).await;

    let store = Arc::new(StubStore::default());
    let loop_ = answer_loop(&server, store.clone());
    let frames = drain(loop_.stream_answer(&req("tokyo weather?", Uuid::now_v7()))).await;

    let text = delta_text(&frames);
    assert_eq!(
        text.trim_end(),
        "Tokyo's weather is currently quite warm, with temperatures reaching about",
        "the partial stream is flushed before the error"
    );
    assert!(
        frames
            .iter()
            .all(|f| !matches!(f, AnswerFrame::Done { .. } | AnswerFrame::Sources { .. })),
        "no done/sources frames on error: {frames:?}"
    );
    match frames.last() {
        Some(AnswerFrame::Error {
            message,
            retry_after_s,
        }) => {
            assert!(
                message.contains("upstream connection terminated"),
                "{message}"
            );
            assert_eq!(*retry_after_s, None);
        }
        other => panic!("expected terminal error frame, got {other:?}"),
    }
}

/// A grounded + confident (>= 4) answer is written to `answers`; the
/// next identical (normalized query, model) call replays `sources` then
/// `done{cached:true}` without touching the provider.
#[tokio::test]
async fn grounded_confident_answer_is_cached_and_replayed() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_TOOLCALL, 1).await;
    mount_sse(&server, SSE_CONFIDENT, 1).await;

    let store = Arc::new(StubStore::default());
    let loop_ = answer_loop(&server, store.clone());
    let q = "current weather in Tokyo right now";
    let first = drain(loop_.stream_answer(&req(q, Uuid::now_v7()))).await;
    let done = first
        .iter()
        .find_map(|f| match f {
            AnswerFrame::Done { .. } => Some(f.clone()),
            _ => None,
        })
        .expect("terminal done frame");
    let AnswerFrame::Done {
        confidence, cached, ..
    } = done
    else {
        unreachable!()
    };
    assert!(!cached && confidence == 8);

    let row = store
        .get_answer(&AnswerKey::new(q, MODEL))
        .await
        .unwrap()
        .expect("grounded + confident answer is cached");
    assert_eq!(
        row.payload.answer,
        "Tokyo's current weather is **22°C** with **clear skies** and **light wind** [1][2]."
    );
    assert_eq!(row.payload.confidence, 8);
    assert_eq!(row.payload.related_questions.len(), 2);
    assert_eq!(row.sources.len(), 2);

    // Replay: sources then done{cached:true}, zero new provider calls.
    let replay_id = Uuid::now_v7();
    let second =
        drain(loop_.stream_answer(&req("  Current Weather In Tokyo Right Now  ", replay_id))).await;
    assert_eq!(second.len(), 2, "replay is sources + done: {second:?}");
    assert!(matches!(&second[0], AnswerFrame::Sources { sources } if sources.len() == 2));
    let AnswerFrame::Done {
        cached,
        confidence,
        request_id: got_id,
        ..
    } = &second[1]
    else {
        panic!("replay ends in done: {second:?}")
    };
    assert!(*cached && *confidence == 8 && *got_id == replay_id);

    let calls = server.received_requests().await.expect("request log");
    assert_eq!(calls.len(), 2, "cache hit makes no provider call");
}

/// The loop stops after `max_iterations` `tool_use` turns and ends in
/// an `error` frame (settled cap: 5).
#[tokio::test]
async fn tool_calls_beyond_max_iterations_end_in_error() {
    let server = MockServer::start().await;
    // No cap: every call returns another tool_use block.
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SSE_TOOLCALL, "text/event-stream"))
        .mount(&server)
        .await;

    let store = Arc::new(StubStore::default());
    let loop_ = answer_loop(&server, store.clone());
    let frames = drain(loop_.stream_answer(&req("loop forever", Uuid::now_v7()))).await;

    assert_eq!(steps(&frames).len(), 5, "one step frame per iteration");
    match frames.last() {
        Some(AnswerFrame::Error { message, .. }) => {
            assert!(message.contains("5"), "cap is named: {message}");
        }
        other => panic!("expected terminal error frame, got {other:?}"),
    }
    let calls = server.received_requests().await.expect("request log");
    assert_eq!(calls.len(), 5, "the loop called the provider 5 times");
}
