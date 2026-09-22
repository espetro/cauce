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

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use cauce_core::config::Config;
use cauce_core::{AuditFilter, SearchPipeline, StoreTuning};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::Value;
use tower::ServiceExt;

fn app() -> (Router, AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    let state = AppState::new(pipeline, store, Config::default());
    (build_router(state.clone()), state, tmp)
}

async fn call(router: &Router, request: Request<Body>) -> (StatusCode, String) {
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn call_json(router: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).expect("valid JSON"))
}

async fn get_html(router: &Router, uri: &str) -> (StatusCode, String) {
    call(
        router,
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("Accept", "text/html")
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

async fn get_json(router: &Router, uri: &str) -> (StatusCode, Value) {
    call_json(
        router,
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("Accept", "application/json")
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

/// The delete request exactly as the page's `hx-delete` button sends it.
async fn page_delete(router: &Router, uri: &str) -> (StatusCode, Value) {
    call_json(
        router,
        Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .header("X-Cauce-Client", "ui")
            .body(Body::empty())
            .unwrap(),
    )
    .await
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
async fn cache_page_lists_entries_with_admin_controls() {
    let (app, _state, _tmp) = app();
    let key = seed_entry(&app, "cachelisttest").await;

    let (status, body) = get_html(&app, "/cache").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("<!doctype html>"), "full page: {body}");
    assert!(body.contains("cachelisttest"), "row query shown: {body}");
    assert!(body.contains("replay"), "row engines shown: {body}");
    assert!(body.contains("hits"), "hits column shown: {body}");
    // Row delete wiring: the audited DELETE endpoint with client `ui`.
    assert!(
        body.contains(&format!(r#"hx-delete="/api/cache/{key}""#)),
        "row delete button targets the key: {body}"
    );
    assert!(
        body.contains(r#"hx-headers='{"X-Cauce-Client":"ui"}'"#),
        "deletes must carry X-Cauce-Client: ui so audit actor is ui: {body}"
    );
    assert!(
        body.contains("hx-confirm"),
        "destructs actions confirm: {body}"
    );
    // Bulk actions.
    assert!(
        body.contains(r#"hx-delete="/api/cache?expired=true""#),
        "delete-expired button: {body}"
    );
    assert!(
        body.contains(r#"hx-delete="/api/cache?all=true""#),
        "delete-all button: {body}"
    );
    // Row expander lazy-loads the payload fragment.
    assert!(
        body.contains(&format!(r#"hx-get="/api/cache/{key}""#)),
        "row expander targets the payload endpoint: {body}"
    );
    // The page shows the request id of the data it rendered.
    assert!(body.contains("request-id"), "request id footer: {body}");
}

#[tokio::test]
async fn cache_page_empty_state() {
    let (app, _state, _tmp) = app();
    let (status, body) = get_html(&app, "/cache").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("cache is empty"), "empty state: {body}");
}

#[tokio::test]
async fn cache_page_json_accept_delegates_to_api() {
    let (app, _state, _tmp) = app();
    seed_entry(&app, "jsonaccept").await;
    let (status, body) = get_json(&app, "/cache").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().expect("JSON array from /cache");
    assert_eq!(rows.len(), 1, "{body}");
    assert_eq!(rows[0]["query"], "jsonaccept");
}

#[tokio::test]
async fn cache_page_filter_uses_lexical_index() {
    let (app, _state, _tmp) = app();
    seed_entry(&app, "alpha%20bravo").await;
    seed_entry(&app, "charlie%20delta").await;

    let (status, body) = get_html(&app, "/cache?q=alpha").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("alpha bravo"), "matching row shown: {body}");
    assert!(
        !body.contains("charlie delta"),
        "non-matching row hidden: {body}"
    );

    // Same filter on the shared JSON handler.
    let (status, entries) = get_json(&app, "/api/cache?q=charlie").await;
    assert_eq!(status, StatusCode::OK);
    let rows = entries.as_array().unwrap();
    assert_eq!(rows.len(), 1, "{entries}");
    assert_eq!(rows[0]["query"], "charlie delta");

    // A filter that matches nothing renders the filtered empty state.
    let (status, body) = get_html(&app, "/cache?q=zzznomatch").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("No cache entries match"), "{body}");
}

#[tokio::test]
async fn cache_page_paginates() {
    let (app, _state, _tmp) = app();
    for q in ["pager one", "pager two", "pager three"] {
        seed_entry(&app, &q.replace(' ', "%20")).await;
    }

    let (status, body) = get_html(&app, "/cache?limit=2").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("pager three"), "newest first: {body}");
    assert!(body.contains("pager two"), "{body}");
    assert!(!body.contains("pager one"), "page 1 shows 2 rows: {body}");
    assert!(
        body.contains("/cache?offset=2"),
        "older link paginates: {body}"
    );

    let (status, body) = get_html(&app, "/cache?limit=2&offset=2").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("pager one"), "{body}");
    assert!(!body.contains("pager three"), "{body}");
    assert!(
        body.contains("/cache?offset=0"),
        "newer link paginates back: {body}"
    );
}

#[tokio::test]
async fn cache_row_expand_renders_pretty_payload() {
    let (app, _state, _tmp) = app();
    let key = seed_entry(&app, "expandme").await;

    let (status, body) = get_html(&app, &format!("/api/cache/{key}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains(r#"<pre class="cache-payload-json">"#),
        "payload fragment: {body}"
    );
    // Pretty-printed JSON contains the stored response fields (askama
    // escapes `"` as `&#34;`).
    assert!(body.contains("&#34;query&#34;"), "escaped JSON: {body}");
    assert!(body.contains("expandme"), "payload shows the query: {body}");

    // The JSON arm is unchanged.
    let (status, entry) = get_json(&app, &format!("/api/cache/{key}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(entry["key"], key);
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
