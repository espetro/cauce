//! WASM / Serverless Deployment Harness Example
//!
//! Demonstrates running `cauce_server::build_router` in an edge / serverless
//! environment (such as Cloudflare Workers, AWS Lambda, Fastly Compute, or Spin)
//! without depending on native SQLite or TCP listeners.
//!
//! Run with:
//! `CARGO_TARGET_DIR=target cargo run -p cauce-server --example wasm_edge_harness`

use std::sync::Arc;

use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use cauce_core::config::Config;
use cauce_core::{
    Engine, EngineError, EngineId, MemoryStore, SearchPipeline, SearchRequest, SearchResult, Tier,
};
use cauce_server::{AppState, RouterOptions, build_router_opts};
use tower::ServiceExt;

/// A lightweight mock engine for testing the pipeline in edge environments
struct MockEdgeEngine;

#[async_trait::async_trait]
impl Engine for MockEdgeEngine {
    fn id(&self) -> EngineId {
        EngineId::from("mock-edge")
    }

    fn tier(&self) -> Tier {
        Tier::T1
    }

    fn page_size(&self) -> u8 {
        10
    }

    async fn search(
        &self,
        req: &SearchRequest,
        _budget: Duration,
    ) -> Result<Vec<SearchResult>, EngineError> {
        Ok(vec![SearchResult {
            url: url::Url::parse("https://edge.cauce.internal/result").unwrap(),
            title: format!("Edge result for {}", req.q),
            snippet: "Fast, in-memory edge execution result".to_string(),
            engine: self.id(),
            published: None,
            score: 1.0,
        }])
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Starting WASM / Serverless Ingress Harness ===");

    // 1. Initialize an ephemeral / WASM-compatible Store (MemoryStore or remote KV / D1)
    let store = Arc::new(MemoryStore::new());
    println!("[1/4] Initialized MemoryStore (zero native SQLite dependency)");

    // 2. Build the SearchPipeline with edge engines
    let engines: Vec<Arc<dyn Engine>> = vec![Arc::new(MockEdgeEngine)];
    let pipeline = Arc::new(SearchPipeline::new(store.clone(), engines));
    println!("[2/4] Initialized SearchPipeline with edge engine adapter");

    // 3. Assemble AppState
    let config = Config::default();
    let state = AppState::new(pipeline, store, config);
    println!("[3/4] Assembled AppState");

    // 4. Build Axum router (pure tower::Service, headless for edge API)
    let router = build_router_opts(state, RouterOptions::headless());
    println!("[4/4] Built Axum Router (tower::Service)");

    // 5. Simulate executing HTTP requests directly through the Service interface
    // (This is identical to how Cloudflare Workers `worker::event(fetch)` or AWS Lambda `service_fn` delegates)

    // Request A: Health check / OpenAPI capabilities
    let req_capabilities = Request::builder()
        .method("GET")
        .uri("/api/capabilities")
        .body(Body::empty())?;

    let response = router.clone().oneshot(req_capabilities).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 1024 * 1024).await?;
    println!(
        "\n✓ Handled GET /api/capabilities:\n{}",
        String::from_utf8_lossy(&body_bytes)
    );

    // Request B: Search query (served through pipeline & stored in MemoryStore)
    let req_search = Request::builder()
        .method("GET")
        .uri("/api/search?q=wasm+metasearch")
        .header("host", "127.0.0.1")
        .body(Body::empty())?;

    let response = router.clone().oneshot(req_search).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 1024 * 1024).await?;
    println!(
        "\n✓ Handled GET /api/search?q=wasm+metasearch:\n{}",
        String::from_utf8_lossy(&body_bytes)
    );

    println!("\n=== WASM & Serverless edge harness executed successfully! ===");
    Ok(())
}
