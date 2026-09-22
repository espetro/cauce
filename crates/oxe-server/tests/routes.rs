//! Routes-table and HTTP surface tests (W0-09).
//!
//! The wiring matrix of parent plan section 7: `ROUTES` is compared against
//! the section-6 table parsed out of `.agents/plans/2026-09-21-v3-rust-core.md`
//! (filtered to `wave <= 0` for the mounted set) and against the live axum
//! router by probing every declared row.
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use oxe_core::config::Config;
use oxe_core::{Admission, AdmissionLimits, SearchPipeline, StoreTuning};
use oxe_engines::{Replay, ReplayOpts};
use oxe_server::{AppState, ROUTES, RouteKind, build_router, mounted_routes};
use oxe_store_sqlite::SqliteStore;
use serde_json::{Value, json};
use tower::ServiceExt;

/// The wire surface wave 0 must mount (parent plan section 6, wave <= 0).
const EXPECTED_WAVE0: &[(&str, &str)] = &[
    ("GET", "/"),
    ("GET", "/search"),
    ("GET", "/api/search"),
    ("GET", "/api/history"),
    ("POST", "/api/click"),
    ("GET", "/api/stats"),
    ("GET", "/api/cache"),
    ("GET", "/api/cache/{key}"),
    ("DELETE", "/api/cache/{key}"),
    ("DELETE", "/api/cache"),
    ("GET", "/api/audit"),
    ("GET", "/health"),
    ("GET", "/api/config"),
    ("PUT", "/api/config"),
];

/// Serialises tests that mutate process env (`OXE_CONFIG_DIR` and friends).
/// Under nextest each test is its own process anyway; this keeps plain
/// `cargo test` (one process per test binary) safe too.
static ENV_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

/// A tempdir-backed state: SqliteStore + one synthetic `replay` engine.
fn test_state() -> (AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("oxe.db"), StoreTuning::default()).expect("store"),
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

fn req(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

async fn call(
    router: &Router,
    request: Request<Body>,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let resp = router.clone().oneshot(request).await.expect("response");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, body)
}

async fn get(router: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    call(router, req("GET", uri)).await
}

/// `{"error": {code, message, request_id}}` assertion helper.
fn assert_envelope(body: &Value, code: &str) {
    let error = &body["error"];
    assert_eq!(error["code"], code, "envelope code: {body}");
    assert!(error["message"].is_string(), "envelope message: {body}");
    assert!(
        error["request_id"]
            .as_str()
            .and_then(|s| s.parse::<uuid::Uuid>().ok())
            .is_some(),
        "envelope request_id must be a uuid: {body}"
    );
}

// ---------------------------------------------------------------------------
// Plan-table parsing (parent plan section 6)
// ---------------------------------------------------------------------------

struct PlanRoute {
    method: String,
    path: String,
    /// First `W<n>` wave marker in the row's notes; `None` when unmarked.
    wave: Option<u8>,
    /// The row reads as a non-JSON, non-wave-0 surface (SSE, HTMX pages,
    /// streamable MCP, Prometheus text).
    non_wave0_hint: bool,
}

/// Parse the `## 6. Wire surfaces` table out of the parent plan: every
/// backtick-quoted route token becomes `(method, path)` — an explicit
/// `METHOD /path` pair, or a bare `/path` (the HTMX pages row) read as GET,
/// or the `/mcp` streamable-HTTP row read as `*`.
fn plan_routes() -> Vec<PlanRoute> {
    let md = include_str!("../../../.agents/plans/2026-09-21-v3-rust-core.md");
    let section = md
        .split("## 6. Wire surfaces")
        .nth(1)
        .expect("section 6")
        .split("### 6.1")
        .next()
        .expect("section 6 body");

    let mut out = Vec::new();
    for line in section.lines() {
        let line = line.trim();
        if !line.starts_with('|') || line.contains("---") {
            continue;
        }
        // Backtick-quoted tokens across the whole row: the `expired|all`
        // token contains a `|`, so cells cannot be split first.
        let tokens: Vec<&str> = line.split('`').skip(1).step_by(2).collect();
        let wave = wave_marker(line);
        let non_wave0_hint = [
            "SSE",
            "HTMX",
            "requires:",
            "streamable",
            "rmcp",
            "Prometheus",
        ]
        .iter()
        .any(|hint| line.contains(hint));
        for token in tokens {
            let token = token.trim();
            let (method, path) = match token.split_once(' ') {
                Some((m, p)) if is_http_method(m) => (m.to_string(), p),
                _ if token.starts_with('/') => {
                    let method = if line.contains("streamable HTTP") {
                        "*"
                    } else {
                        "GET"
                    };
                    (method.to_string(), token)
                }
                _ => continue,
            };
            let path = path.split('?').next().unwrap().trim().to_string();
            if path.is_empty() {
                continue;
            }
            out.push(PlanRoute {
                method,
                path,
                wave,
                non_wave0_hint,
            });
        }
    }
    out
}

fn is_http_method(s: &str) -> bool {
    matches!(s, "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD")
}

/// Smallest `W<n>` marker in the row text (`"W0, extended W1-09"` -> 0).
fn wave_marker(line: &str) -> Option<u8> {
    let bytes = line.as_bytes();
    let mut min: Option<u8> = None;
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'W' && bytes[i + 1].is_ascii_digit() {
            let mut n = 0u32;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                n = n * 10 + u32::from(bytes[j] - b'0');
                j += 1;
            }
            // Guard against matching inside a longer word (none exist in
            // the table, but keep the parse honest).
            if i == 0 || !bytes[i - 1].is_ascii_alphanumeric() {
                min = Some(min.map_or(n as u8, |m| m.min(n as u8)));
            }
        }
    }
    min
}

fn declared_set() -> BTreeSet<(String, String)> {
    ROUTES
        .iter()
        .map(|r| (r.method.to_string(), r.path.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// The wiring-matrix tests
// ---------------------------------------------------------------------------

/// `ROUTES` declares exactly the routes in parent plan section 6 — every
/// wave, so a route added to the plan but never declared (or vice versa)
/// fails here.
#[test]
fn routes_table_matches_plan_section_6() {
    let plan: BTreeSet<(String, String)> = plan_routes()
        .iter()
        .map(|r| (r.method.clone(), r.path.clone()))
        .collect();
    let declared = declared_set();
    assert_eq!(
        declared,
        plan,
        "ROUTES and plan section 6 disagree:\nonly in ROUTES: {:?}\nonly in plan: {:?}",
        declared.difference(&plan).collect::<Vec<_>>(),
        plan.difference(&declared).collect::<Vec<_>>(),
    );
}

/// The plan's `wave <= 0` rows equal `ROUTES`' mounted set.
///
/// Wave derivation for plan rows: an explicit `W<n>` note wins; an unmarked
/// row is wave 0 only when nothing in the row marks it as a later kind
/// (SSE/HTMX/streamable-MCP/Prometheus all land in W1+). That yields
/// exactly the twelve wave-0 JSON routes.
#[test]
fn wave0_routes_match_plan_filter() {
    let plan_wave0: BTreeSet<(String, String)> = plan_routes()
        .iter()
        .filter(|r| r.wave == Some(0) || (r.wave.is_none() && !r.non_wave0_hint))
        .map(|r| (r.method.clone(), r.path.clone()))
        .collect();
    let declared_wave0: BTreeSet<(String, String)> = ROUTES
        .iter()
        .filter(|r| r.wave == 0 && r.kind == RouteKind::Json && r.requires.is_none())
        .map(|r| (r.method.to_string(), r.path.to_string()))
        .collect();
    assert_eq!(declared_wave0, plan_wave0);
    let expected_wave0_json: BTreeSet<(String, String)> = EXPECTED_WAVE0
        .iter()
        .filter(|(_, p)| *p == "/health" || p.starts_with("/api/"))
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .collect();
    assert_eq!(declared_wave0, expected_wave0_json);
}

/// `mounted_routes` (the builder's own view) equals the wave-0 set plus the
/// wave-1 rows that have handlers (currently `* /mcp`, W1-08).
#[test]
fn mounted_routes_match_declaration() {
    let (state, _tmp) = test_state();
    let mounted: BTreeSet<(String, String)> = mounted_routes(&state, &Default::default())
        .map(|r| (r.method.to_string(), r.path.to_string()))
        .collect();
    let expected: BTreeSet<(String, String)> = EXPECTED_WAVE0
        .iter()
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .chain([("*".to_string(), "/mcp".to_string())])
        .collect();
    assert_eq!(mounted, expected);
}

/// Probe the live router: every mounted row answers (never 404/405), every
/// declared-but-unmounted row is absent, and undeclared paths 404 — the
/// mechanical fix for "written but never mounted".
#[tokio::test]
async fn live_router_matches_routes_table() {
    let (router, state, _tmp) = app();
    let mounted: BTreeSet<(String, String)> = mounted_routes(&state, &Default::default())
        .map(|r| (r.method.to_string(), r.path.to_string()))
        .collect();

    // Seed one cache entry so `{key}` probes hit a real row.
    get(&router, "/api/search?q=probe").await;
    let (_, _, body) = get(&router, "/api/cache").await;
    let key = body[0]["key"]
        .as_str()
        .expect("seeded cache entry")
        .to_string();

    for spec in ROUTES {
        // Thin inputs are fine: a 400 still proves the route exists; a
        // 404/405 means it does not.
        let path = spec
            .path
            .replace("{key}", &key)
            .replace("{id}", "x")
            .replace("{url}", "https%3A%2F%2Fexample.com");
        // `*` is not an HTTP method; probe the MCP endpoint with POST (a
        // bare POST without the MCP accept/content headers answers 4xx,
        // which still proves the route is mounted).
        let method = if spec.method == "*" {
            Method::POST
        } else {
            Method::from_bytes(spec.method.as_bytes()).unwrap_or(Method::GET)
        };
        let uri = match (spec.method, spec.path) {
            ("GET", "/api/search") => format!("{path}?q=probe"),
            ("DELETE", "/api/cache") => format!("{path}?all=true"),
            _ => path.clone(),
        };
        // `PUT /api/config` gets deliberately malformed TOML: the 400 proves
        // the route without touching the real config file.
        let body = match (spec.method, spec.path) {
            ("POST", "/api/click") => Body::from(r#"{"url":"https://example.com"}"#),
            ("PUT", "/api/config") => Body::from("[unclosed"),
            _ => Body::empty(),
        };
        let request = Request::builder()
            .method(method)
            .uri(&uri)
            .body(body)
            .unwrap();
        let resp = router.clone().oneshot(request).await.unwrap();
        let status = resp.status();
        let slot = (spec.method.to_string(), spec.path.to_string());
        if mounted.contains(&slot) {
            assert!(
                status != StatusCode::NOT_FOUND && status != StatusCode::METHOD_NOT_ALLOWED,
                "declared+mounted route {} {} answered {status}",
                spec.method,
                spec.path,
            );
        } else {
            assert!(
                status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED,
                "unmounted route {} {} answered {status}",
                spec.method,
                spec.path,
            );
        }
    }

    // Undeclared paths 404, and there is no Exa-shaped HTTP route (locked
    // decision: the only Exa surface is the MCP tool alias).
    for uri in ["/nope", "/exa", "/exa/search", "/api/exa_search"] {
        let (status, _, body) = get(&router, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_envelope(&body, "not_found");
    }
}

// ---------------------------------------------------------------------------
// Handler behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_network_then_cache_hit() {
    let (router, _state, _tmp) = app();

    let (status, headers, body) = get(&router, "/api/search?q=tanstack%20router").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["meta"]["source"], json!("network"));
    let request_id = body["meta"]["request_id"].as_str().unwrap();
    assert_eq!(
        headers["x-request-id"].to_str().unwrap(),
        request_id,
        "X-Request-Id must equal meta.request_id"
    );
    assert!(body["results"].as_array().unwrap().len() >= 5);

    let (status, headers, body) = get(&router, "/api/search?q=tanstack%20router").await;
    assert_eq!(status, StatusCode::OK);
    let source = &body["meta"]["source"];
    assert_eq!(source["cache"]["tier"], 1, "{body}");
    assert!(source["cache"]["ttl_s"].as_u64().unwrap() > 0);
    assert_eq!(source["cache"]["stale"], false);
    // A second request gets its own id.
    assert_ne!(headers["x-request-id"].to_str().unwrap(), request_id);
}

/// An inbound `X-Request-Id` that parses as a UUID is honoured end to end.
#[tokio::test]
async fn inbound_request_id_is_honoured() {
    let (router, _state, _tmp) = app();
    let id = uuid::Uuid::now_v7();
    let request = Request::builder()
        .method("GET")
        .uri("/api/search?q=request-id")
        .header("x-request-id", id.to_string())
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = call(&router, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["x-request-id"].to_str().unwrap(), id.to_string());
    assert_eq!(body["meta"]["request_id"], id.to_string());
}

#[tokio::test]
async fn history_click_and_stats() {
    let (router, _state, _tmp) = app();

    // One network search + one cache hit = two history rows, hit rate 0.5.
    get(&router, "/api/search?q=history-check").await;
    get(&router, "/api/search?q=history-check").await;

    let (status, _, body) = get(&router, "/api/history").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{body}");
    assert_eq!(rows[0]["kind"], "search");

    // History merges searches and clicks by (ts DESC, id DESC) at
    // millisecond precision; without a pause the click can share the last
    // search's ms and lose the id tiebreak, flipping rows[0].
    tokio::time::sleep(Duration::from_millis(2)).await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/click")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"url":"https://example.com/a","title":"A","position":0}"#,
        ))
        .unwrap();
    let (status, _, _) = call(&router, request).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, body) = get(&router, "/api/history").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["kind"], "click", "{body}");

    let (status, _, body) = get(&router, "/api/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["searches"], 2);
    assert_eq!(body["cache_hits"], 1);
    assert_eq!(body["hit_rate"], 0.5);
}

/// Acceptance: `DELETE /api/cache/{key}` then the same search is `network`
/// again, and the delete lands in `audit` with actor `api`.
#[tokio::test]
async fn cache_admin_delete_and_audit() {
    let (router, _state, _tmp) = app();

    let (_, _, body) = get(&router, "/api/search?q=cache-me").await;
    assert_eq!(body["meta"]["source"], json!("network"));

    // The entry is listed and addressable by its hex key.
    let (status, _, body) = get(&router, "/api/cache").await;
    assert_eq!(status, StatusCode::OK);
    let entries = body.as_array().unwrap();
    assert_eq!(entries.len(), 1, "{body}");
    let key = entries[0]["key"].as_str().unwrap().to_string();
    assert_eq!(key.len(), 64);

    let (status, _, body) = get(&router, &format!("/api/cache/{key}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["key"], key);

    // Delete it: audited, and the same query hits the network again.
    let request = req("DELETE", &format!("/api/cache/{key}"));
    let (status, _, body) = call(&router, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);

    let (status, _, _) = get(&router, &format!("/api/cache/{key}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, _, body) = get(&router, "/api/search?q=cache-me").await;
    assert_eq!(body["meta"]["source"], json!("network"), "{body}");

    let (status, _, body) = get(&router, "/api/audit").await;
    assert_eq!(status, StatusCode::OK);
    let rows = body.as_array().unwrap();
    assert!(
        rows.iter()
            .any(|r| r["action"] == "cache.delete" && r["actor"] == "api" && r["target"] == key),
        "audit rows: {body}"
    );
}

/// `DELETE /api/cache` bulk semantics: exactly one of `expired` / `all`.
#[tokio::test]
async fn cache_bulk_delete_flags() {
    let (router, _state, _tmp) = app();
    get(&router, "/api/search?q=bulk-a").await;
    get(&router, "/api/search?q=bulk-b").await;

    // Neither flag -> 400; both -> 400.
    for uri in ["/api/cache", "/api/cache?expired=true&all=true"] {
        let (status, _, body) = call(&router, req("DELETE", uri)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_envelope(&body, "bad_request");
    }

    // expired=true evicts only expired rows (none here) but still audits.
    let (status, _, body) = call(&router, req("DELETE", "/api/cache?expired=true")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], 0);

    let (status, _, body) = call(&router, req("DELETE", "/api/cache?all=true")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], 2);

    let (_, _, body) = get(&router, "/api/audit").await;
    let rows = body.as_array().unwrap();
    for action in ["cache.evict_expired", "cache.clear"] {
        assert!(
            rows.iter()
                .any(|r| r["action"] == action && r["actor"] == "api"),
            "missing audit row for {action}: {body}"
        );
    }
}

#[tokio::test]
async fn error_envelope_and_param_validation() {
    let (router, _state, _tmp) = app();

    // Missing q, unknown param, bad values -> 400 envelope.
    for uri in [
        "/api/search",
        "/api/search?q=x&bogus=1",
        "/api/search?q=x&safesearch=9",
        "/api/search?q=x&page=0",
        "/api/search?q=x&page=abc",
        "/api/search?q=x&q=y",
        "/api/history?since=not-a-date",
        "/api/cache/not-hex-at-all",
    ] {
        let (status, _, body) = get(&router, uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_envelope(&body, "bad_request");
    }

    // Malformed and absent cache keys.
    let (status, _, body) = get(&router, &format!("/api/cache/{}", "f".repeat(64))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_envelope(&body, "not_found");

    // Wrong method on a mounted path -> 405 envelope.
    let (status, _, body) = call(&router, req("DELETE", "/api/search")).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_envelope(&body, "method_not_allowed");

    // Bad click JSON -> 400 envelope.
    let request = Request::builder()
        .method("POST")
        .uri("/api/click")
        .body(Body::from("{nope"))
        .unwrap();
    let (status, _, body) = call(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "bad_request");
}

/// `NoEngines` mapping: a non-empty pin matching nothing is 400
/// `unknown_engines` even with zero configured engines; an empty/zero
/// configured set is 503 `no_engines`.
#[tokio::test]
async fn no_engines_status_mapping() {
    let (router, _state, _tmp) = app();
    let (status, _, body) = get(&router, "/api/search?q=x&engines=nosuch").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "unknown_engines");

    // A valid pin on a configured engine runs (and keys the cache entry
    // separately from the unpinned search).
    let (status, _, body) = get(&router, "/api/search?q=pinned&engines=replay").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let tmp = tempfile::tempdir().unwrap();
    let store =
        Arc::new(SqliteStore::open(tmp.path().join("oxe.db"), StoreTuning::default()).unwrap());
    let pipeline = Arc::new(SearchPipeline::new(store.clone(), vec![]));
    let router = build_router(AppState::new(pipeline, store, Config::default()));

    // Non-empty pin with zero configured engines is still the caller's error.
    let (status, _, body) = get(&router, "/api/search?q=x&engines=replay").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "unknown_engines");

    // No pin with zero configured engines is the operator's error.
    let (status, _, body) = get(&router, "/api/search?q=x").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_envelope(&body, "no_engines");
}

#[tokio::test]
async fn health_reports_ok() {
    let (router, _state, _tmp) = app();
    let (status, _, body) = get(&router, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

/// `GET /api/config` shows `${env:...}` templates literally and never a
/// resolved secret; `PUT /api/config` round-trips a valid change, rejects
/// an invalid one with a 400, and audits the write.
#[tokio::test]
async fn config_get_redaction_and_put_roundtrip() {
    let _guard = env_lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("cfg");
    std::fs::create_dir_all(&config_dir).unwrap();
    // SAFETY: serialized by ENV_LOCK; nextest also isolates per process.
    unsafe {
        std::env::set_var("OXE_CONFIG_DIR", &config_dir);
        std::env::set_var("OXE_DATA_DIR", tmp.path().join("data"));
        std::env::set_var("OXE_ROUTE_TEST_SECRET", "s3cret-value");
        // The env overlay would otherwise beat the PUT body's values.
        std::env::remove_var("OXE_AI_API_KEY");
        std::env::remove_var("OXE_SEARCH_DEADLINE_MS");
    }
    std::fs::write(
        config_dir.join("config.toml"),
        "[ai]\napi_key = \"${env:OXE_ROUTE_TEST_SECRET}\"\n",
    )
    .unwrap();

    let (state, _tmp2) = test_state();
    // Point the state's config at the sandbox (Config::load reads env).
    state.with_config(|cfg| *cfg = Config::load().unwrap());
    assert_eq!(state.with_config(|c| c.ai.api_key.clone()), "s3cret-value");
    let router = build_router(state);

    let (status, _, body) = get(&router, "/api/config").await;
    assert_eq!(status, StatusCode::OK);
    let text = body.to_string();
    assert!(
        text.contains("${env:OXE_ROUTE_TEST_SECRET}"),
        "template must be shown literally: {text}"
    );
    assert!(
        !text.contains("s3cret-value"),
        "resolved secret must never appear: {text}"
    );

    // PUT a valid tree: the value lands, templates elsewhere stay literal,
    // the write is audited.
    let request = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .header("content-type", "application/toml")
        .body(Body::from("[search]\ndeadline_ms = 1234\n"))
        .unwrap();
    let (status, _, body) = call(&router, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["search"]["deadline_ms"], 1234);
    // The PUT body is the whole new config; `ai.api_key` falls back to its
    // blank default now that the file's `${env:...}` template is gone.
    assert_eq!(body["ai"]["api_key"], "");
    assert_eq!(
        Config::load().unwrap().search.deadline_ms,
        1234,
        "the file must carry the new tree"
    );

    let (_, _, body) = get(&router, "/api/audit").await;
    assert!(
        body.as_array()
            .unwrap()
            .iter()
            .any(|r| r["action"] == "config.put" && r["actor"] == "api"),
        "audit rows: {body}"
    );

    // PUT an invalid tree (unknown key): 400 and the file is restored.
    let request = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .body(Body::from("bogus_key = 1\n"))
        .unwrap();
    let (status, _, body) = call(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "invalid_config");
    assert_eq!(Config::load().unwrap().search.deadline_ms, 1234);

    // PUT a tree whose template cannot resolve: 400, file restored.
    let request = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .body(Body::from(
            "[ai]\napi_key = \"${env:OXE_UNSET_FOR_TEST}\"\n",
        ))
        .unwrap();
    let (status, _, body) = call(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_envelope(&body, "invalid_config");
    assert_eq!(Config::load().unwrap().search.deadline_ms, 1234);

    // Malformed TOML: 400, no file touched.
    let request = Request::builder()
        .method("PUT")
        .uri("/api/config")
        .body(Body::from("[unclosed"))
        .unwrap();
    let (status, _, body) = call(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_envelope(&body, "invalid_config");
}

/// W1-07 acceptance: `PipelineError::RateLimited` maps to 429 with a
/// `Retry-After` header and the `rate_limited` envelope code.
#[tokio::test]
async fn search_queue_overflow_returns_429_retry_after() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(
        SqliteStore::open(tmp.path().join("oxe.db"), StoreTuning::default()).expect("store"),
    );
    let engine = Arc::new(Replay::new(ReplayOpts {
        latency_ms: 300,
        ..ReplayOpts::default()
    }));
    let pipeline = Arc::new(
        SearchPipeline::new(store.clone(), vec![engine.clone()]).with_admission(Admission::new(
            AdmissionLimits {
                max_wait: Duration::from_millis(1),
                max_concurrent_per_engine: 1,
            },
        )),
    );
    let router = build_router(AppState::new(pipeline, store, Config::default()));

    // Occupy the single engine slot with an in-flight request.
    let holder = tokio::spawn({
        let router = router.clone();
        async move { router.oneshot(req("GET", "/api/search?q=holder")).await }
    });
    // `call_count` ticks at the top of `search`: 1 means the permit is held.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while engine.call_count() == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(engine.call_count(), 1, "holder never reached the engine");

    let (status, headers, body) = get(&router, "/api/search?q=overflow").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(headers[header::RETRY_AFTER], "1");
    assert_envelope(&body, "rate_limited");

    let holder_resp = holder.await.unwrap().expect("holder response");
    assert_eq!(holder_resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// W1-13: Host/Origin loopback guard
// ---------------------------------------------------------------------------

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
        let (status, _, body) = call(&router, request).await;
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
    let (status, _, _) = call(&router, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    for host in [
        "127.0.0.1",
        "127.0.0.1:4479",
        "localhost",
        "localhost:4479",
        "[::1]",
        "[::1]:4479",
        "search.localhost",
        "oxe.localhost:443",
    ] {
        let request = Request::builder()
            .method("GET")
            .uri("/health")
            .header("host", host)
            .body(Body::empty())
            .unwrap();
        let (status, _, body) = call(&router, request).await;
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
        let (status, _, body) = call(&router, request).await;
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
    let (status, _, _) = call(
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
        let (status, _, body) = call(
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
        let (status, _, body) = call(
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
    let router = oxe_server::build_router_opts(
        state,
        oxe_server::RouterOptions {
            bind_host: "oxe.lvh.me".to_string(),
            ..Default::default()
        },
    );
    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .header("host", "oxe.lvh.me:4479")
        .body(Body::empty())
        .unwrap();
    let (status, _, body) = call(&router, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
