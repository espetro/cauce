//! Reusable conformance suite for `Store` implementations.
//!
//! Enabled by the `conformance` cargo feature (default off; `cauce-core` stays
//! dependency-free of test-only code in normal builds). Each `pub async fn`
//! exercises one area of the `Store` contract and panics with context on any
//! violation. `cauce-store-sqlite` (W0-04) calls them against a temp file; the
//! Postgres impl (W6) must pass the same suite.
//!
//! Each function is written to tolerate pre-existing rows (it asserts deltas
//! or membership rather than whole-table equality), so `run_all` can exercise
//! them all against a single store instance.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use chrono::{DateTime, Utc};
use url::Url;
use uuid::Uuid;

use crate::Tier;
use crate::cache::CacheKey;
use crate::engine::EngineId;
use crate::request::{ClientKind, SafeSearch, SearchRequest};
use crate::response::{
    EngineReport, EngineStatus, SearchMeta, SearchResponse, SearchResult, Source,
};
use crate::store::{
    AuditFilter, AuditRow, BreakerState, ClickRow, EngineHealthRow, HistoryFilter, HistoryItem,
    LogSource, SearchLogRow, Store,
};

/// A deterministic request for `CacheKey` generation. The `q` value should be
/// unique per test area so shared-store runs cannot cross-contaminate.
pub fn request(q: &str) -> SearchRequest {
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

/// A `SearchResponse` with `(title, url, snippet)` result tuples produced by a
/// single `conf-engine` report. Deterministic except for `request_id`.
pub fn response(q: &str, results: &[(&str, &str, &str)]) -> SearchResponse {
    let results: Vec<SearchResult> = results
        .iter()
        .enumerate()
        .map(|(i, (title, url, snippet))| SearchResult {
            url: Url::parse(url).expect("fixture url must parse"),
            title: title.to_string(),
            snippet: snippet.to_string(),
            engine: EngineId::from("conf-engine"),
            published: None,
            score: 1.0 - i as f32 * 0.1,
        })
        .collect();
    SearchResponse {
        query: q.to_string(),
        meta: SearchMeta {
            source: Source::Network,
            engines_used: vec![EngineReport {
                engine: EngineId::from("conf-engine"),
                status: EngineStatus::Ok,
                latency_ms: 12,
                result_count: results.len() as u32,
            }],
            engines_skipped: Vec::new(),
            deadline_hit: false,
            elapsed_ms: 12,
            request_id: Uuid::now_v7(),
        },
        results,
    }
}

/// A `SearchLogRow` builder with the fields the suite varies; `id` is `None`
/// as on a real insert.
pub fn log_row(
    ts: DateTime<Utc>,
    query: &str,
    client: ClientKind,
    source: LogSource,
    latency_ms: u32,
    result_count: u32,
) -> SearchLogRow {
    SearchLogRow {
        id: None,
        ts,
        query_hash: CacheKey::from(&request(query)),
        query: query.to_string(),
        query_raw: Some(query.to_string()),
        client,
        source,
        tier: match source {
            LogSource::Cache => Some(Tier::T1),
            LogSource::Network => None,
        },
        latency_ms,
        result_count,
        engines: vec![EngineId::from("conf-engine")],
        deadline_hit: false,
    }
}

/// `put` then `get_exact` round-trips a `SearchResponse`; a second `put`
/// replaces the row.
pub async fn cache_exact_roundtrip(store: &impl Store) {
    let key = CacheKey::from(&request("conformance exact roundtrip"));
    let resp = response(
        "conformance exact roundtrip",
        &[
            ("Alpha result", "https://example.com/alpha", "alpha snippet"),
            ("Beta result", "https://example.com/beta", "beta snippet"),
        ],
    );

    store
        .put(&key, &resp, Duration::from_secs(3600))
        .await
        .expect("put failed");

    let got = store
        .get_exact(&key)
        .await
        .expect("get_exact failed")
        .expect("fresh row must be visible to get_exact");

    assert_eq!(got.key, key);
    assert_eq!(got.query, resp.query);
    assert_eq!(got.response, resp, "payload must round-trip exactly");
    assert!(got.expires_at > Utc::now(), "fresh row must be unexpired");
    assert!(got.hits >= 1, "a served hit must be counted");
    assert!(
        got.engines.iter().any(|e| e.as_str() == "conf-engine"),
        "engines_json must carry the producing engines"
    );

    // Replace: same key, new payload.
    let resp2 = response(
        "conformance exact roundtrip",
        &[("Gamma result", "https://example.com/gamma", "gamma snippet")],
    );
    store
        .put(&key, &resp2, Duration::from_secs(3600))
        .await
        .expect("second put failed");
    let got = store
        .get_exact(&key)
        .await
        .expect("get_exact failed")
        .expect("replaced row must still be visible");
    assert_eq!(got.response, resp2, "put must replace the stored payload");

    // An absurd TTL must clamp to a representable timestamp, not corrupt the
    // row (saturating to i64::MAX overflows DateTime::from_timestamp_millis).
    let huge_key = CacheKey::from(&request("conformance exact huge ttl"));
    store
        .put(&huge_key, &resp, Duration::MAX)
        .await
        .expect("put with Duration::MAX failed");
    let got = store
        .get_exact(&huge_key)
        .await
        .expect("get_exact failed")
        .expect("clamped row must decode, not read back Corrupt");
    assert!(got.expires_at > Utc::now());
}

/// Rows past `expires_at` are invisible to `get_exact`, visible to the admin
/// `get_cache`, and removed by `evict_expired`.
pub async fn cache_expiry_and_eviction(store: &impl Store) {
    let key = CacheKey::from(&request("conformance expired row"));
    let resp = response(
        "conformance expired row",
        &[("Old result", "https://example.com/old", "old snippet")],
    );

    // A zero TTL expires immediately.
    store
        .put(&key, &resp, Duration::ZERO)
        .await
        .expect("put failed");

    assert!(
        store
            .get_exact(&key)
            .await
            .expect("get_exact failed")
            .is_none(),
        "expired rows must be invisible to get_exact"
    );
    assert!(
        store
            .get_cache(&key)
            .await
            .expect("get_cache failed")
            .is_some(),
        "admin get_cache must still see expired rows"
    );

    let evicted = store.evict_expired().await.expect("evict_expired failed");
    assert!(evicted >= 1, "evict_expired must report the removed row");

    assert!(
        store
            .get_cache(&key)
            .await
            .expect("get_cache failed")
            .is_none(),
        "evicted row must be gone for good"
    );
}

/// `get_lexical` finds entries by query text, result titles and result
/// snippets (FTS5 over `cache_fts` for the sqlite impl).
pub async fn lexical_search(store: &impl Store) {
    let key = CacheKey::from(&request("conformance lexical"));
    let resp = response(
        "conformance lexical",
        &[
            (
                "TanStack Router documentation",
                "https://tanstack.com/router",
                "type safe routing",
            ),
            (
                "Unrelated result",
                "https://example.com/other",
                "nothing here",
            ),
        ],
    );
    store
        .put(&key, &resp, Duration::from_secs(3600))
        .await
        .expect("put failed");

    // Acceptance: a title mention is found by a lowercase term.
    let hits = store
        .get_lexical("tanstack", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().any(|c| c.key == key),
        "get_lexical(\"tanstack\") must find the row whose title mentions TanStack"
    );

    // Snippet-only term also matches.
    let hits = store
        .get_lexical("routing", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().any(|c| c.key == key),
        "get_lexical must index result snippets"
    );

    // Query-column term also matches.
    let hits = store
        .get_lexical("conformance", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().any(|c| c.key == key),
        "get_lexical must index the stored query"
    );

    // A term present nowhere yields no rows.
    let hits = store
        .get_lexical("qzxwvunmatchable", 10)
        .await
        .expect("get_lexical failed");
    assert!(hits.is_empty(), "gibberish term must match nothing");

    // Punctuation-only input is an empty result, not an FTS syntax error.
    let hits = store
        .get_lexical("*", 10)
        .await
        .expect("get_lexical('*') failed");
    assert!(hits.is_empty(), "operator-only input must not error");

    // Pinned semantic: get_lexical DOES return expired rows (callers check
    // CachedSearch::expires_at to serve or mark stale). W6 Postgres must match.
    let expired_key = CacheKey::from(&request("conformance lexical expired"));
    store
        .put(
            &expired_key,
            &response(
                "conformance lexical expired",
                &[("stale zephyr hit", "https://stale.example.com/", "x")],
            ),
            Duration::ZERO,
        )
        .await
        .expect("put expired");
    let hits = store
        .get_lexical("zephyr", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter()
            .any(|c| c.key == expired_key && c.expires_at <= Utc::now()),
        "get_lexical must return expired rows (stale serving is the caller's call)"
    );

    // A zero-result response is still indexed (its query column) and the FTS
    // row carries the cache_entries rowid, not a phantom.
    let empty_key = CacheKey::from(&request("conformance lexical empty"));
    store
        .put(
            &empty_key,
            &response("conformance lexical empty", &[]),
            Duration::from_secs(3600),
        )
        .await
        .expect("put empty");
    let hits = store
        .get_lexical("empty", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().any(|c| c.key == empty_key),
        "empty-results rows must be indexed and found via their query"
    );

    // Lexical results must survive the source row being deleted.
    assert!(store.delete_cache(&key).await.expect("delete_cache failed"));
    let hits = store
        .get_lexical("tanstack", 10)
        .await
        .expect("get_lexical failed");
    assert!(
        hits.iter().all(|c| c.key != key),
        "deleted rows must leave the lexical index"
    );
}

/// Cache admin surface: `list_cache` (newest first, includes expired),
/// `get_cache`, `delete_cache`, `clear_cache`.
pub async fn cache_admin(store: &impl Store) {
    let k1 = CacheKey::from(&request("conformance admin a"));
    let k2 = CacheKey::from(&request("conformance admin b"));
    let r1 = response(
        "conformance admin a",
        &[("A", "https://a.example.com/", "a")],
    );
    let r2 = response(
        "conformance admin b",
        &[("B", "https://b.example.com/", "b")],
    );

    store
        .put(&k1, &r1, Duration::from_secs(3600))
        .await
        .expect("put k1");
    // k2 is already expired: list_cache must still include it.
    store.put(&k2, &r2, Duration::ZERO).await.expect("put k2");

    let all = store.list_cache(50, 0).await.expect("list_cache failed");
    let p1 = all.iter().position(|c| c.key == k1).expect("k1 listed");
    let p2 = all
        .iter()
        .position(|c| c.key == k2)
        .expect("expired k2 listed");
    assert!(
        p2 < p1,
        "list_cache must be newest-first (k2 inserted after k1)"
    );

    // Pagination: page two continues where page one stopped.
    let page0 = store.list_cache(1, 0).await.expect("list_cache page0");
    let page1 = store.list_cache(1, 1).await.expect("list_cache page1");
    assert_eq!(page0.len(), 1);
    assert_eq!(page1.len(), 1);
    assert_eq!(page0[0].key, all[0].key);
    assert_eq!(page1[0].key, all[1].key);

    assert!(store.delete_cache(&k1).await.expect("delete k1"));
    assert!(
        !store.delete_cache(&k1).await.expect("delete missing"),
        "deleting a missing key must return false"
    );

    let removed = store.clear_cache().await.expect("clear_cache failed");
    assert!(removed >= 1, "clear_cache must report removed rows");
    assert!(
        store
            .list_cache(50, 0)
            .await
            .expect("list after clear")
            .is_empty(),
        "clear_cache empties the table"
    );
}

/// `log_search`, `record_click` and the merged `list_history` feed with
/// `since`, `q` and `limit` filters.
pub async fn log_clicks_history(store: &impl Store) {
    let base = Utc::now();
    let alpha = "conformance history alpha";
    let beta = "conformance history beta";
    let click_url = "https://clicked.example.com/result";

    store
        .log_search(log_row(
            base - chrono::Duration::seconds(2),
            alpha,
            ClientKind::Api,
            LogSource::Network,
            640,
            10,
        ))
        .await
        .expect("log_search alpha");
    store
        .log_search(log_row(
            base - chrono::Duration::seconds(1),
            beta,
            ClientKind::Ui,
            LogSource::Cache,
            3,
            10,
        ))
        .await
        .expect("log_search beta");
    // A row as written before schema v2: `query_raw` stays NULL and must
    // decode back as `None`.
    let gamma = "conformance history gamma";
    let mut pre_v2 = log_row(base, gamma, ClientKind::Api, LogSource::Network, 1, 0);
    pre_v2.query_raw = None;
    store.log_search(pre_v2).await.expect("log_search gamma");
    store
        .record_click(ClickRow {
            id: None,
            ts: base,
            query_hash: Some(CacheKey::from(&request(alpha))),
            url: Url::parse(click_url).unwrap(),
            title: "Clicked result".to_string(),
            position: 0,
            client: ClientKind::Ui,
        })
        .await
        .expect("record_click");

    let history = store
        .list_history(&HistoryFilter {
            since: None,
            q: None,
            limit: 50,
        })
        .await
        .expect("list_history failed");

    let pos = |pred: &dyn Fn(&HistoryItem) -> bool| -> usize {
        history.iter().position(pred).expect("item in history")
    };
    let click_pos = pos(&|i| matches!(i, HistoryItem::Click(c) if c.url.as_str() == click_url));
    let beta_pos = pos(&|i| matches!(i, HistoryItem::Search(s) if s.query == beta));
    let alpha_pos = pos(&|i| matches!(i, HistoryItem::Search(s) if s.query == alpha));
    assert!(
        click_pos < beta_pos && beta_pos < alpha_pos,
        "history must merge searches and clicks newest-first"
    );

    // `query_raw` round-trips (schema v2); rows predating the column
    // decode it as None.
    let alpha_row = match &history[alpha_pos] {
        HistoryItem::Search(s) => s,
        _ => unreachable!(),
    };
    assert_eq!(alpha_row.query_raw.as_deref(), Some(alpha));
    let gamma_pos = pos(&|i| matches!(i, HistoryItem::Search(s) if s.query == gamma));
    let gamma_row = match &history[gamma_pos] {
        HistoryItem::Search(s) => s,
        _ => unreachable!(),
    };
    assert_eq!(gamma_row.query_raw, None);

    // `q` filters searches only; clicks pass through.
    let filtered = store
        .list_history(&HistoryFilter {
            since: None,
            q: Some(alpha.to_string()),
            limit: 50,
        })
        .await
        .expect("filtered history");
    assert!(
        filtered
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == alpha))
    );
    assert!(
        !filtered
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == beta))
    );
    assert!(
        filtered
            .iter()
            .any(|i| matches!(i, HistoryItem::Click(c) if c.url.as_str() == click_url)),
        "q filter must not remove clicks"
    );

    // `since` applies to both item kinds.
    let recent = store
        .list_history(&HistoryFilter {
            since: Some(base - chrono::Duration::milliseconds(1500)),
            q: None,
            limit: 50,
        })
        .await
        .expect("since-filtered history");
    assert!(
        !recent
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == alpha))
    );
    assert!(
        recent
            .iter()
            .any(|i| matches!(i, HistoryItem::Search(s) if s.query == beta))
    );

    // `limit` caps the merged feed.
    let one = store
        .list_history(&HistoryFilter {
            since: None,
            q: None,
            limit: 1,
        })
        .await
        .expect("limited history");
    assert_eq!(one.len(), 1);
}

/// `stats` aggregates: hit rate over `search_log` (never `cache_entries`),
/// latency percentiles, per-client and per-day counts, zero-result queries,
/// engine table and cache panel counts.
pub async fn stats_aggregates(store: &impl Store) {
    const DAYS: u32 = 30;
    let before = store.stats(DAYS).await.expect("baseline stats");

    let now = Utc::now();
    let mut rows = [
        log_row(
            now,
            "conf stats one",
            ClientKind::Api,
            LogSource::Cache,
            5,
            10,
        ),
        log_row(
            now,
            "conf stats two",
            ClientKind::Api,
            LogSource::Cache,
            7,
            10,
        ),
        log_row(
            now,
            "conf stats three",
            ClientKind::Api,
            LogSource::Network,
            100,
            3,
        ),
        log_row(
            now,
            "conf stats zero",
            ClientKind::Cli,
            LogSource::Network,
            200,
            0,
        ),
    ];
    // One row flags `deadline_hit`: the windowed counter reads the
    // `search_log.deadline_hit` column, not the metrics registry.
    rows[2].deadline_hit = true;
    for row in rows {
        store.log_search(row).await.expect("log_search");
    }
    // One live and one expired cache entry for the cache panel.
    store
        .put(
            &CacheKey::from(&request("conformance stats live")),
            &response(
                "conformance stats live",
                &[("L", "https://l.example.com/", "l")],
            ),
            Duration::from_secs(3600),
        )
        .await
        .expect("put live");
    store
        .put(
            &CacheKey::from(&request("conformance stats dead")),
            &response(
                "conformance stats dead",
                &[("D", "https://d.example.com/", "d")],
            ),
            Duration::ZERO,
        )
        .await
        .expect("put dead");

    let after = store.stats(DAYS).await.expect("stats");
    assert_eq!(after.window_days, DAYS);
    assert_eq!(after.searches, before.searches + 4);
    assert_eq!(after.cache_hits, before.cache_hits + 2);
    assert_eq!(
        after.deadline_hits,
        before.deadline_hits + 1,
        "deadline_hits counts search_log.deadline_hit rows in the window"
    );

    let expected_rate = after.cache_hits as f64 / after.searches as f64;
    assert!(
        (after.hit_rate - expected_rate).abs() < 1e-9,
        "hit_rate = cache_hits / searches over search_log"
    );

    let lat = after.latency.expect("window with searches has percentiles");
    assert!(lat.p50_ms <= lat.p90_ms && lat.p90_ms <= lat.p99_ms);

    let api_after = after
        .by_client
        .iter()
        .find(|c| c.client == "api")
        .map(|c| c.searches)
        .unwrap_or(0);
    let api_before = before
        .by_client
        .iter()
        .find(|c| c.client == "api")
        .map(|c| c.searches)
        .unwrap_or(0);
    assert_eq!(api_after, api_before + 3, "client split counts by label");

    assert!(
        after
            .zero_result_queries
            .iter()
            .any(|q| q == "conf stats zero"),
        "zero-result queries are surfaced"
    );

    let today = now.date_naive();
    let day_after = after
        .per_day
        .iter()
        .find(|d| d.day == today)
        .expect("today in per_day");
    let day_before = before
        .per_day
        .iter()
        .find(|d| d.day == today)
        .map(|d| d.searches)
        .unwrap_or(0);
    assert_eq!(day_after.searches, day_before + 4);
    assert!(day_after.cache_hits >= 2);

    // Engine table mirrors engine_health.
    let health = store.health().await.expect("health");
    assert_eq!(after.engines.len(), health.len());

    // Cache panel: live vs awaiting eviction.
    assert_eq!(after.cache_entries, before.cache_entries + 1);
    assert_eq!(
        after.cache_entries_expired,
        before.cache_entries_expired + 1
    );

    // Pinned semantic: `stats(0)` is the all-time window, so a 60-day-old row
    // counts there but not in `stats(30)`.
    let all0 = store.stats(0).await.expect("stats(0) baseline");
    let win0 = store.stats(30).await.expect("stats(30) baseline");
    store
        .log_search(log_row(
            Utc::now() - chrono::Duration::days(60),
            "conf stats old",
            ClientKind::Api,
            LogSource::Network,
            50,
            1,
        ))
        .await
        .expect("log old row");
    let all1 = store.stats(0).await.expect("stats(0)");
    let win1 = store.stats(30).await.expect("stats(30)");
    assert_eq!(all1.window_days, 0);
    assert_eq!(all1.searches, all0.searches + 1, "stats(0) is all-time");
    assert_eq!(
        win1.searches, win0.searches,
        "stats(30) excludes a 60-day-old row"
    );
}

/// `put_health` upserts by engine id; `health` returns all rows.
pub async fn engine_health(store: &impl Store) {
    let engine = EngineId::from("conf-engine-health");
    let row = EngineHealthRow {
        engine: engine.clone(),
        ewma_ms: 123.5,
        failures: 2,
        breaker: BreakerState::Open,
        breaker_until: Some(Utc::now() + chrono::Duration::minutes(5)),
        last_ok_at: Some(Utc::now() - chrono::Duration::minutes(1)),
        last_error: Some("timeout".to_string()),
    };
    store.put_health(&row).await.expect("put_health");

    let all = store.health().await.expect("health");
    let got = all
        .iter()
        .find(|r| r.engine == engine)
        .expect("row persisted");
    assert_eq!(got.ewma_ms, row.ewma_ms);
    assert_eq!(got.failures, 2);
    assert_eq!(got.breaker, BreakerState::Open);
    assert!(got.breaker_until.is_some());
    assert_eq!(got.last_error.as_deref(), Some("timeout"));

    // Upsert: same engine, fresh state, still exactly one row.
    let reset = EngineHealthRow {
        engine: engine.clone(),
        ewma_ms: 0.0,
        failures: 0,
        breaker: BreakerState::Closed,
        breaker_until: None,
        last_ok_at: Some(Utc::now()),
        last_error: None,
    };
    store.put_health(&reset).await.expect("put_health reset");
    let all = store.health().await.expect("health");
    assert_eq!(all.iter().filter(|r| r.engine == engine).count(), 1);
    assert_eq!(
        all.iter().find(|r| r.engine == engine).unwrap().breaker,
        BreakerState::Closed
    );
}

/// `audit` appends; `list_audit` returns newest-first with `since`, `actor`,
/// `action` and `limit` filters.
pub async fn audit_trail(store: &impl Store) {
    let base = Utc::now();
    let request_id = Uuid::now_v7();

    store
        .audit(AuditRow {
            id: None,
            ts: base - chrono::Duration::seconds(1),
            actor: "api".to_string(),
            action: "cache.delete".to_string(),
            target: "conformance-audit-target".to_string(),
            details: serde_json::json!({"key": "abc"}),
            request_id: Some(request_id),
        })
        .await
        .expect("audit api");
    store
        .audit(AuditRow {
            id: None,
            ts: base,
            actor: "cli".to_string(),
            action: "config.put".to_string(),
            target: "conformance-audit-config".to_string(),
            details: serde_json::Value::Null,
            request_id: None,
        })
        .await
        .expect("audit cli");

    let all = store
        .list_audit(&AuditFilter {
            since: None,
            actor: None,
            action: None,
            limit: 50,
        })
        .await
        .expect("list_audit");
    let p_api = all
        .iter()
        .position(|r| r.target == "conformance-audit-target")
        .expect("api row listed");
    let p_cli = all
        .iter()
        .position(|r| r.target == "conformance-audit-config")
        .expect("cli row listed");
    assert!(p_cli < p_api, "list_audit is newest-first");
    assert_eq!(all[p_api].request_id, Some(request_id));
    assert_eq!(all[p_api].details, serde_json::json!({"key": "abc"}));

    let by_actor = store
        .list_audit(&AuditFilter {
            since: None,
            actor: Some("cli".to_string()),
            action: None,
            limit: 50,
        })
        .await
        .expect("actor filter");
    assert!(by_actor.iter().all(|r| r.actor == "cli"));
    assert!(
        by_actor
            .iter()
            .any(|r| r.target == "conformance-audit-config")
    );

    let by_action = store
        .list_audit(&AuditFilter {
            since: None,
            actor: None,
            action: Some("cache.delete".to_string()),
            limit: 50,
        })
        .await
        .expect("action filter");
    assert!(by_action.iter().all(|r| r.action == "cache.delete"));
    assert!(
        by_action
            .iter()
            .any(|r| r.target == "conformance-audit-target")
    );

    let recent = store
        .list_audit(&AuditFilter {
            since: Some(base - chrono::Duration::milliseconds(500)),
            actor: None,
            action: None,
            limit: 50,
        })
        .await
        .expect("since filter");
    assert!(
        recent
            .iter()
            .all(|r| r.ts >= base - chrono::Duration::milliseconds(500))
    );

    let one = store
        .list_audit(&AuditFilter {
            since: None,
            actor: None,
            action: None,
            limit: 1,
        })
        .await
        .expect("limited audit");
    assert_eq!(one.len(), 1);

    let facets = store.audit_facets().await.expect("audit_facets");
    assert!(facets.actors.contains(&"api".to_string()));
    assert!(facets.actors.contains(&"cli".to_string()));
    assert!(facets.actions.contains(&"cache.delete".to_string()));
    assert!(facets.actions.contains(&"config.put".to_string()));
}

/// Run the whole suite against one store. Safe on a non-empty store: every
/// function uses unique fixture values and asserts membership or deltas.
/// `cache_admin` runs last because it clears `cache_entries`.
pub async fn run_all(store: &impl Store) {
    cache_exact_roundtrip(store).await;
    cache_expiry_and_eviction(store).await;
    lexical_search(store).await;
    log_clicks_history(store).await;
    stats_aggregates(store).await;
    engine_health(store).await;
    audit_trail(store).await;
    cache_admin(store).await;
}
