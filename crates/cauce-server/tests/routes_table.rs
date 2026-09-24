//! Routes-table and plan-conformance tests (W0-09).
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

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cauce_server::{
    ROUTES, RouteKind, RouterOptions, build_router_opts, feature_enabled, mounted_routes,
};
use tower::ServiceExt;

mod support;
use support::*;

/// The wave-0 JSON surface (parent plan section 6): mounts in every build
/// and every mode, including `cauce serve --headless`.
const EXPECTED_WAVE0_JSON: &[(&str, &str)] = &[
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

/// The wave-0 HTMX pages: `requires: "ui"` rows, mounted only when the
/// `ui` cargo feature is compiled in and `--headless` is not passed.
const EXPECTED_WAVE0_UI: &[(&str, &str)] = &[("GET", "/"), ("GET", "/search")];

/// Wave-2 HTMX pages mounted so far.
const EXPECTED_WAVE2_UI: &[(&str, &str)] = &[("GET", "/settings"), ("GET", "/history")];

/// Wave-1 rows mounted so far (W1-06 engine health, W1-09 metrics).
/// `/mcp` (W1-08) is added under `cfg!(feature = "mcp")` because its
/// method is `*` and the row compiles out without the feature.
const EXPECTED_WAVE1_MOUNTED: &[(&str, &str)] = &[
    ("GET", "/api/engines"),
    ("POST", "/api/engines/{id}/reset"),
    ("GET", "/metrics"),
];

/// Wave-2 API rows mounted so far: the SSE stream endpoint (W2-01),
/// W2-02's audited history-row delete, W2-05's enable/disable posts and
/// the OpenSearch suggestions endpoint the W2-11 descriptor advertises
/// (#150). They mount in every build including headless.
const EXPECTED_WAVE2_MOUNTED: &[(&str, &str)] = &[
    ("DELETE", "/api/history/{id}"),
    ("GET", "/api/search/stream"),
    ("GET", "/api/suggest"),
    ("POST", "/api/engines/{id}/enable"),
    ("POST", "/api/engines/{id}/disable"),
];

/// Wave-2 `requires: "ui"` rows mounted so far: the cache page (W2-04),
/// `/opensearch.xml` (W2-11), the audit + trace pages (W2-06), the favicon
/// (#87), the dashboard (W2-03) and the engines page (W2-05). They join the
/// mounted set only in `ui` builds and drop under `--headless` like the
/// pages.
const EXPECTED_WAVE2_UI_MOUNTED: &[(&str, &str)] = &[
    ("GET", "/cache"),
    ("GET", "/opensearch.xml"),
    ("GET", "/favicon.ico"),
    ("GET", "/dashboard"),
    ("GET", "/audit"),
    ("GET", "/trace/{id}"),
    ("GET", "/engines"),
];

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
    let expected_wave0_json: BTreeSet<(String, String)> = EXPECTED_WAVE0_JSON
        .iter()
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .collect();
    assert_eq!(declared_wave0, expected_wave0_json);
}

/// `mounted_routes` (the builder's own view) equals the wave-0 set plus
/// every wave-1 row implemented so far (`* /mcp` from W1-08, the engine
/// health pair from W1-06, `/metrics` from W1-09), W2-02's history
/// routes, W2-05's enable/disable route plus `/engines` page, and the
/// wave-2 favicon (#87), filtered to the compiled cargo features.
#[test]
fn mounted_routes_match_declaration() {
    let (state, _tmp) = test_state();
    let mounted: BTreeSet<(String, String)> = mounted_routes(&state, &Default::default())
        .map(|r| (r.method.to_string(), r.path.to_string()))
        .collect();
    let mut expected: BTreeSet<(String, String)> = EXPECTED_WAVE0_JSON
        .iter()
        .chain(EXPECTED_WAVE1_MOUNTED)
        .chain(EXPECTED_WAVE2_MOUNTED)
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .collect();
    if cfg!(feature = "ui") {
        expected.extend(
            EXPECTED_WAVE0_UI
                .iter()
                .chain(EXPECTED_WAVE2_UI)
                .chain(EXPECTED_WAVE2_UI_MOUNTED)
                .map(|(m, p)| (m.to_string(), p.to_string())),
        );
    }
    if cfg!(feature = "mcp") {
        expected.insert(("*".to_string(), "/mcp".to_string()));
    }
    assert_eq!(mounted, expected);
}

/// W1-12: `requires` filters the table by compiled cargo feature — a
/// mounted row's feature is always compiled in (a compiled-out row has no
/// handler arm either, so it can never leak into the router).
#[test]
fn mounted_routes_respect_compiled_features() {
    let (state, _tmp) = test_state();
    for opts in [RouterOptions::default(), RouterOptions::headless()] {
        for spec in mounted_routes(&state, &opts) {
            assert!(
                spec.requires.is_none_or(feature_enabled),
                "{} {} mounted but its feature {:?} is compiled out",
                spec.method,
                spec.path,
                spec.requires,
            );
        }
    }
}

/// W1-12 acceptance: `cauce serve --headless` drops the `requires: "ui"` rows
/// (404 on the pages) while the JSON surface stays up.
#[cfg(feature = "ui")]
#[tokio::test]
async fn headless_drops_ui_routes_keeps_api() {
    let (state, _tmp) = test_state();
    let headless: BTreeSet<(String, String)> = mounted_routes(&state, &RouterOptions::headless())
        .map(|r| (r.method.to_string(), r.path.to_string()))
        .collect();
    for ui_row in EXPECTED_WAVE0_UI
        .iter()
        .chain(EXPECTED_WAVE2_UI)
        .chain(EXPECTED_WAVE2_UI_MOUNTED)
    {
        assert!(
            !headless.contains(&(ui_row.0.to_string(), ui_row.1.to_string())),
            "{ui_row:?} must not mount under --headless"
        );
    }
    assert!(headless.contains(&("GET".to_string(), "/api/search".to_string())));

    let router = build_router_opts(state, RouterOptions::headless());
    for uri in [
        "/",
        "/search?q=x",
        "/cache",
        "/opensearch.xml",
        "/favicon.ico",
        "/settings",
        "/history",
        "/dashboard",
        "/audit",
    ] {
        let (status, _, body) = get(&router, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
        assert_envelope(&body, "not_found");
    }
    let (status, _, body) = get(&router, "/api/search?q=headless").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _, _) = get(&router, "/health").await;
    assert_eq!(status, StatusCode::OK);
}

/// Probe the live router: every mounted row answers (never 404/405), every
/// declared-but-unmounted row is absent, and undeclared paths 404 — the
/// mechanical fix for "written but never mounted".
///
/// The `POST /api/engines/{id}/enable|disable` probes run the real
/// config-write path: `Config::save()` resolves `config.toml` through
/// `CAUCE_CONFIG_DIR`, so the env is pinned to a tempdir for the whole
/// test (under `env_lock`, same discipline as `tests/engines.rs`) or the
/// probe would overwrite the developer's real config.
#[tokio::test]
async fn live_router_matches_routes_table() {
    let _guard = env_lock().await;
    let cfg_dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: serialized by env_lock; nextest also isolates per process.
    unsafe {
        std::env::set_var("CAUCE_CONFIG_DIR", cfg_dir.path());
    }
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
        // `{id}` probes a real engine id: `POST /api/engines/{id}/reset`
        // 404s on unknown ids, which would read as "not mounted".
        let path = spec
            .path
            .replace("{key}", &key)
            .replace("{id}", "replay")
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

    unsafe {
        std::env::remove_var("CAUCE_CONFIG_DIR");
    }
}

/// `GET /favicon.ico` serves the embedded SVG icon — the wave-0 browser
/// pass saw it 404 on every page load (#87). `ui` builds only.
#[cfg(feature = "ui")]
#[tokio::test]
async fn favicon_is_served() {
    let (router, _state, _tmp) = app();
    let (status, headers, body) = get_headers(&router, "/favicon.ico").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "image/svg+xml");
    assert!(
        body.starts_with("<svg"),
        "favicon body should be the embedded SVG: {body:?}"
    );
}
