//! `GET /api/suggest` integration tests (issue #150): the OpenSearch
//! suggestions endpoint the W2-11 descriptor advertises. Completions are
//! `search_log` queries prefix-matched and frecency-ranked by
//! `Store::suggest`, served in the `[term, [completions...]]` shape with
//! `application/x-suggestions+json`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::http::{StatusCode, header};
use cauce_core::{
    CacheKey, ClientKind, EngineId, LogSource, SafeSearch, SearchLogRow, SearchRequest, Store,
};
use chrono::{DateTime, Utc};
use serde_json::json;

mod support;
use support::*;

/// The `CacheKey` an unpinned `/api/search?q=` request produces (mirrors
/// `history_page.rs`'s builder).
fn query_hash(q: &str) -> CacheKey {
    CacheKey::from(&SearchRequest {
        q: q.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::default(),
        engines: None,
        client: ClientKind::Api,
    })
}

fn log_row(ts: DateTime<Utc>, q: &str) -> SearchLogRow {
    SearchLogRow {
        id: None,
        ts,
        query_hash: query_hash(q),
        query: q.to_string(),
        query_raw: Some(q.to_string()),
        client: ClientKind::Api,
        source: LogSource::Network,
        tier: None,
        latency_ms: 12,
        result_count: 10,
        engines: vec![EngineId::from("replay")],
        deadline_hit: false,
    }
}

/// Seeds a frecency-ordered fixture: `sug-beta` logged twice, `sug-alpha`
/// and `sug-gamma` once each, plus one non-matching row.
async fn seed_history(store: &dyn Store) {
    let base = Utc::now();
    for (q, offset_s) in [
        ("sug-beta", -10),
        ("sug-beta", -5),
        ("sug-alpha", 0),
        ("sug-gamma", -1),
        ("other-topic", 0),
    ] {
        store
            .log_search(log_row(base + chrono::Duration::seconds(offset_s), q))
            .await
            .expect("log_search");
    }
}

/// Acceptance: prefix match returns the `[term, [completions...]]` shape,
/// completions ranked most-used first with recency breaking ties, and the
/// OpenSearch content type.
#[tokio::test]
async fn suggest_prefix_match_returns_ranked_completions() {
    let (router, state, _tmp) = app();
    seed_history(state.store().as_ref()).await;

    let (status, headers, body) = get(&router, "/api/suggest?q=sug").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        headers[header::CONTENT_TYPE].to_str().unwrap(),
        "application/x-suggestions+json"
    );
    assert_eq!(
        body,
        json!(["sug", ["sug-beta", "sug-alpha", "sug-gamma"]]),
        "frecency order: most-used first, most recent breaks ties"
    );
}

/// The prefix match is case-insensitive on the stored normalized queries.
#[tokio::test]
async fn suggest_matches_case_insensitively() {
    let (router, state, _tmp) = app();
    seed_history(state.store().as_ref()).await;

    let (status, _, body) = get(&router, "/api/suggest?q=SUG-B").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!(["SUG-B", ["sug-beta"]]));
}

/// A prefix nothing in history starts with yields the term with an empty
/// completions array.
#[tokio::test]
async fn suggest_no_match_returns_empty_completions() {
    let (router, state, _tmp) = app();
    seed_history(state.store().as_ref()).await;

    let (status, _, body) = get(&router, "/api/suggest?q=zzz").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!(["zzz", []]));
}

/// An absent or blank `q` answers `["", []]` — the shape never varies.
#[tokio::test]
async fn suggest_blank_or_absent_q_returns_empty() {
    let (router, state, _tmp) = app();
    seed_history(state.store().as_ref()).await;

    for uri in ["/api/suggest", "/api/suggest?q=", "/api/suggest?q=%20"] {
        let (status, _, body) = get(&router, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        assert_eq!(body, json!(["", []]), "{uri}");
    }
}

/// Completions cap at 10.
#[tokio::test]
async fn suggest_caps_completions_at_ten() {
    let (router, state, _tmp) = app();
    let base = Utc::now();
    for i in 0..12 {
        state
            .store()
            .log_search(log_row(
                base + chrono::Duration::seconds(i64::from(i)),
                &format!("cap-{i:02}"),
            ))
            .await
            .expect("log_search");
    }

    let (status, _, body) = get(&router, "/api/suggest?q=cap").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body[0], json!("cap"));
    assert_eq!(body[1].as_array().unwrap().len(), 10, "{body}");
}

/// Unknown and duplicate query params are the shared 400 contract.
#[tokio::test]
async fn suggest_rejects_unknown_or_duplicate_params() {
    let (router, _state, _tmp) = app();
    for uri in ["/api/suggest?q=x&bogus=1", "/api/suggest?q=x&q=y"] {
        let (status, _, body) = get(&router, uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_envelope(&body, "bad_request");
    }
}

/// The descriptor still advertises the now-real endpoint unchanged
/// (`ui` builds only — `/opensearch.xml` rides the `ui` gate).
#[cfg(feature = "ui")]
#[tokio::test]
async fn opensearch_descriptor_still_advertises_suggest() {
    let (router, _state, _tmp) = app();
    let (status, _, body) = get_headers(&router, "/opensearch.xml").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("application/x-suggestions+json")
            && body.contains("/api/suggest?q={searchTerms}"),
        "descriptor must keep advertising the suggestions Url: {body}"
    );
}
