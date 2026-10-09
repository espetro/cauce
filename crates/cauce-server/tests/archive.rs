//! `GET /api/archive` + `GET/POST/DELETE /api/pages[/{url}]` route tests
//! (W5-02): the JSON route answers `{query, results, request_id}` and
//! `DELETE /api/pages/{url}` removes the row from `pages` and
//! `pages_fts`, audited. The HTMX `/archive` page was replaced by the
//! SPA (FX-05) — page-level behaviour lives in `web/` e2e coverage.

// `archive_search`/`pages_get`/`pages_delete` mount only in `archive`
// builds.
#![cfg(feature = "archive")]

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use cauce_core::AuditFilter;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod support;
use support::*;

const NEWS_HTML: &str =
    include_str!("../../cauce-core/tests/fixtures/archive/01-news-article.html");

/// `POST {"url": ...}` as JSON (the `tests/pages.rs` helper's twin).
fn post_pages(body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/pages")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// `app()` under `[archive] allow_private`: the wiremock origin is
/// loopback, which #189's egress guard refuses by default.
fn app_allow_private() -> (axum::Router, cauce_server::AppState, tempfile::TempDir) {
    let mut config = cauce_core::config::Config::default();
    config.archive.allow_private = true;
    let (state, tmp) = test_state_with_config(config);
    (cauce_server::build_router(state.clone()), state, tmp)
}

/// Index the news-article fixture through a mock origin; returns the
/// stored page URL.
async fn index_news(router: &axum::Router) -> String {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/article"))
        .respond_with(ResponseTemplate::new(200).set_body_string(NEWS_HTML))
        .mount(&server)
        .await;
    let page_url = format!("{}/article", server.uri());
    let (status, _headers, body) = call_json(router, post_pages(json!({ "url": page_url }))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    page_url
}

/// `DELETE {uri}` as the UI sends it (`X-Cauce-Client: ui`).
fn delete_uri(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::DELETE)
        .uri(uri)
        .header("X-Cauce-Client", "ui")
        .body(Body::empty())
        .unwrap()
}

/// Percent-encode a page URL into the `{url}` path segment.
fn enc(url: &str) -> String {
    url::form_urlencoded::byte_serialize(url.as_bytes()).collect()
}

/// The JSON surface: `?q=` finds the indexed fixture by body phrase with
/// a clean snippet (no `PAGE_MARK` delimiters on the wire); the bare
/// route browses newest-first; unknown/malformed params are 400s.
#[tokio::test]
async fn api_archive_searches_and_lists() {
    let (router, _state, _tmp) = app_allow_private();
    let page_url = index_news(&router).await;

    // `?q=` — body-phrase FTS hit.
    let (status, body) = get_json(&router, "/api/archive?q=night-time%20dredging").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["query"], "night-time dredging");
    assert!(body["request_id"].as_str().is_some_and(|s| !s.is_empty()));
    let results = body["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["url"], page_url);
    assert!(results[0]["title"].as_str().is_some_and(|t| !t.is_empty()));
    let snippet = results[0]["snippet"].as_str().expect("snippet string");
    assert!(snippet.contains("night-time dredging"), "{snippet}");
    assert!(
        !snippet.contains(cauce_core::PAGE_MARK_OPEN),
        "wire snippets strip the mark delimiters: {snippet}"
    );
    assert!(results[0]["fetched_at"].as_str().is_some());
    assert!(results[0]["score"].is_number());

    // A miss is an empty result set, not an error.
    let (status, body) = get_json(&router, "/api/archive?q=zebra").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["results"].as_array().expect("results").is_empty());

    // No `q` — the browsing arm (newest first, no score).
    let (status, body) = get_json(&router, "/api/archive").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["query"].is_null());
    assert_eq!(body["has_more"], false);
    let results = body["results"].as_array().expect("results array");
    assert!(results.iter().any(|r| r["url"] == page_url));
    assert!(results[0]["score"].is_null() || results[0].get("score").is_none());

    // Blank `q=` reads as the browsing arm; unknown and malformed params
    // are 400s (the shared QueryParams contract).
    let (status, body) = get_json(&router, "/api/archive?q=").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _h, body) = get(&router, "/api/archive?bogus=1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "bad_request");
    let (status, _h, body) = get(&router, "/api/archive?limit=abc").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "bad_request");
    let (status, _h, body) = get(&router, "/api/archive?q=x&q=y").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "bad_request");
}

/// `DELETE /api/pages/{url}` removes the row from `pages` AND
/// `pages_fts`, writes the `page.delete` audit row, and 404s on a miss
/// (including the repeat delete).
#[tokio::test]
async fn delete_is_audited_and_evicts_fts() {
    let (router, state, _tmp) = app_allow_private();
    let page_url = index_news(&router).await;
    let uri = format!("/api/pages/{}", enc(&page_url));

    let (status, _headers, body) = call_json(&router, delete_uri(&uri)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);
    assert_eq!(body["url"], page_url);

    // Gone from `pages` and from `pages_fts` (the delete trigger ran).
    assert!(
        state
            .store()
            .get_page(&url::Url::parse(&page_url).unwrap())
            .await
            .unwrap()
            .is_none()
    );
    let (status, body) = get_json(&router, "/api/archive?q=dredging").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["results"].as_array().expect("results").is_empty());

    // `page.delete` audit row against the normalized URL.
    let audits = state
        .store()
        .list_audit(&AuditFilter {
            action: Some("page.delete".to_string()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(audits.len(), 1, "{audits:?}");
    assert_eq!(audits[0].target, page_url);
    assert_eq!(audits[0].actor, "ui");

    // A second delete (or a never-indexed URL) is `not_found`.
    let (status, _h, body) = call_json(&router, delete_uri(&uri)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_envelope(&body, "not_found");
    let (status, _h, body) = call_json(
        &router,
        delete_uri("/api/pages/https%3A%2F%2Fnever.example.com"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_envelope(&body, "not_found");
}

/// `GET /api/pages/{url}` returns the stored row as JSON (the SPA row
/// expander reads `markdown` from it); a missing row is the JSON
/// envelope 404.
#[tokio::test]
async fn pages_get_json_row_and_missing_404() {
    let (router, _state, _tmp) = app_allow_private();
    let page_url = index_news(&router).await;

    let (status, row) = get_json(&router, &format!("/api/pages/{}", enc(&page_url))).await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["url"].as_str(), Some(page_url.as_str()));
    assert!(
        row["markdown"]
            .as_str()
            .unwrap_or("")
            .contains("night-time dredging"),
        "markdown in the row: {row}"
    );

    let (status, body) = get_json(&router, "/api/pages/https%3A%2F%2Fnever.example.com").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
