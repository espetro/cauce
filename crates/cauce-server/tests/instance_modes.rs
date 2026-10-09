//! FX-07 instance modes (frontend plan §7.4): the `Capabilities` payload
//! per mode + role, the public-mode admin matrix (every `RouteAuth::Admin`
//! row 401s without the `[auth] admin_tokens` bearer, open rows stay
//! open), and the privacy gates — public instances write no `search_log`
//! rows and `archive.enabled = false` takes the whole archive surface
//! down.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use cauce_core::config::Config;
use serde_json::Value;
use tower::ServiceExt;

mod support;
use support::*;

/// A public instance with one static admin token.
fn public_config() -> Config {
    let mut cfg = Config::default();
    cfg.server.public_instance = true;
    cfg.auth.admin_tokens = vec!["test-admin-token".to_string()];
    cfg
}

/// `req` plus a `Authorization: Bearer <token>` header.
fn admin_req(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn capabilities_local_mode() {
    let (router, _state, _tmp) = app_with_factory(Config::default());
    let (status, json) = get_json(&router, "/api/capabilities").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["mode"], "local");
    assert_eq!(json["role"], "admin");
    let flags = &json["flags"];
    assert_eq!(flags["adminSurface"], true);
    assert_eq!(flags["serverHistory"], true);
    assert_eq!(flags["archiving"], true);
    assert_eq!(flags["sharedStats"], true);
}

#[tokio::test]
async fn capabilities_public_mode_roles() {
    let (router, _state, _tmp) = app_with_factory(public_config());

    // No credential: user role, no admin surface, no server-side state.
    let (status, json) = get_json(&router, "/api/capabilities").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["mode"], "public");
    assert_eq!(json["role"], "user");
    let flags = &json["flags"];
    assert_eq!(flags["adminSurface"], false);
    assert_eq!(flags["serverHistory"], false);
    assert_eq!(flags["archiving"], true);
    assert_eq!(flags["sharedStats"], false);

    // The admin token flips role and adminSurface only.
    let resp = router
        .clone()
        .oneshot(admin_req("GET", "/api/capabilities"))
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["role"], "admin");
    assert_eq!(json["flags"]["adminSurface"], true);
    assert_eq!(json["flags"]["serverHistory"], false);

    // A wrong token is still a user.
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/capabilities")
                .header(header::AUTHORIZATION, "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["role"], "user");
}

#[tokio::test]
async fn instance_info_payload() {
    let (router, _state, _tmp) = app_with_factory(public_config());
    let (status, json) = get_json(&router, "/api/instance").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["name"], "cauce");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["engineCount"], 1);
    // Every configured engine id — including disabled ones (ddgs is
    // `enabled=false` in `Config::default`); the SPA pin check needs it.
    assert_eq!(json["engineIds"], serde_json::json!(["replay", "ddgs"]));
    assert_eq!(json["aiEnabled"], false);
}

/// The §7.5 matrix: every `RouteAuth::Admin` row 401s without the token
/// on a public instance; the search/read surface stays open.
#[tokio::test]
async fn public_mode_admin_matrix() {
    let (router, _state, _tmp) = app_with_factory(public_config());

    const ADMIN: &[(&str, &str)] = &[
        ("GET", "/api/history"),
        ("POST", "/api/click"),
        ("DELETE", "/api/history/1"),
        ("GET", "/api/answer-log/1"),
        ("DELETE", "/api/answer-log/1"),
        ("GET", "/api/stats"),
        ("GET", "/api/cache"),
        ("GET", "/api/audit"),
        ("GET", "/api/config"),
        ("PUT", "/api/config"),
        ("GET", "/api/engines"),
        ("POST", "/api/engines/replay/reset"),
        ("POST", "/api/engines/replay/enable"),
        ("POST", "/api/engines/replay/disable"),
        ("GET", "/metrics"),
        ("GET", "/api/report"),
        ("POST", "/api/pages"),
        ("DELETE", "/api/pages/https%3A%2F%2Fexample.com"),
        ("GET", "/mcp"),
    ];
    for (method, uri) in ADMIN {
        let (status, body) = call(&router, req(method, uri)).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must 401 without the admin token, got {status}: {body}"
        );
    }

    // The public surface stays open.
    const OPEN: &[(&str, &str)] = &[
        ("GET", "/api/search?q=x"),
        ("GET", "/api/suggest?q=x"),
        ("GET", "/api/capabilities"),
        ("GET", "/api/instance"),
        ("GET", "/api/archive"),
        ("GET", "/health"),
    ];
    for (method, uri) in OPEN {
        let (status, body) = call(&router, req(method, uri)).await;
        assert!(
            status.is_success(),
            "{method} {uri} must stay open, got {status}: {body}"
        );
    }

    // The same admin rows answer 2xx/4xx-under-the-gate with the token —
    // none of them 401. `/mcp` is skipped: a credentialed GET opens the
    // streamable transport's standalone SSE stream and never returns.
    // `PUT /api/config` gets a body that keeps `admin_tokens` — an empty
    // PUT replaces the file with defaults and hot-applies `auth.admin_tokens
    // = []`, which would 401 every subsequent credentialed call.
    for (method, uri) in ADMIN.iter().filter(|(_, u)| *u != "/mcp") {
        let request = if *method == "PUT" && *uri == "/api/config" {
            Request::builder()
                .method("PUT")
                .uri("/api/config")
                .header(header::AUTHORIZATION, "Bearer test-admin-token")
                .header(header::CONTENT_TYPE, "application/toml")
                .body(Body::from(
                    "[auth]\nadmin_tokens = [\"test-admin-token\"]\n",
                ))
                .unwrap()
        } else {
            admin_req(method, uri)
        };
        let (status, _body) = call(&router, request).await;
        assert_ne!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must not 401 with the admin token"
        );
    }

    // But an unauthorized `/mcp` never reaches the transport.
    let (status, _body) = call(&router, req("GET", "/mcp")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Regression: `public_instance` is an auth boundary — an admin PUTting
/// a config that flips it off must NOT demote the live instance (the
/// change is saved, reported as `requires_restart`, and applies on the
/// next boot). Before the `commit_locked` preserve, this body flipped
/// the gate off live and opened every admin row.
#[tokio::test]
async fn config_put_cannot_demote_instance_mode_live() {
    let (router, _state, _tmp) = app_with_factory(public_config());

    let put = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .header(header::AUTHORIZATION, "Bearer test-admin-token")
        .header(header::CONTENT_TYPE, "application/toml")
        .body(Body::from("[server]\npublic_instance = false\n"))
        .unwrap();
    let (status, body) = call(&router, put).await;
    assert!(
        status.is_success(),
        "PUT must succeed, got {status}: {body}"
    );
    assert!(
        body.contains("server.public_instance"),
        "the change must be reported as requiring a restart: {body}"
    );

    // The live gate is unchanged: no-token admin rows still 401 and the
    // capabilities payload still reports public mode.
    let (status, _body) = call(&router, req("GET", "/api/config")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (_status, json) = get_json(&router, "/api/capabilities").await;
    assert_eq!(json["mode"], "public");
    assert_eq!(json["flags"]["adminSurface"], false);
}

/// Local mode is bit-for-bit: no row needs a credential.
#[tokio::test]
async fn local_mode_admin_rows_open() {
    let (router, _state, _tmp) = app_with_factory(Config::default());
    for (method, uri) in [
        ("GET", "/api/audit"),
        ("GET", "/api/config"),
        ("GET", "/api/stats"),
        ("GET", "/api/history"),
        ("GET", "/api/engines"),
        ("GET", "/metrics"),
    ] {
        let (status, _body) = call(&router, req(method, uri)).await;
        assert_ne!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must not 401 in local mode"
        );
    }
}

/// Public mode writes no `search_log` rows — the privacy gate behind
/// "history is browser-local, never server DB".
#[tokio::test]
async fn public_mode_writes_no_search_log() {
    let (router, state, _tmp) = app_with_factory(public_config());
    let (status, _body) = call(&router, req("GET", "/api/search?q=hello")).await;
    assert_eq!(status, StatusCode::OK);

    let items = state
        .store()
        .list_history(&cauce_core::HistoryFilter::default())
        .await
        .expect("history");
    assert!(
        items.is_empty(),
        "public mode must not write search_log rows: {items:?}"
    );
}

/// The same search in local mode still writes the row (bit-for-bit).
#[tokio::test]
async fn local_mode_writes_search_log() {
    let (router, state, _tmp) = app_with_factory(Config::default());
    let (status, _body) = call(&router, req("GET", "/api/search?q=hello")).await;
    assert_eq!(status, StatusCode::OK);

    let items = state
        .store()
        .list_history(&cauce_core::HistoryFilter::default())
        .await
        .expect("history");
    assert_eq!(items.len(), 1, "local mode writes the search_log row");
}

/// `archive.enabled = false` → `archiving: false` and every archive
/// surface 503s `archive_disabled` (open rows included — the flag is the
/// whole surface, not just the writes).
#[tokio::test]
async fn archive_disabled_takes_the_surface_down() {
    let mut cfg = Config::default();
    cfg.archive.enabled = false;
    let (router, _state, _tmp) = app_with_factory(cfg);

    let (_status, json) = get_json(&router, "/api/capabilities").await;
    assert_eq!(json["flags"]["archiving"], false);

    for (method, uri) in [
        ("GET", "/api/archive"),
        ("GET", "/api/pages/https%3A%2F%2Fexample.com"),
        ("DELETE", "/api/pages/https%3A%2F%2Fexample.com"),
    ] {
        let (status, body) = call(&router, req(method, uri)).await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{method} {uri} must 503 with archive.enabled=false, got {status}: {body}"
        );
        assert!(body.contains("archive_disabled"), "{method} {uri}: {body}");
    }

    // POST with a valid body must 503 too (body parses, then the
    // missing archiver reports `archive_disabled`).
    let post = Request::builder()
        .method("POST")
        .uri("/api/pages")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"url":"https://example.com"}"#))
        .unwrap();
    let (status, body) = call(&router, post).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(body.contains("archive_disabled"), "{body}");
}
