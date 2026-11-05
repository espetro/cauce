# Architecture Assessment, VPS Migration & WASM Trait Pattern

Date: 2026-11-05

## Context

Following the completion of the 12 agent-readiness fixes, user evaluated:
1. Migrating `cauce.fyi` completely to the VPS via `cloudflared tunnel` (rather than maintaining edge proxy functions on Cloudflare Pages).
2. Assessing whether the project needs to be rearchitected.
3. Defining a trait-based approach for WASM/serverless (providing traits that Tokio/SQLite implement, and building deployment templates for Lambda/Cloudflare).

---

## 1. Cloudflare Pages vs VPS Migration Assessment

### Findings:
- `cauce.fyi` was previously configured as a custom domain on Cloudflare Pages (`cauce` project).
- Cloudflare Pages static file server intercepted deep paths and returned `index.html` (HTTP 200), causing soft-404s for automated agent scanners probing `/.well-known/*` and `auth.md`.
- To fix this, edge functions (`functions/`) were introduced to proxy API & discovery routes to `api.cauce.fyi`.
- However, `cauce serve` running on the VPS behind `cloudflared tunnel` (`vps-tunnel`, ID `1dad9cd2-857b-421f-822c-a2bc2cf7f389`) **already serves the complete application**:
  - Embedded Svelte SPA at `/app/`
  - `/` redirect (308) to `/app/`
  - All API routes (`/api/*`, `/health`, `/mcp`)
  - All discovery endpoints (`/.well-known/*`, `/robots.txt`, `/sitemap.xml`, `/auth.md`)
  - Content negotiation (`Accept: text/markdown`) and RFC 8288 `Link` headers
  - Proper HTTP 404 responses for unknown endpoints
- **Decision**: Migrate `cauce.fyi` directly to `vps-tunnel` on the VPS. This eliminates Cloudflare Pages, eliminates edge functions (`functions/`), removes the 100k invocations/day quota concern, and unifies the domain to a single origin.
- Documented in `docs/deploy/vps-migration.md`.

---

## 2. Project Architecture Assessment

### Question: "Do we need to rearchitect the project?"
**Verdict: No.**

### Detailed Evaluation:
1. **Separation of Concerns**:
   - `crates/cauce-core` has zero dependency on SQLite, Axum, or Tokio network sockets. It is pure domain logic, query normalization, pipeline orchestration, and abstract ports.
   - Ports already declared:
     - `Store`: Asynchronous persistence contract (`pub trait Store: Send + Sync`).
     - `Engine`: Upstream search engine execution contract (`pub trait Engine: Send + Sync`).
     - `Egress`: Outgoing HTTP egress contract (`pub trait Egress: Send + Sync`).
     - `ChatProvider`: LLM provider client contract.
2. **Web Layer**:
   - `crates/cauce-server` constructs an `axum::Router` which is a pure `tower::Service`.
   - It takes `Arc<dyn Store>`, not `SqliteStore`.
3. **Adapters**:
   - `crates/cauce-store-sqlite` is a modular adapter implementing `Store`.
   - `crates/cauce-engines` is an adapter for engines.
   - `crates/cauce-cli` is the composition root.
4. **Conclusion**:
   The architectural boundaries are clean and maintainable. No structural rewrite is warranted.

---

## 3. WASM & Serverless Trait Pattern

### Proposal:
- Instead of ripping out Tokio or SQLite from the core crates, leverage the existing Port/Adapter traits:
  - **Storage**: Any edge target implements `cauce_core::Store` (e.g. `WorkerD1Store`, `MemoryStore`, `DynamoStore`).
  - **Egress**: Any edge target implements `cauce_core::http::Egress` (e.g. `wasi:http` or `worker::fetch`).
  - **Ingress**: Axum router is fed directly via `tower::Service::call(req)` without native Tokio sockets.
- Documented in `docs/deploy/wasm-template.md`.
