//! `/history` page + `DELETE /api/history/{id}` integration tests (W2-02).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The HTMX pages exist only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use cauce_core::config::Config;
use cauce_core::{
    AuditFilter, CacheKey, ClientKind, EngineId, LogSource, SafeSearch, SearchLogRow,
    SearchPipeline, SearchRequest, StoreTuning, Tier,
};
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use serde_json::{Value, json};
use tower::ServiceExt;

fn test_state() -> (AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    let state = AppState::new(pipeline, store, Config::default());
    (state, tmp)
}

fn app() -> (Router, AppState, tempfile::TempDir) {
    let (state, tmp) = test_state();
    (build_router(state.clone()), state, tmp)
}

async fn call(router: &Router, request: Request<Body>) -> (StatusCode, String) {
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
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
    let (status, text) = call(
        router,
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("Accept", "application/json")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

async fn search(router: &Router, q: &str) {
    let (status, _) = get_json(router, &format!("/api/search?q={q}")).await;
    assert_eq!(status, StatusCode::OK, "search {q:?} failed");
}

/// Count rendered search rows (one `class="search"` per `search_log` row).
fn search_rows(body: &str) -> usize {
    body.matches(r#"class="search""#).count()
}

/// The `CacheKey` an unpinned `/api/search?q=` request produces (the join
/// key between `search_log` and `clicks`).
fn query_hash(q: &str) -> String {
    CacheKey::from(&SearchRequest {
        q: q.to_string(),
        page: 1,
        lang: None,
        time_range: None,
        safesearch: SafeSearch::default(),
        engines: None,
        client: ClientKind::Api,
    })
    .as_str()
    .to_string()
}

fn log_row(ts: chrono::DateTime<chrono::Utc>, q: &str, source: LogSource) -> SearchLogRow {
    SearchLogRow {
        id: None,
        ts,
        query_hash: CacheKey::from(&SearchRequest {
            q: q.to_string(),
            page: 1,
            lang: None,
            time_range: None,
            safesearch: SafeSearch::default(),
            engines: None,
            client: ClientKind::Api,
        }),
        query: q.to_string(),
        query_raw: Some(q.to_string()),
        client: ClientKind::Api,
        source,
        tier: match source {
            LogSource::Cache => Some(Tier::T1),
            LogSource::Network => None,
        },
        latency_ms: 12,
        result_count: 10,
        engines: vec![EngineId::from("replay")],
        deadline_hit: false,
    }
}

/// Acceptance: after 3 replay searches the page shows 3 rows with the
/// right sources. The `source` column is computed at render time from
/// `cache_entries` (amended W2-02): a fresh search wrote an unexpired
/// entry, so every row reads `cached · <age>` linking to `/cache`.
#[tokio::test]
async fn history_page_shows_replay_searches_with_sources() {
    let (app, _state, _tmp) = app();

    search(&app, "w2-history-alpha").await;
    search(&app, "w2-history-beta").await;
    search(&app, "w2-history-gamma").await;

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(search_rows(&body), 3, "expected 3 search rows: {body}");
    for q in ["w2-history-alpha", "w2-history-beta", "w2-history-gamma"] {
        assert!(body.contains(q), "row for {q} missing: {body}");
    }
    assert!(
        body.contains("cached ·"),
        "live cache entries render `cached · <age>`: {body}"
    );
    assert!(
        body.contains(r#"href="/cache?q=w2-history-alpha#"#),
        "cached source links to the entry on /cache: {body}"
    );
    assert!(
        body.contains("replay"),
        "engines column names replay: {body}"
    );
    // The newest row is first (gamma was searched last).
    let gamma = body.find("w2-history-gamma").expect("gamma row");
    let alpha = body.find("w2-history-alpha").expect("alpha row");
    assert!(gamma < alpha, "rows must be newest-first");

    // A repeat identical search refreshes the entry; both rows stay
    // `cached ·` (the column is the query's live cache state, not the
    // fetch that produced the row).
    search(&app, "w2-history-alpha").await;
    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(search_rows(&body), 4);
    assert!(body.contains("cached ·"), "{body}");
}

/// The source column follows `cache_entries`, not the logged fetch: once
/// the entry is gone the same row reads `network · t<tier>`; a fresh
/// identical search flips it back to `cached ·`.
#[tokio::test]
async fn history_source_tracks_live_cache_state() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-src-flip").await;

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("cached ·"), "entry live: {body}");

    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::DELETE)
            .uri(format!("/api/cache/{}", query_hash("w2-src-flip")))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("network · t1"),
        "entry gone → network source: {body}"
    );
    assert!(!body.contains("cached ·"), "{body}");

    search(&app, "w2-src-flip").await;
    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("cached ·"), "entry refilled: {body}");
}

/// `cached=1` (amended W2-02) keeps only rows whose query has a live cache
/// entry — same param on the JSON route. After the entry is deleted the
/// page renders the filtered-empty state.
#[tokio::test]
async fn history_cached_filter() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-cached-yes").await;
    search(&app, "w2-cached-no").await;
    // Drop the entry for the second query: its row leaves the cached set.
    let (status, _) = call(
        &app,
        Request::builder()
            .method(Method::DELETE)
            .uri(format!("/api/cache/{}", query_hash("w2-cached-no")))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = get_html(&app, "/history?cached=1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("w2-cached-yes"), "{body}");
    assert!(
        !body.contains("w2-cached-no"),
        "uncached row filtered out: {body}"
    );
    // The checkbox re-renders checked and `clear` is offered.
    assert!(
        body.contains(r#"name="cached" value="1" checked"#),
        "{body}"
    );
    assert!(body.contains(">clear<"), "{body}");

    // Same filter on the JSON route.
    let (status, feed) = get_json(&app, "/api/history?cached=1").await;
    assert_eq!(status, StatusCode::OK);
    let rows = feed.as_array().unwrap();
    assert!(
        rows.iter()
            .all(|r| r["query"] != "w2-cached-no" || r["kind"] == "click"),
        "{feed}"
    );
    assert!(rows.iter().any(|r| r["query"] == "w2-cached-yes"));

    // Delete the surviving entry too: the filter now matches nothing and
    // the page names the active filter in the empty state.
    let (status, _) = call(
        &app,
        Request::builder()
            .method(Method::DELETE)
            .uri(format!("/api/cache/{}", query_hash("w2-cached-yes")))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = get_html(&app, "/history?cached=1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(search_rows(&body), 0, "{body}");
    assert!(
        body.contains("that are still cached"),
        "filtered-empty copy names the cached filter: {body}"
    );
}

/// Acceptance: `since=24h` hides a row backdated in the temp DB; `since=all`
/// brings it back.
#[tokio::test]
async fn history_since_filter_hides_backdated_row() {
    let (app, state, _tmp) = app();

    state
        .store()
        .log_search(log_row(
            chrono::Utc::now() - chrono::Duration::days(3),
            "w2-history-ancient",
            LogSource::Network,
        ))
        .await
        .expect("backdated log row");
    search(&app, "w2-history-fresh").await;

    let (status, body) = get_html(&app, "/history?since=24h").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("w2-history-fresh"), "{body}");
    assert!(
        !body.contains("w2-history-ancient"),
        "backdated row must be hidden by since=24h: {body}"
    );

    let (status, body) = get_html(&app, "/history?since=all").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("w2-history-ancient"), "{body}");
    assert!(body.contains("w2-history-fresh"), "{body}");

    // The other advertised windows accept their tokens too.
    for window in ["7d", "30d"] {
        let (status, _) = get_html(&app, &format!("/history?since={window}")).await;
        assert_eq!(status, StatusCode::OK, "since={window}");
    }
    // And the API takes the same tokens (one filter grammar).
    let (status, body) = get_json(&app, "/api/history?since=24h").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert!(
        rows.iter().all(|r| r["query"] != "w2-history-ancient"),
        "api since=24h must hide the backdated row: {body}"
    );
}

/// `q` is a URL-param substring filter over the stored query.
#[tokio::test]
async fn history_q_filter_narrows_rows() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-filter-alpha").await;
    search(&app, "w2-filter-beta").await;

    let (status, body) = get_html(&app, "/history?q=alpha").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("w2-filter-alpha"), "{body}");
    assert!(!body.contains("w2-filter-beta"), "{body}");
    // The filter input re-renders the active value.
    assert!(body.contains(r#"value="alpha""#), "{body}");
}

/// History preserves the submitted casing and spacing from `query_raw`.
#[tokio::test]
async fn history_page_displays_original_query() {
    let (app, _state, _tmp) = app();
    let (status, _) = get_json(&app, "/api/search?q=Rust%20%20History").await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Rust  History"),
        "original query text: {body}"
    );
    assert!(
        body.contains(r#"href="/search?q=Rust%20%20History""#),
        "re-run preserves the original query: {body}"
    );
}

/// A click joins its search row (same `query_hash`): the clicks cell shows
/// the count and the linked title.
#[tokio::test]
async fn history_row_joins_clicks() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-click-join").await;

    let beacon = json!({
        "url": "https://example.com/w2-click",
        "title": "W2 clicked title",
        "position": 0,
        "query_hash": query_hash("w2-click-join"),
    });
    let (status, _) = call(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/click")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&beacon).unwrap()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        search_rows(&body),
        1,
        "click joins the search row, not a new row: {body}"
    );
    assert!(
        body.contains(">1 clicks<"),
        "summary reads `N clicks`: {body}"
    );
    // Nested line: domain, title link (new tab), 1-based #position.
    assert!(body.contains("example.com"), "{body}");
    assert!(body.contains("W2 clicked title"), "{body}");
    assert!(body.contains("https://example.com/w2-click"), "{body}");
    assert!(body.contains(r#"target="_blank""#), "{body}");
    assert!(body.contains("#1"), "{body}");
    // A click nests as an open <details>, not a row of its own.
    assert!(!body.contains("click only"), "{body}");
}

/// A click whose `query_hash` matches no search row renders as its own
/// `(click only)` row with dashes in the search columns.
#[tokio::test]
async fn history_unmatched_click_renders_click_only_row() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-real-search").await;

    let beacon = json!({
        "url": "https://tokio.rs/tutorial",
        "title": "Tokio tutorial",
        "position": 1,
        "query_hash": query_hash("w2-never-searched"),
    });
    let (status, _) = call(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/click")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&beacon).unwrap()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(search_rows(&body), 1, "one search row: {body}");
    assert!(body.contains("(click only)"), "{body}");
    assert!(body.contains("tokio.rs"), "{body}");
    assert!(body.contains("Tokio tutorial"), "{body}");
    assert!(body.contains("#2"), "position is 1-based: {body}");
    // The click row does not turn into a search row.
    assert_eq!(body.matches("(click only)").count(), 1, "{body}");
}

/// Delete is audited (`history.delete`, actor `ui` via `X-Cauce-Client`)
/// and cascades to the row's clicks.
#[tokio::test]
async fn history_delete_removes_row_clicks_and_audits() {
    let (app, state, _tmp) = app();
    search(&app, "w2-delete-me").await;

    let beacon = json!({
        "url": "https://example.com/w2-delete",
        "position": 0,
        "query_hash": query_hash("w2-delete-me"),
    });
    let (status, _) = call(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/click")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&beacon).unwrap()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = get_json(&app, "/api/history").await;
    assert_eq!(status, StatusCode::OK);
    let id = body
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["query"] == "w2-delete-me")
        .and_then(|r| r["id"].as_i64())
        .expect("search row id");

    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::DELETE)
            .uri(format!("/api/history/{id}"))
            .header("X-Cauce-Client", "ui")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["deleted"], true);
    assert_eq!(body["clicks_removed"], 1);

    // Row and its clicks are gone from the page and the feed.
    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("w2-delete-me"), "{body}");
    let (_, feed) = get_json(&app, "/api/history").await;
    assert!(
        !feed
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.to_string().contains("w2-delete")),
        "{feed}"
    );

    // The delete must not touch the cache entry for the query.
    let key: CacheKey = query_hash("w2-delete-me").parse().unwrap();
    assert!(
        state
            .store()
            .get_cache(&key)
            .await
            .expect("get_cache")
            .is_some(),
        "history delete leaves cache_entries untouched"
    );

    // The audit row carries actor ui and the deleted row's context.
    let audits = state
        .store()
        .list_audit(&AuditFilter {
            action: Some("history.delete".to_string()),
            ..AuditFilter::default()
        })
        .await
        .expect("audit rows");
    let row = audits
        .iter()
        .find(|r| r.target == id.to_string())
        .expect("history.delete audit row");
    assert_eq!(row.actor, "ui");
    assert_eq!(row.details["query"], json!("w2-delete-me"));
    assert_eq!(row.details["clicks_removed"], 1);
    assert!(row.request_id.is_some());

    // Deleting a missing row is a 404, a malformed id a 400.
    let (status, _) = call(
        &app,
        Request::builder()
            .method(Method::DELETE)
            .uri(format!("/api/history/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        Request::builder()
            .method(Method::DELETE)
            .uri("/api/history/not-a-number")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Row affordances: re-run link, copy-json link, payload link on a cached
/// row, htmx delete, full request id in the footer.
#[tokio::test]
async fn history_row_actions_and_request_id() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-actions").await;

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"href="/search?q=w2-actions""#),
        "re-run link: {body}"
    );
    assert!(
        body.contains(r#"href="/api/search?q=w2-actions" class="copy-json""#),
        "copy-json link: {body}"
    );
    assert!(
        body.contains(r#"href="/cache?q=w2-actions#"#),
        "payload link on a cached row: {body}"
    );
    assert!(
        body.contains(r#"hx-delete="/api/history/"#),
        "htmx delete: {body}"
    );
    assert!(
        body.contains(r#"hx-headers='{"X-Cauce-Client":"ui"}'"#),
        "delete sends the ui client header: {body}"
    );
    // The footer carries the full request id as selectable text, never
    // abbreviated.
    let marker = r#"class="request-id">"#;
    let pos = body
        .find(marker)
        .unwrap_or_else(|| panic!("request id element missing: {body}"))
        + marker.len();
    let id = &body[pos..pos + 36];
    assert!(
        id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') && id.matches('-').count() == 4,
        "full request id in the footer, got {id:?}: {body}"
    );
    assert!(
        !body.contains(r#"title=""#),
        "no abbreviated id with a title tooltip: {body}"
    );
}

/// Empty state: a fresh store renders the one-sentence empty line; a
/// filter that matches nothing renders the filtered-empty sentence naming
/// the active filter.
#[tokio::test]
async fn history_page_empty_state() {
    let (app, _state, _tmp) = app();
    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("nothing searched yet. run a search and it lands here."),
        "{body}"
    );

    search(&app, "w2-empty-check").await;
    let (status, body) = get_html(&app, "/history?q=no-such-query").await;
    assert_eq!(status, StatusCode::OK);
    // Askama escapes the quotes around the query text.
    assert!(
        body.contains("no searches match") && body.contains("no-such-query"),
        "filtered-empty names the active q filter: {body}"
    );
    // `clear` is offered while a filter is active, hidden otherwise.
    assert!(body.contains(">clear<"), "{body}");
    let (_, body) = get_html(&app, "/history").await;
    assert!(!body.contains(">clear<"), "{body}");
}

/// `Accept: application/json` on `/history` is the same data path as
/// `/api/history` — and `Accept: text/html` on `/api/history` renders the
/// page: one handler, negotiated (W2-02 settled input).
#[tokio::test]
async fn history_page_json_negotiation() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-json-neg").await;

    let (status, body) = get_json(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().expect("json array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["kind"], "search");
    assert_eq!(rows[0]["query"], "w2-json-neg");

    let (status, body) = get_html(&app, "/api/history").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"<table class="history">"#),
        "/api/history under text/html renders the page: {body}"
    );
    assert_eq!(search_rows(&body), 1);
}

/// HTML rows equal the JSON rows for the same params (rubric 9.2: same
/// params, same defaults, same row set).
#[tokio::test]
async fn history_page_html_rows_match_json_rows() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-parity-a").await;
    search(&app, "w2-parity-b").await;
    let beacon = json!({
        "url": "https://parity.example.com/x",
        "title": "parity click",
        "position": 0,
        "query_hash": query_hash("w2-parity-a"),
    });
    let (status, _) = call(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/click")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&beacon).unwrap()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    for (uri, api_uri) in [
        ("/history", "/api/history"),
        ("/api/history?since=24h", "/api/history?since=24h"),
    ] {
        let (status, feed) = get_json(&app, api_uri).await;
        assert_eq!(status, StatusCode::OK);
        let feed = feed.as_array().unwrap();
        let searches = feed.iter().filter(|i| i["kind"] == "search").count();

        let (status, body) = get_html(&app, uri).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            search_rows(&body),
            searches,
            "HTML row count == JSON search count for {uri}"
        );
        for item in feed {
            if item["kind"] == "search" {
                let q = item["query"].as_str().unwrap();
                assert!(body.contains(q), "{uri}: row for {q} missing");
            }
        }
    }
}

/// Header stats line: `N searches in last 24h · N total · N clicks today`.
#[tokio::test]
async fn history_stats_line_counts_searches_and_clicks() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-stats-a").await;
    search(&app, "w2-stats-b").await;
    let beacon = json!({
        "url": "https://stats.example.com/x",
        "position": 0,
        "query_hash": query_hash("w2-stats-a"),
    });
    let (status, _) = call(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/click")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&beacon).unwrap()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("2 searches in last 24h · 2 total · 1 clicks today"),
        "stats line: {body}"
    );
}

/// Exactly 200 available rows are not reported as truncated.
#[tokio::test]
async fn history_page_exactly_200_rows_is_not_marked_capped() {
    let (app, state, _tmp) = app();
    let now = chrono::Utc::now();
    for i in 0..200 {
        state
            .store()
            .log_search(log_row(
                now - chrono::Duration::seconds(i64::from(i)),
                &format!("w2-exact-cap-{i}"),
                LogSource::Network,
            ))
            .await
            .expect("log_search");
    }

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(search_rows(&body), 200);
    assert!(
        !body.contains("use the filters to reach older searches"),
        "exactly 200 rows are not truncated: {body}"
    );
}

/// The 200-row cap: more rows in the feed still render exactly 200.
#[tokio::test]
async fn history_page_caps_at_200_rows() {
    let (app, state, _tmp) = app();
    let now = chrono::Utc::now();
    for i in 0..205 {
        state
            .store()
            .log_search(log_row(
                now - chrono::Duration::seconds(i64::from(i)),
                &format!("w2-cap-{i}"),
                LogSource::Network,
            ))
            .await
            .expect("log_search");
    }

    let (status, body) = get_html(&app, "/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(search_rows(&body), 200, "page caps at 200 rows: {body}");
    assert!(
        body.contains("showing 200 of 205 · use the filters to reach older searches"),
        "the cap note renders when the feed is truncated: {body}"
    );
}

/// Blank `q=` is no filter: JSON and HTML agree, the page shows the
/// fresh-store empty copy and offers no `clear`.
#[tokio::test]
async fn history_blank_q_is_no_filter() {
    let (app, _state, _tmp) = app();

    let (status, body) = get_html(&app, "/history?since=all&q=").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("nothing searched yet"),
        "blank q renders the unfiltered empty state: {body}"
    );
    assert!(
        !body.contains(">clear<"),
        "no clear link when no filter is active: {body}"
    );

    search(&app, "w2-blank-q").await;
    let (status, feed) = get_json(&app, "/api/history?q=%20").await;
    assert_eq!(status, StatusCode::OK);
    let feed = feed.as_array().unwrap();
    assert_eq!(
        feed.iter().filter(|i| i["kind"] == "search").count(),
        1,
        "whitespace q must not filter rows: {feed:?}"
    );
}
