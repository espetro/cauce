# WASM & Serverless Deployment Template (Trait-Based)

## Architecture Overview

`cauce` is architected around the Ports & Adapters (Hexagonal Architecture) pattern. Core domain logic does not depend on concrete databases, specific network listeners, or native OS runtimes.

Instead of rewriting or replacing Tokio and SQLite across the codebase, `cauce` abstracts persistence and ingress behind traits:
- **Storage Port**: `cauce_core::Store` (`Send + Sync + 'static`)
- **Egress Port**: `cauce_core::http::Egress` (traffic egress / HTTP client)
- **Ingress Port**: `cauce_server::build_router(app_state)` returning an `axum::Router` (a pure `tower::Service<Request<Body>, Response = Response<Body>>`)

```
               ┌────────────────────────────────────────────────────────┐
               │                  Deployment Harness                    │
               │   (Cloudflare Worker / WASI 0.2 Component / Lambda)   │
               └──────────────────────────┬─────────────────────────────┘
                                          │
                        ┌─────────────────┴─────────────────┐
                        ▼                                   ▼
             ┌─────────────────────┐             ┌─────────────────────┐
             │    Ingress Adapter   │             │   Platform Storage  │
             │ (tower::Service map)│             │ (implements Store)  │
             └──────────┬──────────┘             └──────────┬──────────┘
                        │                                   │
┌───────────────────────┼───────────────────────────────────┼──────────────────────┐
│ cauce-server          ▼                                   │                      │
│                build_router(state)                        │                      │
│                         │                                 │                      │
│ cauce-core              ▼                                 ▼                      │
│                  SearchPipeline ───────────────▶ Arc<dyn Store>                  │
│                         │                         (Memory / D1 / KV / DynamoDB)  │
│                         ▼                                                        │
│                    Arc<dyn Engine> ────────────▶ Arc<dyn Egress>                 │
│                                                   (wasi:http / fetch)            │
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. The Storage Port (`Store`)

`cauce-core::Store` is defined as:

```rust
#[async_trait]
pub trait Store: Send + Sync {
    // Tier-1 exact cache
    async fn get_exact(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError>;
    async fn put(&self, key: &CacheKey, resp: &SearchResponse, ttl: Duration) -> Result<(), StoreError>;
    async fn evict_expired(&self, grace: Duration) -> Result<u64, StoreError>;

    // Tier-2 lexical FTS
    async fn get_lexical(&self, q: &str, limit: u8) -> Result<Vec<CachedSearch>, StoreError>;
    async fn search_cache_fts(&self, q: &str, limit: u32) -> Result<Vec<CacheResultHit>, StoreError>;

    // Cache admin & statistics
    async fn list_cache(&self, limit: u32, offset: u32) -> Result<Vec<CachedSearch>, StoreError>;
    async fn get_cache(&self, key: &CacheKey) -> Result<Option<CachedSearch>, StoreError>;
    async fn delete_cache(&self, key: &CacheKey) -> Result<bool, StoreError>;
    async fn clear_cache(&self) -> Result<u64, StoreError>;

    // AI answer cache & threads
    async fn get_answer(&self, key: &AnswerKey) -> Result<Option<CachedAnswer>, StoreError>;
    async fn put_answer(&self, key: &AnswerKey, row: &AnswerRow, ttl: Duration) -> Result<(), StoreError>;
    async fn get_thread(&self, id: &Uuid, owner: &str) -> Result<Option<AnswerThreadRow>, StoreError>;
    async fn put_turn(&self, id: &Uuid, owner: &str, title: &str, turn: &AnswerTurn) -> Result<(), StoreError>;
    async fn delete_thread(&self, id: &Uuid, owner: &str) -> Result<bool, StoreError>;

    // Engine health & audit logging
    async fn get_engine_health(&self, engine: &EngineId) -> Result<Option<EngineHealthRow>, StoreError>;
    async fn update_engine_health(&self, row: &EngineHealthRow) -> Result<(), StoreError>;
    async fn write_audit(&self, entry: &AuditEntry) -> Result<(), StoreError>;
    async fn list_audit(&self, filter: &AuditFilter) -> Result<Vec<AuditRow>, StoreError>;
}
```

### Implementing `Store` for WASM / Serverless Environments

In a serverless runtime, instead of locking a local SQLite file on disk, you provide an edge-compatible `Store` implementation:

1. **Ephemeral Memory Store (`MemoryStore`)**:
   Ideal for edge functions where cache persistence is short-lived or local to the warm instance. Backed by `Arc<RwLock<HashMap<CacheKey, CachedSearch>>>`.
2. **Cloudflare D1 / Hyperdrive (`D1Store`)**:
   Issues SQL queries over Cloudflare's D1 API (`worker::d1::D1Database`), executing identical SQL schemas without POSIX file locks.
3. **HTTP Remote Store (`RemoteStore`)**:
   Proxies cache lookups to a central storage service or DynamoDB / KV.

---

## 2. Ingress Without Native Tokio Sockets

`cauce_server::build_router(state)` yields an `axum::Router`. Because Axum routers implement `tower::Service<http::Request<Body>>`, you do not need `tokio::net::TcpListener` or a multi-threaded OS reactor.

### A. Cloudflare Workers (Rust / WASM) Template

Using the `worker` crate, the Worker receives an incoming request and forwards it through the Tower router:

```rust
use worker::*;
use tower_service::Service;
use cauce_server::{build_router, AppState, RouterOptions};
use cauce_core::SearchPipeline;
use std::sync::Arc;

// Edge Store implementation (e.g. D1 or Memory)
struct WorkerD1Store {
    db: D1Database,
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    env: Env,
    _ctx: Context,
) -> Result<HttpResponse> {
    // 1. Initialize store from environment (D1 binding)
    let d1 = env.d1("DB")?;
    let store = Arc::new(WorkerD1Store::new(d1));

    // 2. Initialize pipeline with WASM-compatible egress
    let pipeline = Arc::new(SearchPipeline::new(...));

    // 3. Assemble AppState
    let config = cauce_core::Config::default();
    let state = AppState::new(pipeline, store, config);

    // 4. Build Axum router
    let mut router = build_router(state);

    // 5. Convert HttpRequest -> tower Request, execute, and return HttpResponse
    let response = router.call(req).await.map_err(|e| Error::RustError(e.to_string()))?;
    Ok(response)
}
```

### B. AWS Lambda (WASM or Custom Runtime) Template

With AWS Lambda Web Adapter (LWA) or `lambda_http`:

```rust
use lambda_http::{run, service_fn, Error, Request, Response};
use tower::Service;
use cauce_server::{build_router, AppState};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let store = Arc::new(DynamoOrHttpStore::new().await);
    let pipeline = Arc::new(SearchPipeline::new(...));
    let state = AppState::new(pipeline, store, Default::default());
    let mut router = build_router(state);

    run(service_fn(move |req: Request| {
        let mut router = router.clone();
        async move {
            router.call(req).await
        }
    })).await
}
```

### C. WASI 0.2 (`wasm32-wasip2`) Component Template

Using `wasi:http/incoming-handler` (Spin, Wasmtime, Fastly):

```rust
// WIT declaration:
// export wasi:http/incoming-handler@0.2.0;

impl Guest for ServerComponent {
    fn handle(request: IncomingRequest, response_out: ResponseOutparam) {
        let (parts, body) = wasi_to_http(request);
        let http_req = http::Request::from_parts(parts, body);

        // Execute through cauce_server router
        let state = get_app_state();
        let mut router = build_router(state);

        wasi_spawn(async move {
            let res = router.call(http_req).await.unwrap();
            http_to_wasi(res, response_out);
        });
    }
}
```

---

## 3. Benefits of the Trait Pattern

1. **Zero Disruption to Existing Code**: The native single-binary `cauce serve` continues using `tokio::net::TcpListener` and `cauce-store-sqlite` with zero regressions or runtime penalties.
2. **Pluggable Storage**: Swapping SQLite for Postgres, DynamoDB, Cloudflare D1, or in-memory requires only a new crate implementing `cauce_core::Store`.
3. **Pluggable Network Egress**: Engine fetches remain isolated behind `cauce_core::http::Egress`.
4. **Binary Footprint Control**: Edge and WASM targets can disable native dependencies via Cargo features (`default-features = false`).
