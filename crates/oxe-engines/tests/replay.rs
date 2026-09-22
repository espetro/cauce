//! Acceptance tests for W0-06: `replay` engine and `oxe record` core.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use url::Url;

use oxe_core::{
    ClientKind, Engine, EngineError, EngineId, SafeSearch, SearchRequest, SearchResult, Tier,
    normalize_url,
};
use oxe_engines::cassette::{Cassette, cassette_key, cassette_path};
use oxe_engines::{Replay, ReplayOpts, record};

const BUDGET: Duration = Duration::from_secs(5);

fn req(q: &str) -> SearchRequest {
    SearchRequest {
        q: q.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::Moderate,
        engines: None,
        client: ClientKind::Api,
    }
}

fn replay(fixtures_root: &Path, opts: impl FnOnce(&mut ReplayOpts)) -> Replay {
    let mut o = ReplayOpts {
        fixtures_root: fixtures_root.to_path_buf(),
        ..ReplayOpts::default()
    };
    opts(&mut o);
    Replay::new(o)
}

#[tokio::test]
async fn synthetic_mode_is_deterministic_per_normalized_query() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = replay(tmp.path(), |_| {});

    let a = engine
        .search(&req("tanstack router"), BUDGET)
        .await
        .unwrap();
    // Same normalized query, different surface form: identical results.
    let b = engine
        .search(&req("  TanStack   ROUTER "), BUDGET)
        .await
        .unwrap();
    assert_eq!(a, b);
}

#[tokio::test]
async fn cassette_mode_is_deterministic() {
    let tmp = tempfile::tempdir().unwrap();
    let results = vec![SearchResult {
        url: Url::parse("https://example.com/one").unwrap(),
        title: "one".into(),
        snippet: "first".into(),
        engine: EngineId::from("stub"),
        published: None,
        score: 1.0,
    }];
    Cassette::new(
        EngineId::from("stub"),
        "  Cassette  Query ",
        results.clone(),
    )
    .save(&cassette_path(tmp.path(), "stub", "cassette query"))
    .unwrap();

    let engine = replay(tmp.path(), |_| {});
    let a = engine.search(&req("cassette query"), BUDGET).await.unwrap();
    let b = engine
        .search(&req("CASSETTE   query"), BUDGET)
        .await
        .unwrap();
    assert_eq!(a, results);
    assert_eq!(a, b);
}

#[tokio::test]
async fn fail_every_two_fails_exactly_calls_2_4_6() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = replay(tmp.path(), |o| o.fail_every = 2);

    for call in 1..=6u64 {
        let got = engine.search(&req("q"), BUDGET).await;
        if call % 2 == 0 {
            assert!(
                matches!(got, Err(EngineError::Transport(_))),
                "call {call} should fail with Transport, got {got:?}"
            );
        } else {
            assert!(got.is_ok(), "call {call} should succeed, got {got:?}");
        }
    }
    assert_eq!(engine.call_count(), 6);
}

#[tokio::test]
async fn synthetic_mode_returns_10_results_with_unique_normalized_urls() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = replay(tmp.path(), |_| {});

    let results = engine.search(&req("unique urls"), BUDGET).await.unwrap();
    assert_eq!(results.len(), 10);

    let normalized: HashSet<String> = results
        .iter()
        .map(|r| normalize_url(&r.url).as_str().to_string())
        .collect();
    assert_eq!(normalized.len(), 10, "duplicate normalized url");

    // Real-looking hosts, distinct paths.
    for r in &results {
        assert_eq!(r.url.scheme(), "https");
        assert!(r.url.host_str().unwrap().contains('.'));
        assert!(!r.title.is_empty());
        assert!(!r.snippet.is_empty());
    }
}

#[tokio::test]
async fn latency_ms_sleeps_before_responding() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = replay(tmp.path(), |o| o.latency_ms = 100);

    let start = Instant::now();
    engine.search(&req("slow"), BUDGET).await.unwrap();
    assert!(start.elapsed() >= Duration::from_millis(100));
}

#[tokio::test]
async fn blocked_always_returns_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = replay(tmp.path(), |o| o.blocked = true);

    for _ in 0..3 {
        assert_eq!(
            engine.search(&req("q"), BUDGET).await,
            Err(EngineError::Blocked)
        );
    }
}

#[tokio::test]
async fn empty_returns_zero_results() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = replay(tmp.path(), |o| o.empty = true);
    assert_eq!(engine.search(&req("q"), BUDGET).await.unwrap(), vec![]);
}

#[tokio::test]
async fn page_limit_beyond_returns_no_results() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = replay(tmp.path(), |o| o.page_limit = Some(1));

    assert!(engine.search(&req("q"), BUDGET).await.is_ok());
    let mut page2 = req("q");
    page2.page = 2;
    assert_eq!(
        engine.search(&page2, BUDGET).await,
        Err(EngineError::NoResults)
    );
}

/// Fixed-result engine used to exercise `record` without the network.
struct Stub {
    results: Vec<SearchResult>,
}

#[async_trait]
impl Engine for Stub {
    fn id(&self) -> EngineId {
        EngineId::from("stub")
    }

    fn tier(&self) -> Tier {
        Tier::T1
    }

    fn page_size(&self) -> u8 {
        10
    }

    async fn search(
        &self,
        _req: &SearchRequest,
        _budget: Duration,
    ) -> Result<Vec<SearchResult>, EngineError> {
        Ok(self.results.clone())
    }
}

fn stub_results() -> Vec<SearchResult> {
    vec![
        SearchResult {
            url: Url::parse("https://example.com/alpha").unwrap(),
            title: "alpha".into(),
            snippet: "first result".into(),
            engine: EngineId::from("stub"),
            published: None,
            score: 1.0,
        },
        SearchResult {
            url: Url::parse("https://blog.example.org/beta").unwrap(),
            title: "beta".into(),
            snippet: "second result".into(),
            engine: EngineId::from("stub"),
            published: None,
            score: 0.5,
        },
    ]
}

#[tokio::test]
async fn record_round_trip_writes_cassette_and_replay_serves_it() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = Stub {
        results: stub_results(),
    };

    let path = record(&stub, "  Round   TRIP ", tmp.path())
        .await
        .expect("record writes a cassette");

    // Cassette landed at <out>/<engine>/<sha8(normalized query)>.json.
    let expected = tmp
        .path()
        .join("stub")
        .join(format!("{}.json", cassette_key("round trip")));
    assert_eq!(path, expected);
    assert!(path.is_file());

    let cassette: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(cassette["query"], "round trip");
    assert_eq!(cassette["engine"], "stub");
    assert!(cassette["recorded_at"].is_string());
    assert_eq!(cassette["results"].as_array().unwrap().len(), 2);

    // Replay finds the cassette by scanning <fixtures_root>/<engine>/ and
    // serves exactly the recorded results.
    let engine = replay(tmp.path(), |_| {});
    let served = engine.search(&req("round trip"), BUDGET).await.unwrap();
    assert_eq!(served, stub_results());
    // And it is deterministic across calls.
    assert_eq!(
        engine.search(&req("  ROUND  trip "), BUDGET).await.unwrap(),
        served
    );
}

#[tokio::test]
async fn engine_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = replay(tmp.path(), |_| {});
    assert_eq!(engine.id(), EngineId::from("replay"));
    assert_eq!(engine.tier(), Tier::T1);
    assert_eq!(engine.page_size(), 10);
}

#[test]
fn from_env_reads_oxe_replay_vars() {
    // Safety: nextest runs each test in its own process; these names are not
    // read by any other test in this binary.
    unsafe {
        std::env::set_var("OXE_REPLAY_LATENCY_MS", "42");
        std::env::set_var("OXE_REPLAY_FAIL_EVERY", "3");
        std::env::set_var("OXE_REPLAY_BLOCKED", "1");
        std::env::set_var("OXE_REPLAY_EMPTY", "true");
        std::env::set_var("OXE_REPLAY_PAGE_LIMIT", "4");
    }
    let engine = Replay::from_env();
    let opts = engine.opts();
    assert_eq!(opts.latency_ms, 42);
    assert_eq!(opts.fail_every, 3);
    assert!(opts.blocked);
    assert!(opts.empty);
    assert_eq!(opts.page_limit, Some(4));
}
