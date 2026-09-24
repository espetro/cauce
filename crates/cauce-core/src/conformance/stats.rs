//! Dashboard aggregate (`stats`) conformance checks.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::time::Duration;

use chrono::{DateTime, Utc};

use super::{log_row, request, response};
use crate::cache::CacheKey;
use crate::request::ClientKind;
use crate::store::{LogSource, StatsSnapshot, Store};

/// `stats` aggregates: hit rate over `search_log` (never `cache_entries`),
/// latency percentiles, per-client and per-day counts, zero-result queries,
/// engine table and cache panel counts.
pub async fn stats_aggregates(store: &impl Store) {
    const DAYS: u32 = 30;
    let before = store.stats(DAYS).await.expect("baseline stats");

    let now = Utc::now();
    seed_stats_rows(store, now).await;
    verify_stats_deltas(store, &before, now, DAYS).await;
    verify_all_time_window(store).await;
}

/// Four search rows (one deadline-hit, one zero-result) plus one live and one
/// expired cache entry for the cache panel.
async fn seed_stats_rows(store: &impl Store, now: DateTime<Utc>) {
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
}

/// The seeded rows move the windowed aggregates by exactly their deltas:
/// hit rate, latency percentiles, per-client and per-day counts, zero-result
/// queries, engine table and cache panel counts.
async fn verify_stats_deltas(
    store: &impl Store,
    before: &StatsSnapshot,
    now: DateTime<Utc>,
    days: u32,
) {
    let after = store.stats(days).await.expect("stats");
    assert_eq!(after.window_days, days);
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
}

/// `stats(0)` is the all-time window: a 60-day-old row counts there but not
/// in `stats(30)`.
async fn verify_all_time_window(store: &impl Store) {
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
