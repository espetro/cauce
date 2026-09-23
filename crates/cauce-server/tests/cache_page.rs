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
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use cauce_core::config::Config;
use cauce_core::{AuditFilter, CacheKey, SearchPipeline, StoreTuning};
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

/// Write a `cache_entries` row straight through the test store, bypassing
/// the pipeline. `ttl` of `Duration::ZERO` lands the row already expired
/// (`expires_at <= now`), which is how the page's expired state is
/// reached in tests — the spec's `ttl_s=1` recipe is not a supported
/// `/api/search` param.
async fn seed_store_entry(state: &AppState, q: &str, ttl: Duration) -> String {
    let req = cauce_core::conformance::request(q);
    let key = CacheKey::from(&req);
    let resp = cauce_core::conformance::response(q, &[("T", "https://t.example.com/", "s")]);
    state
        .store()
        .put(&key, &resp, ttl)
        .await
        .expect("seed cache entry");
    key.as_str().to_string()
}

#[tokio::test]
async fn cache_page_lists_entries_with_admin_controls() {
    let (app, _state, _tmp) = app();
    let key = seed_entry(&app, "cachelisttest").await;

    let request_id = "01234567-89ab-cdef-0123-456789abcdef";
    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::GET)
            .uri("/cache")
            .header("Accept", "text/html")
            .header("x-request-id", request_id)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("<!doctype html>"), "full page: {body}");
    assert!(body.contains("cachelisttest"), "row query shown: {body}");
    assert!(body.contains("replay"), "row engines shown: {body}");
    assert!(body.contains("1 entry"), "count line singular: {body}");
    assert!(body.contains("expires in"), "live expiry phrase: {body}");
    assert!(
        !body.contains("Z</span>"),
        "created is local YYYY-MM-DD HH:MM, no UTC suffix: {body}"
    );
    // Deletes say so in a noscript hint next to the controls.
    assert!(
        body.contains("<noscript>") && body.contains("need JavaScript"),
        "noscript hint for deletes: {body}"
    );
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
    // The count line is addressable and the row delete decrements it in
    // place (spec: "row removed in place, count line decrements"), with
    // the singular/plural swap embedded from strings::cache.
    assert!(
        body.contains(r#"<span id="cache-count">1 entry</span>"#),
        "count line carries the decrement target id: {body}"
    );
    assert!(
        body.contains("hx-on::after-request") && body.contains("getElementById('cache-count')"),
        "row delete decrements the count line after the request: {body}"
    );
    assert!(
        body.contains("'entry'") && body.contains("'entries'"),
        "decrement keeps the entry/entries singular-plural copy: {body}"
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
    // The full rendered request id is visible and selectable in the footer.
    assert!(
        body.contains(&format!(r#"<code class="request-id">{request_id}</code>"#)),
        "full request id footer: {body}"
    );
}

#[tokio::test]
async fn cache_page_empty_state() {
    let (app, _state, _tmp) = app();
    let (status, body) = get_html(&app, "/cache").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("no cached queries yet"),
        "empty state: {body}"
    );
    assert!(body.contains("0 entries"), "count line: {body}");
    // Nothing to delete, so the bulk controls are hidden.
    assert!(
        !body.contains(r#"hx-delete="/api/cache?all=true""#),
        "delete controls hidden when the list is empty: {body}"
    );
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
    let alpha_key = seed_entry(&app, "alpha%20bravo").await;
    seed_entry(&app, "charlie%20delta").await;

    let (status, body) = get_html(&app, "/cache?q=alpha").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("alpha bravo"), "matching row shown: {body}");
    assert!(
        !body.contains("charlie delta"),
        "non-matching row hidden: {body}"
    );
    assert!(
        body.contains("1 matching entry"),
        "filtered count line: {body}"
    );
    assert!(
        body.contains("showing the newest"),
        "filtered one-page cap note: {body}"
    );

    // The HTML page and JSON endpoint use the same filter and cache rows.
    let (status, entries) = get_json(&app, "/api/cache?q=alpha").await;
    assert_eq!(status, StatusCode::OK);
    let rows = entries.as_array().unwrap();
    assert_eq!(rows.len(), 1, "{entries}");
    assert_eq!(rows[0]["key"], alpha_key);
    assert!(
        body.contains(&alpha_key),
        "HTML renders the same JSON row: {body}"
    );

    // A filter that matches nothing names the filter in the empty state.
    let (status, body) = get_html(&app, "/cache?q=zzznomatch").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"nothing cached matches "zzznomatch"."#)
            || body.contains("nothing cached matches &#34;zzznomatch&#34;."),
        "filtered empty state names the filter: {body}"
    );
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

/// `GET /api/cache/{key}` under `Accept: text/html` answers error
/// statuses as a fragment, so the row expander can render the failure
/// inline instead of a JSON envelope the page cannot swap. The page's
/// own htmx fetch (`HX-Request: true`) gets the fragment with `200` so
/// htmx swaps it in place without a console-logged network error;
/// non-htmx callers get the real status.
#[tokio::test]
async fn cache_row_expand_error_is_an_html_fragment() {
    let (app, _state, _tmp) = app();

    // Absent key, non-htmx HTML caller: 404 fragment naming the status.
    let missing = "f".repeat(64);
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/cache/{missing}"))
                .header("Accept", "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        ct.starts_with("text/html"),
        "error arm is text/html, got {ct}: {body}"
    );
    assert!(
        body.contains("error: could not load payload (404)"),
        "inline error line: {body}"
    );

    // The same request as the page's expander sends it (`HX-Request`
    // plus `Accept: text/html`, per the details' hx-headers): a 200
    // fragment that htmx swaps into .cache-payload directly.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/cache/{missing}"))
                .header("Accept", "text/html")
                .header("HX-Request", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "htmx error fetches stay swap-friendly"
    );
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        body.contains("error: could not load payload (404)"),
        "inline error line keeps the failing status: {body}"
    );

    // Malformed key: same fragment with the 400 status.
    let (status, body) = get_html(&app, "/api/cache/not-hex-at-all").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.contains("error: could not load payload (400)"),
        "inline error line: {body}"
    );

    // The JSON arm still answers the envelope.
    let (status, body) = get_json(&app, &format!("/api/cache/{missing}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found", "{body}");
}

/// The page renders the error line inside the payload block when the
/// lazy fetch fails (hx-on::response-error wiring on the details).
#[tokio::test]
async fn cache_page_expand_error_wiring() {
    let (app, _state, _tmp) = app();
    seed_entry(&app, "errorwiring").await;
    let (status, body) = get_html(&app, "/cache").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("hx-on::response-error"),
        "details handles htmx response errors: {body}"
    );
    assert!(
        body.contains("error: could not load payload ({status})"),
        "error copy is embedded for the handler: {body}"
    );
}

/// Expired rows are seeded through the test store (`Store::put` with a
/// zero TTL lands `expires_at <= now`), then render muted with the
/// `expired <rel> ago` phrase.
#[tokio::test]
async fn cache_page_expired_row_renders_muted() {
    let (app, state, _tmp) = app();
    seed_store_entry(&state, "stalerow", Duration::ZERO).await;

    let (status, body) = get_html(&app, "/cache").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("stalerow"), "expired row is listed: {body}");
    assert!(
        body.contains("cache-entry expired"),
        "expired row carries the muted class: {body}"
    );
    assert!(
        body.contains("expired") && body.contains("ago"),
        "expired <rel> ago phrase: {body}"
    );
    assert!(
        !body.contains("expires in"),
        "expired row must not read 'expires in': {body}"
    );

    // `evict_expired` semantics agree the seeded row is expired.
    let (status, body) = page_delete(&app, "/api/cache?expired=true").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed"], 1, "seeded row is expired: {body}");
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
