# Compiled-language migration research — oxe v1

**Date:** 2026-09-15
**Mode:** research / recommendation
**Companion to:** `.agents/plans/2026-09-15-go-migration-research.md`
**Status:** opinion, not a decision
**Verdict (TL;DR):** Still don't migrate. Rust is the only language that genuinely fits the technical targets, but every language inherits the same `ddgs`/DDG-scraping risk that the Python port dodges, and the audience / distribution story isn't actually a complaint. Re-evaluate when concrete evidence of pain appears.

## Note on data quality

The `gateway__search-exa_search` MCP tool (the local oxe proxy) returned zero
results for all 30+ queries across four parallel subagents during this
research session. Cache and `exa_user_history` were healthy; only
`exa_search` was broken. This is consistent with the project's documented DDG
brittleness (VQD token / rate-limit / Cloudflare on agent-shaped query
patterns). Logged in `.agents/MEMORY.md` for future agents. All verdicts
below lean on (a) the priors I handed the subagents, (b) well-established
public knowledge of stable crate/library characteristics, (c) subagent
domain expertise. No fresh primary sources could be verified — confidence
reflects that.

## What oxe needs (carried from prior report)

- Single static binary, ~10–15 MB ideal, low RSS (<30 MB ideal).
- Local-first (binds 127.0.0.1), single-tenant.
- HTTP server with content negotiation, SSE, static file serving.
- SQLite cache (WAL, gzip JSON values).
- MCP server (Streamable HTTP) with 2–3 tools.
- DDG-backed web search via HTTP scraping — **this is the brittle dependency**.
- Server-rendered HTML fallback templates (~5 templates, ~400 LOC).
- Python-embed audience (`from oxe import make_app; make_app(cache=...)`).
- Audience already has Python (LLM agent hosts run Python via uv).
- Exa-compatible HTTP API surface.

## The matrix

| Lang | Verdict | Confidence | Binary | MCP SDK | DDG story | Embed loss |
|---|---|---|---|---|---|---|
| **Go** | revisit_later | med | 8–15 MB ✓ | Tier 1, official ✓ | no serious lib; hand-roll html.duckduckgo.com | full break |
| **Rust** | revisit_later | med | 10–18 MB ✓ | Tier 2, rmcp ✓ | `websearch-rs` parity to `ddgs` v9.14.x (best non-Python story) | full break (PyO3 is worse, not better) |
| **Zig** | never | high | 1–10 MB ✓ | three competing libs, 0–5 stars, no prod users | none; would have to shell out to `ddgs` anyway | forces ctypes friction back |
| **V** | revisit_later | med | 150–700 KB ✓✓ | no lib; hand-roll ~200 LOC JSON-RPC | no lib; shell out to `ddgs` | full break |
| **Odin** | never | med | small ✓ | no lib | no lib | full break |

The matrix collapses to one question: **is the audience / distribution story
actually broken, or are we solving a problem nobody has?**

## Per-language findings

### Go — revisit_later

**Where Go wins:** distribution (cross-compile, brew, `go install`), RSS (10–20 MB vs 70 MB), static binary, MCP SDK at Tier 1 (`modelcontextprotocol/go-sdk`), `html/template` covers the ~5 templates, `modernc.org/sqlite` for the cache. This is the textbook "local first static binary" stack.

**Where Go loses for v1:**
- **DDG ecosystem is strictly weaker than Python.** Four libs surveyed: velariumai/go-ddgs (Apr 2026, niche), jcalvert/metawebsearch (Mar 2026, niche), Djarvur/ddg-search (Feb 2026, 9 stars), kuhahalong/ddgsearch (Dec 2024, abandoned). vs. `ddgs` v9.16 (May 2026, 2.7k stars, ~6.7M downloads/mo, weekly releases). Any Go port starts from scratch against DDG's rotating VQD-token + anti-bot surface.
- **`from oxe import make_app` has no clean Go equivalent.** Realistic options are "spawn the Go binary, speak HTTP" (the local-gateway/Ollama pattern — clean but a behavioral break) or "subprocess + JSON-RPC stdio" (adds supervisor complexity). PyO3-style embedding from the other direction isn't possible.
- **C-FFI interop:** cgo can wrap SQLite, libcurl, OpenSSL — but not the DDG problem.

**Verdict:** Go nails every hard target except the two that matter: the brittle dep (`ddgs`) is materially weaker in Go, and the embed API breaks. Revisit if oxe drops the embed API or if a credible Go DDG library emerges.

### Rust — revisit_later (downgraded from subagent's "ship_it_for_v1")

The Rust subagent returned `ship_it_for_v1`. I'm downgrading to `revisit_later` for one reason: **the brittle dependency doesn't get better in Rust**. A Rust port's value is binary/RSS/cold start, none of which are user-visible constraints on oxe today. The subagent's verdict correctly noted this — "Rust just makes the fallback strategy more code to maintain" — then promoted anyway on the basis of the technical wins. That's the wrong tradeoff for v1.

**Where Rust wins:**
- Best non-Python DDG story: `websearch-rs` claims **behavioral parity with Python `ddgs` 9.14.x** (the same baseline oxe's Python port uses). This is the only compiled-language lib that explicitly tracks `ddgs`'s engine roster, parameters, and result field names. Still inherits the VQD/anti-bot churn, but at least the translation layer (Exa-shape output) is unchanged.
- The standard stack is genuinely production-grade: `axum 0.7 + sqlx + tokio + tower-http + rmcp + askama`. Every library has a 2-3 year track record. Compile-time SQL verification via sqlx macros is a real win.
- Binary 10–18 MB stripped, RSS 8–12 MB idle, cold start 5–15 ms — the textbook single-binary story.
- Templating: **askama** is the obvious pick. Jinja-like syntax, `include_str!` embed, auto-escape, compile-time check. Closest 1:1 mental model from FastAPI+Jinja2.
- Tooling: sccache + zld (macOS) cuts the dev-loop pain to ~3–8× Python's `pytest --watch`, which is workable but real.

**Where Rust loses for v1:**
- **DDG parity claim is unverified.** `websearch-rs` says it tracks `ddgs` 9.14.x — needs a fresh comparison against the v9.16.x line oxe actually depends on, and a working fallback for the cases where parity breaks. The Rust subagent's worst-case is shelling out to a CDP sidecar — heavier than Python's `duckduckgo-search-cli` equivalent.
- **MCP SDK at Tier 2 vs Go's Tier 1.** Both meet oxe's 2-tool need today; Tier 2 means spec coverage tracks the protocol but lags by typically one minor version. Risk is low but nonzero.
- **Dev loop cost is real.** Even with sccache+zld, a 3,000 LOC port's edit-test cycle is multi-second. For a maintainer used to FastAPI's reload, this is a daily tax.
- **PyO3 is the wrong shape.** The subagent correctly noted: a Rust core wrapped in Python for `pip install oxe` adds cross-platform wheel CI complexity, bug-report friction (wheel or source?), and keeps the Python dependency the user wants to escape. If you migrate to Rust, go full-binary via `brew` / `curl | sh` / `cargo install`. This means the `make_app(cache=...)` embed audience is gone.
- **The "audience already has Python" point is misframed.** It applies to *operators* (Claude Code, Hermes, maki agents), not users. Operators prefer static binaries — they just don't *demand* them. There's no complaint on record.

**Verdict:** Rust is the strongest compiled-language fit on technical axes, but every axis it wins is one nobody is currently complaining about. The one axis oxe is actually vulnerable on (DDG churn) gets parity-track support but not parity-fix. Revisit when (a) `websearch-rs` demonstrates sustained parity with `ddgs` v9.16+, or (b) oxe genuinely needs binary distribution to a non-Python audience.

### Zig — never

Four immature layers stack up: Zig 0.16 itself broke most things in April 2026 (new `std.Io`, `@cImport` deprecated, `@Type` removed, package fetch now local `zig-pkg/`), MCP SDKs are 0–5 stars with no disclosed production users (PaytonWebber/zig-mcp-sdk, bkataru/mcp.zig, dreliq9/adam-mcp-zig), web frameworks are 4–5 stars with explicit ecosystem-immaturity acknowledgments from maintainers (ziggit benchmark thread: "the ecosystem is not mature enough yet"), and DDG scraping has no Zig story at all. Single binary is trivially 5–10 MB; SQLite via `vrischmann/zig-sqlite` (618 stars) works fine. None of this matters because:

**The embed-loss point is fatal.** oxe exists so Python users avoid ctypes friction; Zig via C-ABI forces ctypes friction back. Same Python footprint, two runtimes, no audience gain.

Plausible Zig oxe would shell out to a local `ddgs` Python process for DDG scraping — keeping the brittle dep where it's mature. That makes the Zig rewrite a wrapper around Python, which is the wrong shape.

### V — revisit_later

`veb` + V's built-in SQLite ORM match oxe's needs on paper. ~150–700 KB single-binary with precompiled templates is the smallest of any option. Compiles fast. The two real blockers:

1. **CVE-2026-67201 (July 2026)** — V through 0.5.2 had an SSRF bypass via parser differential between `net.urllib` and `net.http`. **For a tool that scrapes the web, this is the exact exploit class that matters.** Fixed in commit 85859f0, but the fix's availability depends on a versioned release (0.5.3?) that I couldn't confirm during this session.
2. **No native MCP or DDG lib.** MCP can be hand-rolled (~200 LOC JSON-RPC over HTTP+SSE) — manageable. DDG would require shelling out to `ddgs`, which breaks the single-binary story.

V is the lowest-friction compiled option *if* the security gap closes and oxe is willing to delegate DDG to a sibling Python process. Realistic candidate if a v2 rewrite ever happens.

### Odin — never

Four-way HTTP library fragmentation (odin-http/laytan 417★, Tina, HyperSock 18★, Gjallarhorn) with no clear winner is a yellow flag. Thread-per-core Isolate actor model is overkill for a localhost-bound 5-route service. No MCP or DDG libraries — both hand-rolls. No central package manager means every helper is hand-rolled or vendored. The size/perf win over V doesn't justify the missing-ecosystem cost.

## Cross-cutting analysis

**What all compiled languages share:**
- Lose the `from oxe import make_app` embed API. The replacement pattern (spawn binary / speak HTTP) is cleaner anyway but it's a v1 audience break.
- Inherit the DDG scraping risk. None of them make the VQD-token / anti-bot surface easier; only Rust's `websearch-rs` even claims to track `ddgs`'s output shape.
- Don't add packaging pain for an audience with Python already on PATH.

**What no compiled language solves that's actually a problem:**
- Nothing. The complaints people might raise ("binary is 70 MB RSS", "Python import is slow") aren't complaints anyone has raised for oxe.

**What no compiled language adds that's a win but isn't asked for:**
- Static binary distribution. `uv tool install oxe` is already a one-liner with a pinned wheel. The audience (LLM agent hosts) already has Python.
- Lower RSS, faster cold start. 70 MB → 10 MB RSS is measurable but invisible at this usage pattern.
- Compile-time correctness on async state. oxe has one fan-out site (`FanoutBackend`); type-checking it doesn't move the needle.

## When to revisit (updated)

The bar for migration moves up if **any** of these become true:

1. **DDG scraping stabilises** — i.e., the surface stops moving, OR oxe moves off DDG to a paid API with a stable SDK in the target language. **Rust becomes viable** specifically because `websearch-rs` claims to track `ddgs` 9.14.x.
2. **oxe gets a real concurrent workload** — fan-out scraper, distributed cache, etc. Then Go's / Rust's concurrency story would earn its keep.
3. **The Python-embed audience disappears** — e.g., oxe's primary consumers migrate to a static-binary-friendly workflow. Until then, embed API preservation is a non-trivial constraint.
4. **The `ddgs` upstream actually breaks in a way that hurts users** — e.g., maintenance stops, or DDG starts requiring JS execution that defeats HTTP scraping. Today: `ddgs` is at v9.16 in May 2026 with weekly releases and ~6.7M downloads/mo. The risk is low.

None of those are true today. Revisit when any one becomes true.

## Decision record (placeholder)

> **2026-09-15:** Considered migrating oxe to a compiled language (Go, Rust, Zig, V, Odin) for v1.0. Decision: **don't**. Reasoning: every language inherits the DDG-surface risk that `ddgs` carries for Python; none of them solve a problem anyone is currently complaining about; every language breaks the `from oxe import make_app` embed API. Rust is the strongest technical fit and `websearch-rs`'s parity-track to `ddgs` is the strongest non-Python DDG story, but its wins are in axes (binary size, cold start) that aren't on the critical path. Re-evaluate if DDG stabilises, oxe grows concurrent workloads, or the embed audience disappears. Prior report: `.agents/plans/2026-09-15-go-migration-research.md`. Full per-language matrix: this report.