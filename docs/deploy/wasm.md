# WASM 0.2 (WASI Preview 2) Component Deployment

This document describes compiling and deploying `cauce` to WASI 0.2 Component Model targets (`wasm32-wasip2`), including Spin (Fermyon), Wasmtime, Cloudflare Workers, and Fastly Compute.

## Architecture & Boundaries

```
┌────────────────────────────────────────────────────────┐
│             wasi:http/incoming-handler                 │
│               (WASI Preview 2 Export)                  │
└──────────────────────────┬─────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────┐
│               Tower / Axum Router Core                 │
│  - Routes & Handlers (`cauce-server`)                  │
│  - MCP Streamable-HTTP (`/mcp`)                        │
│  - Discovery Endpoints (`/.well-known/*`)              │
└──────────────┬──────────────────────────┬──────────────┘
               │                          │
               ▼                          ▼
┌──────────────────────────────┐ ┌──────────────────────┐
│  Search Pipeline & Ranking   │ │  Pluggable Store     │
│       (`cauce-core`)         │ │  (`wasi:filesystem`  │
│  - Scoring, Deduplication    │ │   or MemoryStore)    │
│  - Declarative Engines       │ └──────────────────────┘
└──────────────────────────────┘
```

## Key Adaptations for WASI 0.2

1. **Incoming HTTP Handler (`wasi:http`)**:
   Standard WASI 0.2 components do not listen on POSIX TCP sockets (`TcpListener`). Instead, they export `wasi:http/incoming-handler@0.2.0`. Axum's `Router` is adapted into an incoming-handler via Tower service conversion (`tower-to-wasi-http`).

2. **Storage (`Store` trait)**:
   - In Spin / Wasmtime: `cauce-store-sqlite` runs with bundled C SQLite over pre-opened directories (`--dir .`).
   - In Cloudflare Workers: SQLite file locking is not supported; use an in-memory TTL store (`MemoryStore`) or a Cloudflare D1 adapter.

3. **Outbound HTTP Fetching**:
   Declarative YAML engines perform outbound HTTP requests using `wasi:http/outgoing-handler@0.2.0` (or `fetch` on Workers).

4. **Engine Support**:
   - Declarative YAML engines (`reqwest` + `scraper`): fully supported.
   - Replay engine: fully supported.
   - Exec engines (`std::process::Command`): unsupported in WASI sandboxes.
