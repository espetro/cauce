//! MCP-over-HTTP integration tests (W1-08 acceptance).
//!
//! An rmcp streamable-HTTP client talks to a real `cauce serve` router (same
//! `AppState`, tempdir SQLite, `replay` engine): `tools/list` must expose
//! exactly the settled tools, `search_web`/`cache_status`/
//! `cache_invalidate` must carry `request_id`, the MCP client name must land
//! in `ClientKind::Mcp(name)` on the `search_log` row, `exa_search` must
//! validate against the frozen v2 wire shape (`fixtures/exa_schema.json`),
//! and `fetch_and_index` must yield a `pages` row carrying the extracted
//! markdown (W5-01, `archive` builds).
//!
//! This Source Code Form is subject to the terms of the Mozilla Public
//! License, v. 2.0. If a copy of the MPL was not distributed with this
//! file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

// `/mcp` and `cauce_server::mcp` exist only in `mcp` builds (W1-12).
#![cfg(feature = "mcp")]

use std::collections::BTreeSet;
use std::net::SocketAddr;

use cauce_core::{AuditFilter, ClientKind, HistoryFilter, HistoryItem};
use cauce_server::{AppState, build_router};
use rmcp::model::{
    CallToolRequestParams, ClientCapabilities, Implementation, InitializeRequestParams,
};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
#[cfg(feature = "archive")]
use wiremock::matchers::{method, path};
#[cfg(feature = "archive")]
use wiremock::{Mock, MockServer, ResponseTemplate};

mod support;
use support::*;

/// The settled tool surface: the four W1-08 tools plus W5-01's
/// `fetch_and_index` in `archive` builds.
const TOOL_NAMES: &[&str] = &[
    "search_web",
    "cache_status",
    "cache_invalidate",
    "exa_search",
    #[cfg(feature = "archive")]
    "fetch_and_index",
];

const CLIENT_NAME: &str = "cauce-mcp-http-test";

/// Serve the full router (including `/mcp`) on a loopback port.
async fn spawn_server(state: AppState) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, build_router(state))
            .await
            .expect("serve");
    });
    (addr, handle)
}

/// An rmcp client over streamable HTTP, initialized as `CLIENT_NAME`.
async fn mcp_client(addr: SocketAddr) -> RunningService<RoleClient, InitializeRequestParams> {
    let transport = StreamableHttpClientTransport::with_client(
        reqwest::Client::new(),
        StreamableHttpClientTransportConfig::with_uri(format!("http://{addr}/mcp")),
    );
    InitializeRequestParams::new(
        ClientCapabilities::default(),
        Implementation::new(CLIENT_NAME, "0.0.0"),
    )
    .serve(transport)
    .await
    .expect("mcp initialize")
}

fn args(value: Value) -> rmcp::model::JsonObject {
    value.as_object().expect("object args").clone()
}

fn structured(result: &rmcp::model::CallToolResult) -> &Value {
    assert_ne!(result.is_error, Some(true), "tool error: {result:?}");
    result
        .structured_content
        .as_ref()
        .expect("structured_content")
}

fn request_id_of(value: &Value, key: &str) -> uuid::Uuid {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing {key}: {value}"))
        .parse::<uuid::Uuid>()
        .unwrap_or_else(|_| panic!("{key} is not a UUID: {value}"))
}

/// Acceptance 1 + 2: `tools/list` returns exactly the four settled tools and
/// `search_web` over the `replay` engine returns results with a request id.
/// Also asserts the client name lands in `ClientKind::Mcp(name)` on the
/// `search_log` row and the server instructions point at `replay`/`wikipedia`.
#[tokio::test]
async fn mcp_http_tools_and_search() {
    let (state, _tmp) = test_state();
    let (addr, server) = spawn_server(state.clone()).await;
    let client = mcp_client(addr).await;

    // Server instructions mention the iteration engines.
    let info = client.peer_info().expect("initialize result");
    let instructions = info.instructions.as_deref().unwrap_or_default();
    assert!(
        instructions.contains("replay"),
        "instructions: {instructions}"
    );
    assert!(
        instructions.contains("wikipedia"),
        "instructions: {instructions}"
    );

    // Exactly the settled tools.
    let tools = client.list_all_tools().await.expect("tools/list");
    let names: BTreeSet<String> = tools.iter().map(|t| t.name.to_string()).collect();
    let expected: BTreeSet<String> = TOOL_NAMES.iter().map(|s| s.to_string()).collect();
    assert_eq!(names, expected, "tool surface must be exactly {expected:?}");

    // search_web against the replay engine.
    let result = client
        .call_tool(
            CallToolRequestParams::new("search_web")
                .with_arguments(args(json!({"query": "mcp probe", "engines": ["replay"]}))),
        )
        .await
        .expect("call search_web");
    let body = structured(&result);
    let request_id = body["meta"]["request_id"]
        .as_str()
        .expect("meta.request_id")
        .parse::<uuid::Uuid>()
        .expect("meta.request_id is a UUID");
    assert!(
        !body["results"].as_array().expect("results").is_empty(),
        "replay must return results: {body}"
    );
    assert_eq!(body["meta"]["source"], json!("network"), "{body}");
    assert_eq!(
        body["meta"]["engines_used"][0]["engine"],
        json!("replay"),
        "{body}"
    );

    // The MCP client name flows into ClientKind::Mcp(name) on search_log.
    let history = state
        .store()
        .list_history(&HistoryFilter {
            since: None,
            q: None,
            cached: false,
            limit: 10,
        })
        .await
        .expect("history");
    let search_row = history
        .iter()
        .find_map(|item| match item {
            HistoryItem::Search(row) => Some(row),
            HistoryItem::Click(_) => None,
        })
        .expect("one search_log row");
    assert_eq!(
        search_row.client,
        ClientKind::Mcp(CLIENT_NAME.to_string()),
        "search_log client"
    );
    // Same request id the tool reported.
    let audit = state
        .store()
        .list_audit(&AuditFilter {
            action: Some("mcp.search_web".to_string()),
            ..Default::default()
        })
        .await
        .expect("audit");
    assert!(
        audit
            .iter()
            .any(|r| r.request_id == Some(request_id) && r.actor == format!("mcp:{CLIENT_NAME}")),
        "mcp.search_web audit row: {audit:?}"
    );

    client.cancel().await.expect("cancel");
    server.abort();
}

/// Issue #90 strict contract on the MCP surface: a pin naming any unknown
/// id — even alongside a valid one — fails `search_web` with
/// `invalid_params` naming the rejected ids and the configured set.
#[tokio::test]
async fn mcp_http_search_web_rejects_unknown_engine_ids() {
    let (state, _tmp) = test_state();
    let (addr, server) = spawn_server(state).await;
    let client = mcp_client(addr).await;

    for engines in [json!(["nope"]), json!(["replay", "nope"])] {
        let err = client
            .call_tool(
                CallToolRequestParams::new("search_web")
                    .with_arguments(args(json!({"query": "pin probe", "engines": engines}))),
            )
            .await
            .expect_err("unknown pin must be a tool error");
        let text = err.to_string();
        assert!(text.contains("nope"), "error names the rejected id: {text}");
        assert!(
            text.contains("replay"),
            "error lists the configured set: {text}"
        );
    }

    client.cancel().await.expect("cancel");
    server.abort();
}

/// `cache_status` and `cache_invalidate` carry `request_id`; the destructive
/// call is audited as `mcp.cache_invalidate`.
#[tokio::test]
async fn mcp_http_cache_tools() {
    let (state, _tmp) = test_state();
    let (addr, server) = spawn_server(state.clone()).await;
    let client = mcp_client(addr).await;

    // Seed one cache entry.
    client
        .call_tool(
            CallToolRequestParams::new("search_web")
                .with_arguments(args(json!({"query": "cacheable"}))),
        )
        .await
        .expect("seed search");

    let result = client
        .call_tool(CallToolRequestParams::new("cache_status"))
        .await
        .expect("call cache_status");
    let body = structured(&result);
    request_id_of(body, "request_id");
    assert_eq!(body["searches"], json!(1), "{body}");
    assert_eq!(body["cache_entries"], json!(1), "{body}");

    // No selector is an argument error, not a transport failure.
    let err = client
        .call_tool(CallToolRequestParams::new("cache_invalidate"))
        .await
        .expect_err("selector required");
    assert!(err.to_string().contains("exactly one of"), "error: {err}");

    let result = client
        .call_tool(
            CallToolRequestParams::new("cache_invalidate")
                .with_arguments(args(json!({"all": true}))),
        )
        .await
        .expect("call cache_invalidate");
    let body = structured(&result);
    request_id_of(body, "request_id");
    assert_eq!(body["removed"], json!(1), "{body}");

    let audit = state
        .store()
        .list_audit(&AuditFilter {
            action: Some("mcp.cache_invalidate".to_string()),
            ..Default::default()
        })
        .await
        .expect("audit");
    assert!(
        audit
            .iter()
            .any(|r| r.actor == format!("mcp:{CLIENT_NAME}")),
        "mcp.cache_invalidate audit row: {audit:?}"
    );

    client.cancel().await.expect("cancel");
    server.abort();
}

/// Acceptance 4: `exa_search` output validates against the frozen v2 schema
/// (`tests/fixtures/exa_schema.json`), and `source = "history"` short-circuits
/// to the `clicks` table.
#[tokio::test]
async fn mcp_http_exa_search_frozen_shape() {
    let (state, _tmp) = test_state();
    let (addr, server) = spawn_server(state.clone()).await;
    let client = mcp_client(addr).await;

    let schema: Value =
        serde_json::from_str(include_str!("fixtures/exa_schema.json")).expect("schema json");
    let validator = jsonschema::validator_for(&schema).expect("compile schema");

    let result = client
        .call_tool(
            CallToolRequestParams::new("exa_search")
                .with_arguments(args(json!({"query": "exa probe", "num_results": 5}))),
        )
        .await
        .expect("call exa_search");
    let body = structured(&result);
    if let Err(err) = validator.validate(body) {
        let details: Vec<String> = validator
            .iter_errors(body)
            .map(|e| format!("{e} at {}", e.instance_path()))
            .collect();
        panic!("exa_search output violates the frozen schema: {err}\n{details:?}\n{body}");
    }
    assert_eq!(body["tool_source"], json!("web"), "{body}");
    assert_eq!(body["searchType"], json!("auto"), "{body}");
    assert_eq!(body["costDollars"]["total"], json!(0.0), "{body}");
    body["requestId"]
        .as_str()
        .expect("requestId")
        .parse::<uuid::Uuid>()
        .expect("requestId is a UUID");
    let first = &body["results"][0];
    assert!(
        first["url"].as_str().unwrap().starts_with("http"),
        "{first}"
    );
    assert_eq!(
        first["highlights"].as_array().unwrap().len(),
        first["highlightScores"].as_array().unwrap().len(),
        "highlights/scores lengths must match"
    );

    // `source = "history"` short-circuits to `clicks`: seed one click via the
    // store, then the tool returns it without touching the engines.
    state
        .store()
        .record_click(cauce_core::ClickRow {
            id: None,
            ts: chrono::Utc::now(),
            query_hash: None,
            url: "https://example.com/clicked".parse().unwrap(),
            title: "clicked result".to_string(),
            position: 0,
            client: ClientKind::Ui,
        })
        .await
        .expect("record click");
    let result = client
        .call_tool(
            CallToolRequestParams::new("exa_search")
                .with_arguments(args(json!({"query": "", "source": "history"}))),
        )
        .await
        .expect("call exa_search history");
    let body = structured(&result);
    assert_eq!(body["tool_source"], json!("history"), "{body}");
    request_id_of(body, "request_id");
    let rows = body["results"].as_array().expect("history results");
    assert_eq!(rows.len(), 1, "{body}");
    assert_eq!(
        rows[0]["url"],
        json!("https://example.com/clicked"),
        "{body}"
    );

    client.cancel().await.expect("cancel");
    server.abort();
}

/// Portless-alias regression: a `/mcp` POST carrying `Host:
/// search.localhost` (or any `*.localhost` name) must pass rmcp's host
/// check because `host_origin_guard` already enforces the loopback rules
/// upstream; a truly foreign Host must still be a 403 there.
#[tokio::test]
async fn mcp_http_allows_portless_alias_host() {
    let (state, _tmp) = test_state();
    let (addr, server) = spawn_server(state).await;
    let url = format!("http://{addr}/mcp");
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });
    let http = reqwest::Client::new();

    // search.localhost is loopback per W1-13: the request reaches the MCP
    // handler rather than dying in rmcp's exact-match host list.
    let resp = http
        .post(&url)
        .header("host", "search.localhost")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .await
        .expect("post");
    assert_ne!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "search.localhost must not be 403: {:?}",
        resp.text().await
    );

    // A non-loopback host is still rejected by the W1-13 guard.
    let resp = http
        .post(&url)
        .header("host", "evil.example.com")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    server.abort();
}

/// W5-01: `fetch_and_index` over a mock origin — the tool fetches,
/// extracts markdown and writes the `pages` row, returning the row as
/// structured content. The `mcp.fetch_and_index` audit row lands like
/// every tool's.
#[cfg(feature = "archive")]
#[tokio::test]
async fn mcp_fetch_and_index_writes_page() {
    let origin = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/doc"))
        .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
            "../../cauce-core/tests/fixtures/archive/10-minimal.html"
        )))
        .mount(&origin)
        .await;

    let (state, _tmp) = test_state();
    let (addr, server) = spawn_server(state.clone()).await;
    let client = mcp_client(addr).await;

    let page_url = format!("{}/doc", origin.uri());
    let result = client
        .call_tool(
            CallToolRequestParams::new("fetch_and_index")
                .with_arguments(args(json!({ "url": page_url }))),
        )
        .await
        .expect("fetch_and_index");
    let body = structured(&result);
    assert_eq!(body["url"], page_url);
    assert!(
        body["markdown"]
            .as_str()
            .is_some_and(|m| m.contains("patience as a method")),
        "markdown: {body}"
    );
    assert!(body["byte_len"].as_u64().is_some_and(|n| n > 0));

    // The row landed in `pages` and the audit trail recorded the tool.
    let stored = state
        .store()
        .get_page(&url::Url::parse(&page_url).unwrap())
        .await
        .unwrap()
        .expect("pages row written");
    assert_eq!(stored.markdown, body["markdown"].as_str().unwrap());
    let audits = state
        .store()
        .list_audit(&AuditFilter {
            action: Some("mcp.fetch_and_index".to_string()),
            ..AuditFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(audits.len(), 1);

    // A bad URL is invalid_params, not a panic.
    let err = client
        .call_tool(
            CallToolRequestParams::new("fetch_and_index")
                .with_arguments(args(json!({ "url": "not a url" }))),
        )
        .await
        .expect_err("invalid url");
    assert!(
        err.to_string().contains("invalid url"),
        "error names the input: {err}"
    );

    server.abort();
}
