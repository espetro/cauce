//! W3-02 acceptance on the HTTP surface: the `stale · refreshing` badge
//! on an in-grace stale serve, and the empty-response cache-hygiene rule
//! (a 200 whose fan-out answered `[]` writes no cache row).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use cauce_core::CacheKey;
use cauce_engines::ReplayOpts;
use support::{app, app_with, get_html, get_json};

/// Seed a `cache_entries` row straight through the test store (same
/// recipe as `cache_page`'s `seed_store_entry`): `Duration::ZERO` lands
/// the row already expired — inside the default 6 h stale-serve grace.
async fn seed_expired(state: &cauce_server::AppState, q: &str) {
    let req = cauce_core::conformance::request(q);
    let key = CacheKey::from(&req);
    let resp = cauce_core::conformance::response(q, &[("T", "https://t.example.com/", "s")]);
    state
        .store()
        .put(&key, &resp, Duration::ZERO)
        .await
        .expect("seed cache entry");
}

/// An in-grace expired row is served stale and the SSR badge reads
/// `stale · refreshing`.
#[tokio::test]
async fn stale_serve_renders_the_refreshing_badge() {
    let (app, state, _tmp) = app();
    seed_expired(&state, "stalebadge").await;

    let (status, body) = get_html(&app, "/search?q=stalebadge").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("stale · refreshing"),
        "stale serve must render the refreshing badge: {body}"
    );
}

/// W3-02 rule a: a fan-out whose engine answers `[]` is a 200 with no
/// cache write — a wedged engine must not poison the cache.
#[tokio::test]
async fn empty_engine_response_is_not_cached() {
    let (app, _state, _tmp) = app_with(ReplayOpts {
        empty: true,
        ..ReplayOpts::default()
    });

    let (status, body) = get_json(&app, "/api/search?q=noresults").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["meta"]["source"], "network", "{body}");

    let (status, entries) = get_json(&app, "/api/cache").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        entries.as_array().unwrap().len(),
        0,
        "empty response must not be cached: {entries}"
    );
}
