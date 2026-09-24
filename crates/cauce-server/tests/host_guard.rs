//! W1-13: Host/Origin loopback guard — foreign `Host` is 403 before
//! routing, mutating methods reject foreign/`null` `Origin`, and the
//! allow list honours a configured bind host.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cauce_server::{ROUTES, RouterOptions, build_router_opts};

mod support;
use support::*;

/// A request with a foreign `Host` is 403 before routing, whatever the
/// path or method — the DNS-rebinding shape. Loopback and portless-alias
/// hosts (`*.localhost`) pass.
#[tokio::test]
async fn foreign_host_is_forbidden() {
    let (router, _state, _tmp) = app();

    for (method, uri) in [
        ("GET", "/api/search?q=x"),
        ("GET", "/health"),
        ("GET", "/"),
        ("DELETE", "/api/cache?all=true"),
        ("PUT", "/api/config"),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "attacker.example.com")
            .body(Body::empty())
            .unwrap();
        let (status, _, body) = call_json(&router, request).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {body}");
        assert_envelope(&body, "forbidden");
    }

    // A foreign Host is rejected even on an undeclared path: the guard
    // runs before routing.
    let request = Request::builder()
        .method("GET")
        .uri("/nope")
        .header("host", "attacker.example.com")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    for host in [
        "127.0.0.1",
        "127.0.0.1:4479",
        "localhost",
        "localhost:4479",
        "[::1]",
        "[::1]:4479",
        "search.localhost",
        "cauce.localhost:443",
    ] {
        let request = Request::builder()
            .method("GET")
            .uri("/health")
            .header("host", host)
            .body(Body::empty())
            .unwrap();
        let (status, _, body) = call_json(&router, request).await;
        assert_eq!(status, StatusCode::OK, "host {host}: {body}");
    }
}

/// Every mutating `ROUTES` row carries the guard: a foreign `Origin` on a
/// mutating method is 403 before the handler runs. The table is iterated
/// rather than enumerated so rows added in later waves are covered
/// automatically; `*` (the MCP row) is probed as POST.
#[tokio::test]
async fn mutating_routes_reject_foreign_origin() {
    let (router, _state, _tmp) = app();

    let mut mutating = 0;
    for spec in ROUTES
        .iter()
        .filter(|s| !matches!(s.method, "GET" | "HEAD" | "OPTIONS"))
    {
        mutating += 1;
        let method = if spec.method == "*" {
            "POST"
        } else {
            spec.method
        };
        let path = spec
            .path
            .replace("{key}", &"a".repeat(64))
            .replace("{id}", "x")
            .replace("{url}", "x");
        let request = Request::builder()
            .method(method)
            .uri(&path)
            .header("origin", "https://attacker.example.com")
            .body(Body::empty())
            .unwrap();
        let (status, _, body) = call_json(&router, request).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{} {} must carry the guard: {body}",
            spec.method,
            spec.path
        );
        assert_envelope(&body, "forbidden");
    }
    assert!(mutating > 0, "the table must contain mutating rows");
}

/// `Origin` absent (CLI/server-to-server) or same-host passes the guard;
/// `null` and foreign origins on mutating methods do not.
#[tokio::test]
async fn mutating_origin_must_be_same_host() {
    let (router, _state, _tmp) = app();
    let click = || {
        Request::builder()
            .method("POST")
            .uri("/api/click")
            .header("content-type", "application/json")
    };

    // Absent Origin reaches the handler (204 = click accepted).
    let (status, _, _) = call_json(
        &router,
        click()
            .body(Body::from(r#"{"url":"https://example.com/a"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Same-host origins pass, whatever the port or loopback alias.
    for origin in [
        "http://localhost:4479",
        "http://localhost:3000",
        "https://search.localhost",
        "http://127.0.0.1:4479",
        "http://[::1]:4479",
    ] {
        let (status, _, body) = call_json(
            &router,
            click()
                .header("origin", origin)
                .body(Body::from(r#"{"url":"https://example.com/a"}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "origin {origin}: {body}");
    }

    // `null` (sandboxed frame) and foreign origins are rejected.
    for origin in ["null", "https://attacker.example.com", "not a uri"] {
        let (status, _, body) = call_json(
            &router,
            click()
                .header("origin", origin)
                .body(Body::from(r#"{"url":"https://example.com/a"}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "origin {origin}: {body}");
        assert_envelope(&body, "forbidden");
    }
}

/// The guard's allow list honours a configured bind host beyond the
/// loopback names (a loopback-resolving alias like `lvh.me`).
#[tokio::test]
async fn guard_accepts_configured_bind_host() {
    let (state, _tmp) = test_state();
    let router = build_router_opts(
        state,
        RouterOptions {
            bind_host: "cauce.lvh.me".to_string(),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .header("host", "cauce.lvh.me:4479")
        .body(Body::empty())
        .unwrap();
    let (status, _, body) = call_json(&router, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
