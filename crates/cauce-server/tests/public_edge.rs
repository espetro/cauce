//! PUB-01 public-instance readiness (plan
//! `2026-10-09-public-instances.md`): the `cache` column's edge headers,
//! the `[rate_limit]` per-IP bucket and the `server.max_inflight` cap —
//! scope, exemptions and the 429 + `Retry-After` contract.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use axum::body::Body;
use axum::http::HeaderName;
use axum::http::header::{AUTHORIZATION, CACHE_CONTROL, RETRY_AFTER};

const XFF: HeaderName = HeaderName::from_static("x-forwarded-for");
use axum::http::{Request, StatusCode};
use cauce_core::config::Config;
use cauce_engines::ReplayOpts;
use tower::ServiceExt;

mod support;
use support::*;

fn public_config() -> Config {
    let mut cfg = Config::default();
    cfg.server.public_instance = true;
    cfg.auth.admin_tokens = vec!["test-admin-token".to_string()];
    cfg
}

fn cc(resp: &axum::http::Response<Body>) -> String {
    resp.headers()
        .get(CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn xff_req(uri: &str, ip: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header(&XFF, ip)
        .body(Body::empty())
        .unwrap()
}

// ---------------------------------------------------------------------
// `cache:` column → Cache-Control
// ---------------------------------------------------------------------

/// `shared` rows get the `edge.ttl_s` public TTL; `private` rows get
/// `private, no-store` — including admin rows read in local mode.
#[tokio::test]
async fn edge_cache_classes() {
    let (router, _state, _tmp) = app_with_factory(Config::default());

    let resp = router
        .clone()
        .oneshot(req("GET", "/api/search?q=x"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        cc(&resp),
        "public, max-age=60, s-maxage=60, stale-while-revalidate=60"
    );

    // `/api/capabilities` varies on Authorization → private.
    let resp = router
        .clone()
        .oneshot(req("GET", "/api/capabilities"))
        .await
        .unwrap();
    assert_eq!(cc(&resp), "private, no-store");

    // An admin row is private even though it answered 200 locally.
    let resp = router
        .clone()
        .oneshot(req("GET", "/api/config"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(cc(&resp), "private, no-store");

    // `/api/instance`: one payload for every caller → shared.
    let resp = router
        .clone()
        .oneshot(req("GET", "/api/instance"))
        .await
        .unwrap();
    assert_eq!(
        cc(&resp),
        "public, max-age=60, s-maxage=60, stale-while-revalidate=60"
    );
}

/// `edge.ttl_s` drives the stamp; `edge.enabled = false` emits nothing
/// (pre-PUB-01 header shape, bit-for-bit).
#[tokio::test]
async fn edge_cache_config_knobs() {
    let mut cfg = Config::default();
    cfg.edge.ttl_s = 300;
    let (router, _state, _tmp) = app_with_factory(cfg);
    let resp = router
        .clone()
        .oneshot(req("GET", "/api/search?q=x"))
        .await
        .unwrap();
    assert_eq!(
        cc(&resp),
        "public, max-age=300, s-maxage=300, stale-while-revalidate=300"
    );

    let mut cfg = Config::default();
    cfg.edge.enabled = false;
    let (router, _state, _tmp) = app_with_factory(cfg);
    let resp = router
        .clone()
        .oneshot(req("GET", "/api/search?q=x"))
        .await
        .unwrap();
    assert_eq!(cc(&resp), "");
}

/// Error responses are never stamped (a CDN must not cache a 400).
#[tokio::test]
async fn edge_cache_skips_errors() {
    let (router, _state, _tmp) = app_with_factory(Config::default());
    // `/api/search` without `q` is a 400 on a `shared` row.
    let resp = router
        .clone()
        .oneshot(req("GET", "/api/search"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(cc(&resp), "");
}

/// Handler-set headers win: the SPA asset pipeline's own
/// `immutable`/`no-cache` stamps are not clobbered.
#[cfg(feature = "ui")]
#[tokio::test]
async fn edge_cache_preserves_handler_headers() {
    let (router, _state, _tmp) = app_with_factory(Config::default());
    let resp = router.clone().oneshot(req("GET", "/app")).await.unwrap();
    assert_eq!(cc(&resp), "no-cache");
    // A permanent redirect keeps its own headers but is marked shared.
    let resp = router.clone().oneshot(req("GET", "/")).await.unwrap();
    assert!(resp.status().is_redirection());
    assert_eq!(
        cc(&resp),
        "public, max-age=60, s-maxage=60, stale-while-revalidate=60"
    );
}

// ---------------------------------------------------------------------
// `[rate_limit]`
// ---------------------------------------------------------------------

fn limited_config() -> Config {
    let mut cfg = public_config();
    cfg.rate_limit.enabled = Some(true);
    cfg.rate_limit.trust_proxy_headers = true;
    cfg.rate_limit.requests_per_second = 1;
    cfg.rate_limit.burst = 1;
    cfg
}

/// Burst 1 + rps 1 on one forwarded key: the second call is
/// `429 rate_limited` + `Retry-After`.
#[tokio::test]
async fn rate_limit_429_shape() {
    let (router, _state, _tmp) = app_with_factory(limited_config());
    let resp = router
        .clone()
        .oneshot(xff_req("/api/search?q=x", "203.0.113.7"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = router
        .clone()
        .oneshot(xff_req("/api/search?q=y", "203.0.113.7"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(resp.headers().contains_key(RETRY_AFTER));
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "rate_limited");
}

/// A different forwarded key gets its own bucket; an admin bearer and a
/// non-`/api` path are not counted; `enabled` unset on a public
/// instance defaults to on.
#[tokio::test]
async fn rate_limit_scope_and_exemptions() {
    // Default-enabled on public instances (enabled left unset).
    let mut cfg = public_config();
    cfg.rate_limit.trust_proxy_headers = true;
    cfg.rate_limit.requests_per_second = 1;
    cfg.rate_limit.burst = 1;
    let (router, _state, _tmp) = app_with_factory(cfg);

    // First hit on key A passes; second 429s — the default-on case.
    assert_eq!(
        router
            .clone()
            .oneshot(xff_req("/api/search?q=x", "203.0.113.1"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .clone()
            .oneshot(xff_req("/api/search?q=x", "203.0.113.1"))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );

    // Key B is a separate bucket.
    assert_eq!(
        router
            .clone()
            .oneshot(xff_req("/api/search?q=x", "203.0.113.2"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // The admin token is exempt.
    let admin = Request::builder()
        .method("GET")
        .uri("/api/search?q=x")
        .header(&XFF, "203.0.113.2")
        .header(AUTHORIZATION, "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(admin).await.unwrap().status(),
        StatusCode::OK
    );

    // `trust_proxy_headers` honours the header as the bucket key, but a
    // UI path is out of scope even past its budget.
    let resp = router
        .clone()
        .oneshot(xff_req("/health", "203.0.113.2"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// `trust_proxy_headers = false` ignores spoofable headers entirely —
/// with no `ConnectInfo` (oneshot) the request is unattributable and
/// passes; over a real socket the peer IP would be the key.
#[tokio::test]
async fn rate_limit_untrusted_headers() {
    let mut cfg = limited_config();
    cfg.rate_limit.trust_proxy_headers = false;
    let (router, _state, _tmp) = app_with_factory(cfg);
    for _ in 0..3 {
        assert_eq!(
            router
                .clone()
                .oneshot(xff_req("/api/search?q=x", "203.0.113.9"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
}

// ---------------------------------------------------------------------
// `server.max_inflight`
// ---------------------------------------------------------------------

/// Cap 1 + a latency-injected replay engine: the first request holds the
/// permit and the concurrent second is `429 overloaded` + `Retry-After`.
#[tokio::test]
async fn inflight_saturates() {
    let mut cfg = public_config();
    cfg.server.max_inflight = 1;
    let replay = ReplayOpts {
        latency_ms: 400,
        ..ReplayOpts::default()
    };
    let (state, _tmp) = replay_state(replay, cfg);
    let router = cauce_server::build_router(state);

    let first = tokio::spawn({
        let router = router.clone();
        async move {
            router
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/search?q=x")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let resp = router
        .clone()
        .oneshot(req("GET", "/api/search?q=y"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(resp.headers().contains_key(RETRY_AFTER));
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "overloaded");

    // `/health` slips under the cap while it is still saturated.
    let resp = router.clone().oneshot(req("GET", "/health")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let first = first.await.unwrap().unwrap();
    assert_eq!(first.status(), StatusCode::OK);
}
