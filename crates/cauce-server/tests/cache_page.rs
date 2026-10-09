//! `/cache` page integration tests (W2-04).
//!
//! Acceptance: delete via the page removes the row and writes an `audit`
//! row; next search is `Network`.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The HTMX pages exist only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use cauce_core::AuditFilter;
use serde_json::Value;

mod support;
use support::*;

/// The delete request exactly as the page's `hx-delete` button sends it.
async fn page_delete(router: &Router, uri: &str) -> (StatusCode, Value) {
    let (status, _headers, body) = call_json(
        router,
        Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .header("X-Cauce-Client", "ui")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    (status, body)
}

/// First cache entry's key after seeding `GET /api/search?q=<q>`.
async fn seed_entry(router: &Router, q: &str) -> String {
    let (status, _) = get_json(router, &format!("/api/search?q={q}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, entries) = get_json(router, "/api/cache").await;
    assert_eq!(status, StatusCode::OK);
    entries[0]["key"].as_str().expect("cache key").to_string()
}

#[tokio::test]
async fn cache_page_filter_uses_lexical_index() {
    let (app, _state, _tmp) = app();
    let alpha_key = seed_entry(&app, "alpha%20bravo").await;
    seed_entry(&app, "charlie%20delta").await;

    // FX-05: the listing is JSON-only (`GET /api/cache` answers the bare
    // entries array; the SPA renders the same rows the HTMX page did).
    let (status, entries) = get_json(&app, "/api/cache?q=alpha").await;
    assert_eq!(status, StatusCode::OK);
    let rows = entries.as_array().unwrap();
    assert_eq!(rows.len(), 1, "{entries}");
    assert_eq!(rows[0]["key"], alpha_key);
    assert_eq!(rows[0]["query"].as_str(), Some("alpha bravo"));

    // A filter that matches nothing returns an empty entries list.
    let (status, entries) = get_json(&app, "/api/cache?q=zzznomatch").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(entries.as_array().unwrap().len(), 0, "{entries}");
}

#[tokio::test]
async fn cache_page_paginates() {
    let (app, _state, _tmp) = app();
    for q in ["pager one", "pager two", "pager three"] {
        seed_entry(&app, &q.replace(' ', "%20")).await;
    }

    let (status, entries) = get_json(&app, "/api/cache?limit=2").await;
    assert_eq!(status, StatusCode::OK);
    let rows = entries.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{entries}");
    assert_eq!(
        rows[0]["query"].as_str(),
        Some("pager three"),
        "newest first"
    );
    assert_eq!(rows[1]["query"].as_str(), Some("pager two"));

    let (status, entries) = get_json(&app, "/api/cache?limit=2&offset=2").await;
    assert_eq!(status, StatusCode::OK);
    let rows = entries.as_array().unwrap();
    assert_eq!(rows.len(), 1, "{entries}");
    assert_eq!(rows[0]["query"].as_str(), Some("pager one"));
}

/// W2-04 acceptance: delete via the page removes the row and writes an
/// `audit` row; next search is `Network`.
#[tokio::test]
async fn page_delete_audits_and_next_search_is_network() {
    let (app, state, _tmp) = app();
    let key = seed_entry(&app, "deleteme").await;

    // Second search serves from cache, proving the row is live.
    let (status, body) = get_json(&app, "/api/search?q=deleteme").await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(
        body["meta"]["source"], "network",
        "second search should be a cache hit: {body}"
    );

    let (status, body) = page_delete(&app, &format!("/api/cache/{key}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);

    // The row is gone from the admin surface.
    let (status, entries) = get_json(&app, "/api/cache").await;
    assert_eq!(status, StatusCode::OK);
    assert!(entries.as_array().unwrap().is_empty(), "{entries}");

    // Audit row written with actor `ui` (the X-Cauce-Client header the
    // page's hx-headers send).
    let audit = state
        .store()
        .list_audit(&AuditFilter {
            action: Some("cache.delete".to_string()),
            ..AuditFilter::default()
        })
        .await
        .expect("audit rows");
    assert_eq!(audit.len(), 1, "{audit:?}");
    assert_eq!(audit[0].actor, "ui");
    assert_eq!(audit[0].target, key);

    // Next search misses the cache.
    let (status, body) = get_json(&app, "/api/search?q=deleteme").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["meta"]["source"], "network",
        "post-delete search must be Network: {body}"
    );
}

#[tokio::test]
async fn page_bulk_deletes_are_audited() {
    let (app, state, _tmp) = app();
    seed_entry(&app, "bulkone").await;
    seed_entry(&app, "bulktwo").await;

    let (status, body) = page_delete(&app, "/api/cache?expired=true").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed"], 0, "nothing is expired yet");

    let (status, body) = page_delete(&app, "/api/cache?all=true").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed"], 2, "{body}");

    let actions: Vec<String> = state
        .store()
        .list_audit(&AuditFilter::default())
        .await
        .expect("audit")
        .iter()
        .filter(|r| r.actor == "ui")
        .map(|r| r.action.clone())
        .collect();
    assert!(
        actions.contains(&"cache.evict_expired".to_string()),
        "expired delete audited with actor ui: {actions:?}"
    );
    assert!(
        actions.contains(&"cache.clear".to_string()),
        "delete-all audited with actor ui: {actions:?}"
    );
}
