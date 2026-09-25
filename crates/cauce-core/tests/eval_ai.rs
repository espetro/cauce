//! W4-04 acceptance for the `cauce eval ai` machinery: a grounded case
//! over a recorded transcript plus a replay cassette scores 1.0 against
//! the committed baseline; the deliberately broken tail parser leaves
//! the metadata tail in the answer body and drops the same case to
//! `2/3` — below `baseline - tolerance`, so the CI gate would fail.
//! Also covered: an error turn ends the case as a 0-scored outcome with
//! the provider message as its note.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::path::PathBuf;
use std::sync::Arc;

use cauce_core::evals::ai::{
    AiEvalCase, AiThresholds, Transcript, TranscriptCompletion, TranscriptProvider, TranscriptTurn,
    gate_ok, score_frames,
};
use cauce_core::{
    AnswerFrame, AnswerLoop, AnswerRequest, ChatProvider, ClientKind, Engine, EngineId,
    SearchPipeline, SearchResult, Store,
};
use cauce_engines::cassette::{Cassette, cassette_path};
use cauce_engines::{Replay, ReplayOpts};
use url::Url;

#[allow(dead_code)]
mod support;
use support::*;

const TOOL_QUERY: &str = "current weather in Tokyo right now";

/// `AnswerLoop::with_tail_parser`'s signature.
type TailParser = fn(&str) -> (String, u8, Vec<String>);

fn tokyo_transcript() -> Transcript {
    Transcript {
        model: "eval-model".to_string(),
        provider: None,
        recorded_at: None,
        turns: vec![
            TranscriptTurn {
                deltas: vec![],
                completion: Some(TranscriptCompletion {
                    id: None,
                    model: None,
                    content: String::new(),
                    tool_calls: vec![cauce_core::evals::ai::TranscriptToolCall {
                        id: "call-1".to_string(),
                        name: "search_web".to_string(),
                        arguments: format!("{{\"query\": \"{TOOL_QUERY}\"}}"),
                    }],
                    finish_reason: Some("tool_calls".to_string()),
                    usage: None,
                }),
                error: None,
            },
            TranscriptTurn {
                deltas: vec![],
                completion: Some(TranscriptCompletion {
                    id: None,
                    model: Some("eval-model".to_string()),
                    content: "Tokyo is **22°C** with **clear skies** [1].\n\
                              {\"confidence\": 8, \"related_questions\": [\"q?\"]}"
                        .to_string(),
                    tool_calls: vec![],
                    finish_reason: Some("stop".to_string()),
                    usage: None,
                }),
                error: None,
            },
        ],
    }
}

fn case() -> AiEvalCase {
    AiEvalCase {
        query: "tokyo weather".to_string(),
        transcript: "tokyo".to_string(),
        must_cite_domains: vec!["jma.go.jp".to_string()],
        must_contain: vec!["22".to_string(), "clear skies".to_string()],
        must_not_contain: vec!["related_questions".to_string()],
        tags: vec!["smoke".to_string()],
    }
}

fn thresholds() -> AiThresholds {
    AiThresholds {
        baseline: 1.0,
        tolerance: 0.2,
        ungrounded_max: Some(0.5),
    }
}

/// Write the cassette serving `TOOL_QUERY` under `fixtures/replay/`.
fn write_cassette(fixtures: &std::path::Path) {
    let results = vec![
        SearchResult {
            url: Url::parse("https://www.jma.go.jp/bosai/forecast.html").unwrap(),
            title: "JMA forecast".to_string(),
            snippet: "official forecast".to_string(),
            engine: EngineId::from("replay"),
            published: None,
            score: 1.0,
        },
        SearchResult {
            url: Url::parse("https://www.timeanddate.com/weather/japan/tokyo").unwrap(),
            title: "Tokyo weather".to_string(),
            snippet: "conditions".to_string(),
            engine: EngineId::from("replay"),
            published: None,
            score: 0.5,
        },
    ];
    let path = cassette_path(fixtures, "replay", TOOL_QUERY);
    Cassette::new(EngineId::from("replay"), TOOL_QUERY, results)
        .save(&path)
        .unwrap();
}

async fn run_frames(
    transcript: Transcript,
    fixtures: PathBuf,
    tail_parser: Option<TailParser>,
) -> Vec<AnswerFrame> {
    let store: Arc<dyn Store> = Arc::new(StubStore::default());
    let replay: Arc<dyn Engine> = Arc::new(Replay::new(ReplayOpts {
        id: EngineId::from("replay"),
        cassette_engine: Some(EngineId::from("replay")),
        fixtures_root: fixtures,
        ..Default::default()
    }));
    let provider: Arc<dyn ChatProvider> = Arc::new(TranscriptProvider::new(transcript));
    let pipeline = SearchPipeline::new(store.clone(), vec![replay]);
    let mut loop_ = AnswerLoop::new(pipeline, provider, store);
    if let Some(f) = tail_parser {
        loop_ = loop_.with_tail_parser(f);
    }
    let req = AnswerRequest {
        q: "tokyo weather".to_string(),
        client: ClientKind::Cli,
        request_id: None,
        actor: None,
    };
    let mut rx = loop_.stream_answer(&req);
    let mut frames = Vec::new();
    while let Some(frame) = rx.recv().await {
        frames.push(frame);
    }
    frames
}

#[tokio::test]
async fn grounded_case_scores_full_and_passes_gate() {
    let dir = tempfile::tempdir().unwrap();
    write_cassette(dir.path());
    let frames = run_frames(tokyo_transcript(), dir.path().to_path_buf(), None).await;
    let outcome = score_frames(&case(), &frames);

    assert_eq!(outcome.score, 1.0, "outcome: {outcome:?}");
    assert!(outcome.ok);
    assert!(!outcome.ungrounded);
    assert_eq!(
        outcome.cited_hosts,
        vec![
            "www.jma.go.jp".to_string(),
            "www.timeanddate.com".to_string()
        ]
    );
    assert!(gate_ok(outcome.score, 0.0, &thresholds()));
}

/// The deliberately broken tail parser: it returns the raw text
/// untouched, so `{"confidence":..., "related_questions":...}` stays in
/// `done.answer` — `must_not_contain` catches the leak and the case
/// drops to 2/3, below `baseline - tolerance` (0.8).
#[tokio::test]
async fn broken_tail_parser_drops_score_below_baseline() {
    fn broken_tail_parser(text: &str) -> (String, u8, Vec<String>) {
        (text.trim_end().to_string(), 0, vec![])
    }

    let dir = tempfile::tempdir().unwrap();
    write_cassette(dir.path());
    let frames = run_frames(
        tokyo_transcript(),
        dir.path().to_path_buf(),
        Some(broken_tail_parser),
    )
    .await;
    let outcome = score_frames(&case(), &frames);

    let t = thresholds();
    assert!(
        !outcome.ok && outcome.score < t.baseline - t.tolerance,
        "broken tail parser must drop the score below baseline-tolerance: {outcome:?}"
    );
    assert_eq!(outcome.clean_rate, 0.0);
    assert_eq!(
        outcome.leaked_strings,
        vec!["related_questions".to_string()]
    );
    assert!(!gate_ok(outcome.score, 0.0, &t));
}

/// An error turn ends the stream without `done`: the case scores 0 with
/// the provider message as its note — visible, never silently absent.
#[tokio::test]
async fn error_turn_scores_zero_with_note() {
    let mut t = tokyo_transcript();
    t.turns[0].error = Some(cauce_core::evals::ai::TranscriptError {
        kind: "transport".to_string(),
        message: "upstream connection terminated".to_string(),
        status: None,
        retry_after_s: None,
    });
    t.turns[0].completion = None;

    let dir = tempfile::tempdir().unwrap();
    write_cassette(dir.path());
    let frames = run_frames(t, dir.path().to_path_buf(), None).await;
    let outcome = score_frames(&case(), &frames);

    assert_eq!(outcome.score, 0.0);
    assert_eq!(
        outcome.note.as_deref(),
        Some("provider transport error: upstream connection terminated")
    );
}
