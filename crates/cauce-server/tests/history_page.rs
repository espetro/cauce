//! `GET /api/history` + `DELETE /api/history/{id}` wire tests (W2-02).
//! The HTMX `/history` page was replaced by the SPA (FX-05) — the page
//! read the same `list_history` data path, so the contract that survives
//! is the JSON feed: `HistoryItem[]` (search | click | answer rows,
//! newest first), the `since`/`q`/`cached`/`origin`/`limit` filter
//! grammar, and the audited row delete.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The HTMX pages exist only in `ui` builds (W1-12 feature gates).
#![cfg(feature = "ui")]

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use cauce_core::{
    AuditFilter, CacheKey, ClientKind, EngineId, LogSource, SafeSearch, SearchLogRow,
    SearchRequest, Tier,
};
use serde_json::{Value, json};

mod support;
use support::*;

async fn search(router: &Router, q: &str) {
    let (status, _) = get_json(router, &format!("/api/search?q={q}")).await;
    assert_eq!(status, StatusCode::OK, "search {q:?} failed");
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
        origin: cauce_core::SearchOrigin::User,
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
            origin: cauce_core::SearchOrigin::User,
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
        origin: cauce_core::SearchOrigin::User,
    }
}

/// POST the click beacon (`POST /api/click`).
async fn click(router: &Router, body: serde_json::Value) {
    let (status, _) = call(
        router,
        Request::builder()
            .method(Method::POST)
            .uri("/api/click")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

fn items(body: &Value) -> &Vec<Value> {
    body.as_array().expect("history feed is a JSON array")
}

/// The feed lists logged searches newest-first with the row fields the
/// SPA renders (query, engines, source, tier, id, query_hash).
#[tokio::test]
async fn history_api_lists_searches_newest_first() {
    let (app, _state, _tmp) = app();

    search(&app, "w2-history-alpha").await;
    search(&app, "w2-history-beta").await;
    search(&app, "w2-history-gamma").await;

    let (status, body) = get_json(&app, "/api/history?origin=all").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = items(&body);
    assert_eq!(rows.len(), 3, "{body}");
    for (i, q) in ["w2-history-gamma", "w2-history-beta", "w2-history-alpha"]
        .iter()
        .enumerate()
    {
        assert_eq!(rows[i]["kind"], "search", "{body}");
        assert_eq!(rows[i]["query"], *q, "rows must be newest-first: {body}");
        assert!(
            rows[i]["engines"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e == "replay"),
            "engines names replay: {body}"
        );
        assert!(rows[i]["id"].is_i64(), "row id for the delete link: {body}");
        assert_eq!(
            rows[i]["query_hash"].as_str(),
            Some(query_hash(q).as_str()),
            "{body}"
        );
    }
}

/// Click beacons join the feed as `kind:"click"` rows merged
/// newest-first — the SPA renders them nested under the search row that
/// shares `query_hash` (or as a click-only row when none matches).
#[tokio::test]
async fn history_api_merges_click_rows() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-click-join").await;

    click(
        &app,
        json!({
            "url": "https://example.com/w2-click",
            "title": "W2 clicked title",
            "position": 0,
            "query_hash": query_hash("w2-click-join"),
        }),
    )
    .await;
    click(
        &app,
        json!({
            "url": "https://tokio.rs/tutorial",
            "title": "Tokio tutorial",
            "position": 1,
            "query_hash": query_hash("w2-never-searched"),
        }),
    )
    .await;

    let (status, body) = get_json(&app, "/api/history?origin=all").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = items(&body);
    let clicks: Vec<&Value> = rows.iter().filter(|r| r["kind"] == "click").collect();
    assert_eq!(clicks.len(), 2, "{body}");
    let matched = clicks
        .iter()
        .find(|r| r["query_hash"].as_str() == Some(query_hash("w2-click-join").as_str()))
        .expect("click joining the search row");
    assert_eq!(matched["url"], "https://example.com/w2-click");
    assert_eq!(matched["title"], "W2 clicked title");
    assert_eq!(matched["position"], 0);
    let stray = clicks
        .iter()
        .find(|r| r["url"] == "https://tokio.rs/tutorial")
        .expect("unmatched click is its own row");
    assert_eq!(stray["position"], 1);
}

/// `since` windows hide backdated rows on the same filter grammar the
/// page used (`24h`/`7d`/`30d`; `all` is the explicit no-window).
#[tokio::test]
async fn history_api_since_filter_hides_backdated_row() {
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

    let (status, body) = get_json(&app, "/api/history?since=24h").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = items(&body);
    assert!(
        rows.iter().any(|r| r["query"] == "w2-history-fresh"),
        "{body}"
    );
    assert!(
        rows.iter().all(|r| r["query"] != "w2-history-ancient"),
        "backdated row must be hidden by since=24h: {body}"
    );

    let (status, body) = get_json(&app, "/api/history?since=all").await;
    assert_eq!(status, StatusCode::OK);
    let rows = items(&body);
    assert!(
        rows.iter().any(|r| r["query"] == "w2-history-ancient"),
        "{body}"
    );

    for window in ["7d", "30d"] {
        let (status, _) = get_json(&app, &format!("/api/history?since={window}")).await;
        assert_eq!(status, StatusCode::OK, "since={window}");
    }
}

/// `q` is a substring filter over the stored query; a blank `q=` is no
/// filter at all (one grammar, no params-vs-page drift).
#[tokio::test]
async fn history_api_q_filter_and_blank_q() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-q-hit").await;
    search(&app, "w2-other").await;

    let (status, body) = get_json(&app, "/api/history?origin=all&q=q-hit").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = items(&body);
    assert_eq!(rows.len(), 1, "{body}");
    assert_eq!(rows[0]["query"], "w2-q-hit");

    let (status, body) = get_json(&app, "/api/history?origin=all&q=").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(items(&body).len(), 2, "blank q= keeps every row: {body}");
}

/// `origin=all` is the explicit no-filter; `origin=user|agent` narrows;
/// a bad value is a 400.
#[tokio::test]
async fn history_api_origin_filter() {
    let (app, state, _tmp) = app();
    // A user-origin row (the SPA's "mine") next to an api-origin search.
    state
        .store()
        .log_search(log_row(
            chrono::Utc::now(),
            "w2-origin-user",
            LogSource::Network,
        ))
        .await
        .expect("user log row");
    search(&app, "w2-origin-api").await;

    let (status, body) = get_json(&app, "/api/history?origin=user").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = items(&body);
    assert!(
        rows.iter().any(|r| r["query"] == "w2-origin-user"),
        "{body}"
    );
    assert!(
        rows.iter().all(|r| r["query"] != "w2-origin-api"),
        "origin=user hides the api-origin row: {body}"
    );

    let (status, _) = get_json(&app, "/api/history?origin=bogus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "bad origin is a 400");
}

/// `cached=1` keeps only rows whose query has a live `cache_entries`
/// row — the deleted entry drops the row off the feed.
#[tokio::test]
async fn history_api_cached_filter() {
    let (app, _state, _tmp) = app();
    search(&app, "w2-cached-yes").await;
    search(&app, "w2-cached-maybe").await;

    let (status, body) = get_json(&app, "/api/history?origin=all&cached=1").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(items(&body).len(), 2, "{body}");

    let (status, body) = call(
        &app,
        Request::builder()
            .method(Method::DELETE)
            .uri(format!("/api/cache/{}", query_hash("w2-cached-maybe")))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = get_json(&app, "/api/history?origin=all&cached=1").await;
    assert_eq!(status, StatusCode::OK);
    let rows = items(&body);
    assert_eq!(rows.len(), 1, "{body}");
    assert_eq!(rows[0]["query"], "w2-cached-yes", "{body}");
}

/// The 200-row cap (`HISTORY_LIMIT`): 205 seeded rows return exactly 200.
#[tokio::test]
async fn history_api_caps_at_200_rows() {
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

    let (status, body) = get_json(&app, "/api/history?origin=all").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(items(&body).len(), 200, "{body}");
}

/// Delete is audited (`history.delete`, actor `ui` via `X-Cauce-Client`)
/// and cascades to the row's clicks.
#[tokio::test]
async fn history_delete_removes_row_clicks_and_audits() {
    let (app, state, _tmp) = app();
    search(&app, "w2-delete-me").await;

    click(
        &app,
        json!({
            "url": "https://example.com/w2-delete",
            "position": 0,
            "query_hash": query_hash("w2-delete-me"),
        }),
    )
    .await;

    let (status, body) = get_json(&app, "/api/history?origin=all").await;
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

    // The row and its clicks are gone from the feed.
    let (_, feed) = get_json(&app, "/api/history?origin=all").await;
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
