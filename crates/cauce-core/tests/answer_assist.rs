//! W7-02 acceptance: `stream_assist` — the single no-tools turn the
//! SERP's Assist card drives. Covers: `sources` (the supplied set) →
//! `delta`s → `done` with no `step` frames; the provider request
//! carries `tool_choice: "none"` and no tools; the pipeline/engine is
//! never touched (no engine re-fetch); the `answers` write lands under
//! the `AnswerKey::assist` key (never colliding with a tool-loop
//! answer for the same `q`); grounded+confident replays
//! `done{cached:true}`; an empty result set answers `ungrounded` and
//! caches nothing; a mid-stream error flushes partial deltas.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::sync::Arc;

use cauce_core::{
    AiConfig, AnswerFrame, AnswerKey, AnswerLoop, AnswerRequest, AnswerSource, ChatProvider,
    ClientKind, EngineId, OpenAiClient, SearchPipeline, Store,
};
use url::Url;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[allow(dead_code)]
mod support;
use support::*;

const MODEL: &str = "test-answer-loop";

const SSE_ANSWER: &str = include_str!("../fixtures/ai/sse_answer.raw");
const SSE_CONFIDENT: &str = include_str!("../fixtures/ai/sse_confident.raw");
const SSE_NOTOOLS: &str = include_str!("../fixtures/ai/sse_notools.raw");
const SSE_ERROR: &str = include_str!("../fixtures/ai/sse_error.raw");

/// The two-result set the tests ground against (the same URLs the
/// tool-loop fixtures cite).
fn serp_sources() -> Vec<AnswerSource> {
    let src = |url: &str, title: &str, snippet: &str| AnswerSource {
        url: Url::parse(url).expect("fixture url parses"),
        title: title.to_string(),
        snippet: snippet.to_string(),
        engine: EngineId::from("replay"),
    };
    vec![
        src(
            "https://www.jma.go.jp/tokyo",
            "Tokyo Weather - JMA",
            "Tokyo: 22C, clear skies, light wind",
        ),
        src(
            "https://weather.com/tokyo",
            "Weather Tokyo - weather.com",
            "Currently 22 degrees and clear in Tokyo",
        ),
    ]
}

/// An answer loop whose single engine is a `GateEngine` over a synthetic
/// `Replay` — `call_count()` is the no-engine-refetch witness.
fn assist_loop(
    server: &MockServer,
    store: Arc<StubStore>,
) -> (AnswerLoop, Arc<GateEngine>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("fixtures root");
    let engine = Arc::new(GateEngine::new(replay_at(tmp.path(), |_| {}), true));
    let pipeline = SearchPipeline::new(store.clone() as Arc<dyn Store>, vec![engine.clone()]);
    let provider: Arc<dyn ChatProvider> = Arc::new(
        OpenAiClient::new(&AiConfig {
            base_url: server.uri(),
            api_key: "sk-test".to_string(),
            model: MODEL.to_string(),
            enabled: true,
            protocol: cauce_core::AiProtocol::OpenAi,
        })
        .expect("client builds"),
    );
    (AnswerLoop::new(pipeline, provider, store), engine, tmp)
}

fn req(q: &str) -> AnswerRequest {
    AnswerRequest {
        q: q.to_string(),
        history: Vec::new(),
        client: ClientKind::Ui,
        request_id: Some(Uuid::now_v7()),
        actor: None,
    }
}

/// wiremock matches in mount order and skips mocks past `up_to_n_times`,
/// so mounts run in call order (same convention as answer.rs).
async fn mount_sse(server: &MockServer, body: &'static str, times: u64) {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .up_to_n_times(times)
        .mount(server)
        .await;
}

async fn drain(rx: tokio::sync::mpsc::UnboundedReceiver<AnswerFrame>) -> Vec<AnswerFrame> {
    let mut frames = Vec::new();
    let mut rx = rx;
    while let Some(frame) = rx.recv().await {
        frames.push(frame);
    }
    frames
}

fn done_of(frames: &[AnswerFrame]) -> AnswerFrame {
    frames
        .iter()
        .find(|f| matches!(f, AnswerFrame::Done { .. }))
        .cloned()
        .expect("terminal done frame")
}

/// Acceptance: `sources` carries the supplied set first, then deltas,
/// then `done` — and the provider turn went out with no tools and
/// `tool_choice: "none"`. The engine's call count proves no re-fetch.
#[tokio::test]
async fn assist_streams_supplied_sources_then_deltas_then_done() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_ANSWER, 1).await;

    let store = Arc::new(StubStore::default());
    let (loop_, engine, _tmp) = assist_loop(&server, store);
    let sources = serp_sources();
    let frames =
        drain(loop_.stream_assist(&req("current weather in Tokyo right now"), sources.clone()))
            .await;

    assert!(
        frames
            .iter()
            .all(|f| !matches!(f, AnswerFrame::Step { .. })),
        "a no-tools turn emits no step frames: {frames:?}"
    );
    let AnswerFrame::Sources { sources: got } = &frames[0] else {
        panic!("the supplied sources lead the stream: {frames:?}")
    };
    assert_eq!(*got, sources, "the sources frame echoes the SERP set");
    assert!(
        frames
            .iter()
            .any(|f| matches!(f, AnswerFrame::Delta { .. })),
        "deltas stream between sources and done"
    );
    let AnswerFrame::Done {
        answer,
        ungrounded,
        cached,
        ..
    } = done_of(&frames)
    else {
        unreachable!()
    };
    assert!(answer.contains("22°C"), "answer text: {answer:?}");
    assert!(!ungrounded, "a supplied set is never ungrounded");
    assert!(!cached);

    assert_eq!(
        engine.call_count(),
        0,
        "assist never fans out to the pipeline"
    );

    let requests = server.received_requests().await.expect("request log");
    assert_eq!(requests.len(), 1, "one provider call, no tool loop");
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["tool_choice"], "none");
    assert!(
        body.get("tools").is_none() || body["tools"].as_array().unwrap().is_empty(),
        "no tools offered: {body}"
    );
    let user = body["messages"][1]["content"].as_str().unwrap();
    assert!(user.contains("current weather in Tokyo right now"));
    assert!(
        user.contains("https://www.jma.go.jp/tokyo") && user.contains("https://weather.com/tokyo"),
        "the result set is serialized into the user turn: {user}"
    );
}

/// The assist key carries the result set: it differs from the
/// tool-loop key for the same `q`, and a changed top-K is a miss.
#[test]
fn assist_key_separates_from_tool_loop_and_tracks_results() {
    let sources = serp_sources();
    let key = AnswerKey::assist("what is rust", MODEL, &sources);
    assert_ne!(key, AnswerKey::new("what is rust", MODEL));
    assert_eq!(
        key,
        AnswerKey::assist("  What Is Rust ", MODEL, &sources),
        "query normalization still applies"
    );
    let mut fewer = sources.clone();
    fewer.pop();
    assert_ne!(
        key,
        AnswerKey::assist("what is rust", MODEL, &fewer),
        "a different result set is a different key"
    );
    let mut swapped = sources.clone();
    swapped.swap(0, 1);
    assert_ne!(
        key,
        AnswerKey::assist("what is rust", MODEL, &swapped),
        "order is part of the grounding set"
    );
}

/// Grounded + confident (>= 4) assist answers cache under the assist
/// key and replay `sources` → `done{cached:true}` with no provider
/// call; the tool-loop key for the same `q` stays empty.
#[tokio::test]
async fn assist_grounded_confident_is_cached_and_replayed() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_CONFIDENT, 1).await;

    let store = Arc::new(StubStore::default());
    let (loop_, _engine, _tmp) = assist_loop(&server, store.clone());
    let q = "current weather in Tokyo right now";
    let sources = serp_sources();
    let frames = drain(loop_.stream_assist(&req(q), sources.clone())).await;
    let AnswerFrame::Done {
        confidence, cached, ..
    } = done_of(&frames)
    else {
        unreachable!()
    };
    assert!(!cached && confidence == 8);

    let row = store
        .get_answer(&AnswerKey::assist(q, MODEL, &sources))
        .await
        .unwrap()
        .expect("grounded + confident assist is cached");
    assert_eq!(row.payload.confidence, 8);
    assert_eq!(row.sources.len(), 2);
    assert!(
        store
            .get_answer(&AnswerKey::new(q, MODEL))
            .await
            .unwrap()
            .is_none(),
        "the assist write never lands on the tool-loop key"
    );

    let second = drain(loop_.stream_assist(&req(q), sources)).await;
    assert_eq!(second.len(), 2, "replay is sources + done: {second:?}");
    let AnswerFrame::Done { cached, .. } = &second[1] else {
        panic!("replay ends in done: {second:?}")
    };
    assert!(*cached);

    let calls = server.received_requests().await.expect("request log");
    assert_eq!(calls.len(), 1, "cache hit makes no provider call");
}

/// An empty `context_results` is the degenerate assist: the done frame
/// is `ungrounded` and the grounded-only rule writes no row.
#[tokio::test]
async fn assist_empty_context_is_ungrounded_and_uncached() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_NOTOOLS, 1).await;

    let store = Arc::new(StubStore::default());
    let (loop_, engine, _tmp) = assist_loop(&server, store.clone());
    let frames = drain(loop_.stream_assist(&req("what is rust?"), Vec::new())).await;

    let AnswerFrame::Sources { sources } = &frames[0] else {
        panic!("an empty sources frame still leads: {frames:?}")
    };
    assert!(sources.is_empty());
    let AnswerFrame::Done { ungrounded, .. } = done_of(&frames) else {
        unreachable!()
    };
    assert!(ungrounded);
    assert_eq!(engine.call_count(), 0);
    assert!(
        store
            .get_answer(&AnswerKey::assist("what is rust?", MODEL, &[]))
            .await
            .unwrap()
            .is_none(),
        "ungrounded assist is never cached"
    );
}

/// A mid-stream provider error flushes the partial text and ends in an
/// `error` frame — same contract as the tool loop.
#[tokio::test]
async fn assist_mid_stream_error_flushes_then_errors() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_ERROR, 1).await;

    let store = Arc::new(StubStore::default());
    let (loop_, _engine, _tmp) = assist_loop(&server, store);
    let frames = drain(loop_.stream_assist(&req("tokyo weather?"), serp_sources())).await;

    let text: String = frames
        .iter()
        .filter_map(|f| match f {
            AnswerFrame::Delta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        text.contains("Tokyo's weather is currently quite warm"),
        "partial deltas still arrive: {text:?}"
    );
    match frames.last() {
        Some(AnswerFrame::Error { message, .. }) => {
            assert!(
                message.contains("upstream connection terminated"),
                "{message}"
            );
        }
        other => panic!("expected terminal error frame, got {other:?}"),
    }
    assert!(
        frames
            .iter()
            .all(|f| !matches!(f, AnswerFrame::Done { .. })),
        "no done frame on error: {frames:?}"
    );
}

/// The grounding ceiling: more than `ASSIST_MAX_SOURCES` (10) supplied
/// rows are truncated before the prompt and the `sources` frame.
#[tokio::test]
async fn assist_caps_context_at_max_sources() {
    let server = MockServer::start().await;
    mount_sse(&server, SSE_ANSWER, 1).await;

    let store = Arc::new(StubStore::default());
    let (loop_, _engine, _tmp) = assist_loop(&server, store);
    let sources: Vec<AnswerSource> = (0..14)
        .map(|i| AnswerSource {
            url: Url::parse(&format!("https://example.com/{i}")).unwrap(),
            title: format!("result {i}"),
            snippet: format!("snippet {i}"),
            engine: EngineId::from("replay"),
        })
        .collect();
    let frames = drain(loop_.stream_assist(&req("q"), sources)).await;

    let AnswerFrame::Sources { sources } = &frames[0] else {
        panic!("sources frame first: {frames:?}")
    };
    assert_eq!(sources.len(), 10, "sources frame is capped");
    let requests = server.received_requests().await.expect("request log");
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let user = body["messages"][1]["content"].as_str().unwrap();
    assert!(user.contains("example.com/9"), "top-K serialized");
    assert!(
        !user.contains("example.com/10"),
        "rows past the cap never reach the prompt"
    );
}
