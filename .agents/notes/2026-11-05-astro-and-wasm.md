# Architectural Assessment: Astro Frontend & WASM 0.2 Target

**Date:** 2026-11-05
**Author:** Agent pair programming session

## 1. Astro + Svelte Islands for Frontend & SEO

### Context & Assessment
- **Problem**: `cauce.fyi`'s Svelte SPA currently serves `index.html` on all paths, creating soft-404s for machine discovery scanners and providing no pre-rendered HTML for search crawlers and AI bots.
- **Solution**: Migrate the web marketing surface to Astro with Svelte 5 islands.
- **Advantages**:
  1. Complete static pre-rendering (SSG) for all marketing routes (`/`, `/compare`, `/engines`, `/install`).
  2. 0 KB JavaScript by default for static copy, cutting page weight.
  3. Svelte 5 runes (`$state`, `$derived`, `$props`) run as interactive islands (`<Omnibox client:load />`, `<AnswerStream client:visible />`).
  4. Build output in `output: 'static'` mode emits plain HTML/CSS/JS that `rust-embed` bundles into `cauce serve` with zero change to the single-binary deployment contract.
  5. Built-in `@astrojs/sitemap`, OpenGraph, and structured JSON-LD support.

---

## 2. WASM 0.2 (WASI Preview 2) Compilation

### Context & Assessment
- **Feasibility**: High across `cauce-core` and declarative `cauce-engines`; requires an adapter layer for `cauce-server`.
- **Target**: `wasm32-wasip2` Component Model.
- **Key Adaptations**:
  1. Replace `tokio::net::TcpListener` with an export of `wasi:http/incoming-handler@0.2.0`.
  2. Implement a `MemoryStore` or KV/D1 store for platforms lacking POSIX file locking (e.g., Cloudflare Workers).
  3. Gate OS-specific metrics (`sysinfo`) and signals (`signal-hook`).
  4. Outbound engine requests route through `wasi:http/outgoing-handler`.
