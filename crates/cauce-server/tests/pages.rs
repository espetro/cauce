//! `/api/pages` route tests (W5-01): `POST /api/pages` fetches through a
//! mock origin and yields the stored `pages` row (the exit criterion —
//! one call, one row), `GET /api/pages/{url}` reads it back by
//! percent-encoded URL, and the error envelope maps match the click
//! handler's (`bad_request`/`upstream_error`/`not_found`).

// `pages_index`/`pages_get` mount only in `archive` builds.
#![cfg(feature = "archive")]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cauce_core::AuditFilter;
use cauce_server::build_router;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod support;
use support::*;

const NEWS_HTML: &str =
    include_str!("../../cauce-core/tests/fixtures/archive/01-news-article.html");

/// `POST {"url": ...}` as JSON.
fn post_pages(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Exit criterion: `POST /api/pages` fetches, extracts and writes the
/// `pages` row — the 201 body IS the row — and `GET /api/pages/{url}`
/// serves it by the percent-encoded normalized URL. The `page.index`
/// audit row lands like the other audited writes.
#[tokio::test]
async fn post_indexes_and_get_reads_back() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/article"))
        .respond_with(ResponseTemplate::new(200).set_body_string(NEWS_HTML))
        .mount(&server)
        .await;

    let (router, state, _tmp) = app();
    let page_url = format!("{}/article", server.uri());
    let (status, _headers, body) = call_json(
        &router,
        post_pages(
            "/api/pages",
            json!({ "url": format!("{page_url}?utm_source=x") }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["url"], page_url);
    assert!(body["title"].as_str().is_some_and(|t| !t.is_empty()));
    assert!(
        body["markdown"]
            .as_str()
            .is_some_and(|m| m.contains("night-time dredging"))
    );
    assert!(body["byte_len"].as_u64().is_some_and(|n| n > 0));

    // The row is queryable through the store (normalized: `utm_source`
    // stripped).
    let stored = state
        .store()
        .get_page(&url::Url::parse(&page_url).unwrap())
        .await
        .unwrap()
        .expect("pages row written");

    // `GET /api/pages/{url}` — the whole URL percent-encoded into one
    // segment — returns the same row.
    let encoded: String = url::form_urlencoded::byte_serialize(page_url.as_bytes()).collect();
    let uri = format!("/api/pages/{encoded}");
    let (status, _headers, body) = get(&router, &uri).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["url"], stored.url.as_str());
    assert_eq!(body["markdown"], stored.markdown);

    // Audit: `page.index` against the indexed URL.
    let audits = state
        .store()
        .list_audit(&AuditFilter {
            action: Some("page.index".to_string()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].target, page_url);
}

/// Caller faults map to 4xx: a bad URL is `bad_request`, an unindexed
/// `GET` is `not_found`, a malformed `{url}` segment is `bad_request`.
#[tokio::test]
async fn caller_faults_map_to_4xx() {
    let (router, _state, _tmp) = app();

    let (status, _h, body) = call_json(
        &router,
        post_pages("/api/pages", json!({ "url": "not a url" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "bad_request");

    // A segment that decodes to something `Url::parse` rejects.
    let (status, _h, body) = get(&router, "/api/pages/not%20a%20url").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "bad_request");

    let (status, _h, body) = get(
        &router,
        "/api/pages/https%3A%2F%2Fnever-indexed.example.com",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_envelope(&body, "not_found");
}

/// An upstream failure is `502 upstream_error`, not a 4xx and not a
/// written row.
#[tokio::test]
async fn upstream_error_maps_to_502() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/broken"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let (router, state, _tmp) = app();
    let page_url = format!("{}/broken", server.uri());
    let (status, _h, body) = call_json(
        &router,
        post_pages("/api/pages", json!({ "url": page_url })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_envelope(&body, "upstream_error");
    assert!(
        state
            .store()
            .get_page(&url::Url::parse(&page_url).unwrap())
            .await
            .unwrap()
            .is_none()
    );
}

/// `GET /` renders the click beacon only when the archive pipeline is up
/// and `[archive] index_on_click` is on (the default).
#[cfg(feature = "ui")]
#[tokio::test]
async fn beacon_renders_only_when_enabled() {
    let (router, _state, _tmp) = app();
    let (_status, html) = get_html(&router, "/").await;
    assert!(html.contains("/api/pages"), "beacon must render by default");

    let mut config = cauce_core::config::Config::default();
    config.archive.index_on_click = false;
    let (state, _tmp2) = test_state_with_config(config);
    let router = build_router(state);
    let (_status, html) = get_html(&router, "/").await;
    assert!(
        !html.contains("/api/pages"),
        "index_on_click = false must drop the beacon",
    );
}
