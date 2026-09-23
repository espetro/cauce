//! OpenSearch descriptor tests (W2-11).
//!
//! `GET /opensearch.xml` is validated against the OpenSearch 1.1 schema in
//! `tests/fixtures/opensearch-1-1.xsd` (no official XSD exists; the fixture
//! is written from the draft-6 spec text and targets XSD 1.1 so `xs:all`
//! children may repeat). Validation runs through `oxixml-schema`, a
//! pure-Rust XSD 1.0/1.1 engine — no libxml2 C dependency in CI.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// The descriptor is a `requires: "ui"` route (it only exists to point at
// the HTMX pages), so the whole file compiles out without `ui`.
#![cfg(feature = "ui")]

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use cauce_core::SearchPipeline;
use cauce_core::StoreTuning;
use cauce_core::config::Config;
use cauce_engines::{Replay, ReplayOpts};
use cauce_server::{AppState, build_router};
use cauce_store_sqlite::SqliteStore;
use oxixml_schema::{SchemaSet, XsdVersion};
use tower::ServiceExt;

/// The OpenSearch 1.1 description-document schema this descriptor must
/// satisfy; see the fixture's own comment for provenance.
const XSD: &str = include_str!("fixtures/opensearch-1-1.xsd");

fn app() -> (Router, tempfile::TempDir) {
    app_with_config(Config::default())
}

fn app_with_config(config: Config) -> (Router, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("cauce.db"), StoreTuning::default()).expect("store"),
    );
    let pipeline = Arc::new(SearchPipeline::new(
        store.clone(),
        vec![Arc::new(Replay::new(ReplayOpts::default()))],
    ));
    let state = AppState::new(pipeline, store, config);
    (build_router(state), tmp)
}

async fn get(router: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let request = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        headers,
        String::from_utf8(bytes.to_vec()).expect("utf8"),
    )
}

/// Acceptance: the served descriptor validates against the OpenSearch 1.1
/// schema.
#[tokio::test]
async fn opensearch_descriptor_validates_against_schema() {
    let (router, _tmp) = app();
    let (status, headers, body) = get(&router, "/opensearch.xml").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        headers[header::CONTENT_TYPE].to_str().unwrap(),
        "application/opensearchdescription+xml"
    );

    let mut set = SchemaSet::new().with_version(XsdVersion::V1_1);
    set.add_document(None, XSD).expect("fixture schema parses");
    let schema = set.compile().expect("fixture schema compiles");
    let report = schema.validate_str(&body);
    assert!(
        report.valid,
        "descriptor failed schema validation: {:?}",
        report.first_error()
    );
}

/// The descriptor uses the configured HTTPS origin for both templates and
/// ignores hostile request and forwarded host headers.
#[tokio::test]
async fn opensearch_descriptor_uses_canonical_origin_not_request_host() {
    let mut config = Config::default();
    config.server.public_url = Some("https://search.localhost".to_string());
    let (router, _tmp) = app_with_config(config);
    let request = Request::builder()
        .method(Method::GET)
        .uri("/opensearch.xml")
        .header(header::HOST, "localhost\" x=\"bad.localhost")
        .header("x-forwarded-host", "attacker.invalid")
        .header("x-forwarded-proto", "http")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(bytes.to_vec()).expect("utf8");
    assert_eq!(status, StatusCode::OK, "{body}");

    assert!(
        body.contains("https://search.localhost/search?q={searchTerms}"),
        "results URL must use the configured HTTPS origin: {body}"
    );
    assert!(
        body.contains("https://search.localhost/api/suggest?q={searchTerms}"),
        "suggestions URL must use the configured HTTPS origin: {body}"
    );
    assert!(
        !body.contains("bad.localhost") && !body.contains("attacker.invalid"),
        "untrusted request headers must not reach the descriptor: {body}"
    );
    assert!(
        body.contains("application/x-suggestions+json"),
        "suggestions Url placeholder missing: {body}"
    );
}

/// Browsers discover the descriptor through `<link rel="search">` in the
/// page head of both mounted pages.
#[tokio::test]
async fn pages_link_to_opensearch_descriptor() {
    let (router, _tmp) = app();
    for uri in ["/", "/search?q=link-check"] {
        let (status, _, body) = get(&router, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert!(
            body.contains(r#"<link rel="search" type="application/opensearchdescription+xml""#)
                && body.contains(r#"href="/opensearch.xml""#),
            "{uri} should link the descriptor in <head>"
        );
    }
}
