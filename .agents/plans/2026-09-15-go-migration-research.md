# Go migration research — oxe v1

**Date:** 2026-09-15
**Mode:** research / recommendation
**Status:** opinion, not a decision
**Verdict (TL;DR):** Don't migrate to Go for v1. The pain oxe actually has is upstream of "wrong language", and a port solves the smallest of them at high cost. If we ever do migrate, it should be after v1 ships and the pain has crystallised into something concrete.

---

## What oxe is, sized

Pure inventory of the Python backend (`oxe/*.py`):

| Module | LOC | Role |
|---|---|---|
| `server.py` | 580 | FastAPI app, routes, content negotiation, request validation |
| `ui.py` | 388 | server-rendered HTML fallback (Jinja-style) |
| `ai.py` | 376 | ReAct loop over aisuite, SSE streaming, tool/function calling |
| `cache.py` | 350 | SQLite WAL, gzip JSON values, TTL, click log, search log |
| `stats.py` | 352 | static dashboard generator (no-JS, inline SVG) |
| `backends.py` | 193 | SearchBackend protocol, DdgsBackend, FallbackBackend, FanoutBackend |
| `config.py` | 188 | TOML loader, AIConfig, env-var parsing |
| `exa_compat.py` | 146 | DDG → Exa-shape translation |
| `mcp_server.py` | 105 | 2 MCP tools, thin wrapper over cache+exa_compat |
| `registry.py` | 98 | entry-point discovery for backends |
| `search.py` | 68 | the orchestrator (cache lookup, fetch, store, observe) |
| `sqlload.py` | 21 | aiosql wrapper |
| `__main__.py` | 42 | CLI entry, signal handling |
| `__init__.py` | 21 | version + re-exports |
| **Total** | **~2,928 LOC** | |

Tests: ~924 LOC across 7 files. Backend has 0 compiled deps and pure-Python install via `uv tool install`. Pinned to Py 3.10+. Runtime footprint ~70 MB RSS, single process.

A Go port of equivalent surface area (without the AI/ReAct loop or templates) is realistically 4–6k LOC of Go. With them, 10k+. That's the magnitude of the bet.

---

## Hypotheses in priority order

I'll grade each "should we migrate" reason on (a) does it actually apply to oxe as it is today, (b) is Go actually the fix, and (c) cost to act.

### H1. "Single static binary is easier to ship"

**Applies, partially. Go solves it, but at the wrong scope.**

`uv tool install oxe` is already a one-liner that pulls a pinned wheel with no system deps (except Python, which every dev machine and every AI-agent runtime already has — Claude Code, Hermes, maki all spawn Python through uv). Distribution is not a complaint anyone has voiced. The downside Go removes is "needs Python on PATH"; the upside oxe would get in return is one ~12 MB static binary. The audience (LLM agent hosts) already has the runtime that oxe targets.

**Verdict:** Real benefit, wrong audience. **Cost: high** (full rewrite). **Don't do it for this.**

### H2. "Lower RSS, faster startup, fewer deps"

**Applies, but the ceiling is small.**

70 MB RSS and ~150 ms cold start (Python+FastAPI+uvicorn) is already acceptable for a tool that spawns once per host and lives for hours. There is no scenario in oxe's current usage where this matters — the bottleneck is network round-trips to DuckDuckGo (hundreds of ms to seconds), not Python startup. A Go binary would land at ~10 MB RSS and ~10 ms cold. That's a 7x improvement on a number that isn't on the critical path.

**Verdict:** Measurable but cosmetic. **Cost: high** for ~zero user-visible gain.

### H3. "Concurrency model is cleaner in Go"

**Doesn't apply yet.**

oxe today is async-single-loop (FastAPI/uvicorn/asyncio). There's exactly one place that fans out work concurrently: `FanoutBackend` in `oxe/backends.py` — a single asyncio.gather over backend calls. There's no streaming aggregator, no worker pool, no sharded state. Go's goroutines would shine here, but there's nothing to shine *at*.

If/when we add a fan-out crawler, scrape-with-timeouts, or distributed cache, Go's concurrency story would matter. Today it would just be swapped for what we have.

**Verdict:** Future-looking. **Cost: high** for an unproven future need.

### H4. "The Python dep tree is fragile / risky"

**Applies partially — and not because of Python.**

Concretely, oxe's runtime deps are:

- `ddgs>=9.16.0` — actively maintained (deedy5, 22 issues, weekly releases through May 2026, ~6.7M monthly downloads). The DDG scraping surface is *the* risk surface, not Python.
- `fastapi>=0.110`, `uvicorn>=0.27`, `pydantic>=2.0` — mature, boring, near-zero churn.
- `mcp>=2.0` — the official MCP Python SDK; this *is* the risk: it's the youngest dep, still 2.x, has churn. But porting the MCP *server* (we expose it, we don't consume it) to a Go MCP server library is ~80 LOC of glue, and it would be needed anyway because oxe can't expose MCP without some MCP SDK.
- `aiosql>=15.0` — 21 LOC wrapper. Trivial to replace with `database/sql`.

The fragile dep is `ddgs`, and a Go port would swap it for one of:
- `github.com/velariumai/go-ddgs` (v0.2.2, April 2026 — 8 months old, niche, single-maintainer)
- `github.com/jcalvert/metawebsearch` (March 2026 — 5 months old, niche)
- `github.com/Djarvur/ddg-search` (Feb 2026 — 7 months old, niche)
- `github.com/kuhahalong/ddgsearch` (Dec 2024 — 18 months old)

vs. `ddgs` (May 2026, 2.7k stars, ~6.7M downloads/month, 91-day release cadence, the reference impl everyone copies). **The Go ecosystem for DDG is younger, less maintained, and more fragmented than the Python one.** The migration trades a known quantity for several unknowns.

**Verdict:** Real concern, wrong solution. **The Go DDG libs are weaker than the Python one we're already using.** If we want to de-risk the search backend, the answer is "vendor `ddgs` in a subprocess we shell out to" or "write a thin HTML scraper over `html.duckduckgo.com` ourselves", not "port the whole server."

### H5. "The web UI is the product; the Python server is just plumbing"

**Mostly applies. Speaks to scope, not language.**

Look at the repo: `ui/` is Preact + Tailwind + daisyUI + valibot, sized in its own workspace with its own mise tasks. The "oxe" product (the search experience) lives in `ui/src/`. The Python server is plumbing: cache + protocol adapter + MCP exposure. It is invisible to the user. If the server shipped in a different language and had the same `POST /search` shape, the user would not notice.

This is the strongest argument for a Go port — but only because it argues we don't need much of what we have, not because Go is special. The Python server could shrink by half with no user-visible change too.

**Verdict:** Highest-leverage point, but it's a "delete code" insight, not a "rewrite in Go" insight. **Cost: low** to act on (Python), **high** if we frame it as a port.

### H6. "Type safety / refactor confidence"

**Applies, partially solved.**

Pydantic 2 + FastAPI gives us 80% of type safety at the boundary. There is no Pydantic-free zone in oxe where types matter for correctness — the inner code is plain dicts and dataclasses. If we want stricter typing we can adopt `pyright --strict` for ~one afternoon. Switching languages for this is not the right cost.

**Verdict:** Solve in Python. **Cost: low.** The cost in Go is the whole rewrite.

### H7. "Better packaging for oxmgr / launchd / systemd"

**Already solved.**

`uv tool install oxe` → `oxe` on PATH → `nohup oxe &`. oxmgr already supervises it. Systemd/launchd unit is one stanza. There is no distribution problem here.

**Verdict:** Not a real problem.

### H8. "Performance under load"

**Doesn't apply.**

oxe is local-first, bound to `127.0.0.1`, single-tenant (one human + their agents per host). There is no multi-tenant load. SQLite WAL handles thousands of req/s on local disk. Even the `/stats` dashboard SSG is built on-demand and cached. A Go binary would not change throughput in any user-visible way.

**Verdict:** N/A.

### H9. "Hiring / contributor pool"

**Applies weakly in reverse.**

Go is easier to onboard a contributor into than Python? No — Python is the easier language. The contributor pool for "Exa-compatible search proxy + MCP server + click history" is tiny either way; the bottleneck is "wants to work on oxe specifically", not "knows the language". oxe's commit history is one human (deedy5/ddgs is someone else's problem).

**Verdict:** N/A.

---

## What the migration would actually require

A faithful v1 Go port needs:

1. **HTTP server**: chi or gin, ~150 LOC for routes + middleware.
2. **SQLite layer**: `modernc.org/sqlite` (cgo-free) or `mattn/go-sqlite3`. ~400 LOC for cache, click log, search log, schema migrations. The aiosql→SQL file separation would need an analog (sqlc or inlined named queries).
3. **Exa-compat translation**: 1:1 from `exa_compat.py`, ~150 LOC.
4. **DDG backend**: pick one of four under-maintained Go libs, or hand-roll a scraper. ~200–500 LOC.
5. **MCP server**: `github.com/modelcontextprotocol/go-sdk` (still 0.x as of search; check version). ~150 LOC for 2 tools.
6. **Templates (server-rendered fallback UI)**: `html/template`. ~400 LOC to mirror `oxe/ui.py` + `oxe/static/*.html`.
7. **Stats dashboard SSG**: 1:1 from `oxe/stats.py`, ~350 LOC.
8. **AI ReAct loop (oxe/ai.py)**: drop it for v1 in Go, or write a port. This is the part with the highest *non-obvious* risk: streaming tool-call accumulation, SSE event ordering, the worker-thread + done-Event pattern from `.agents/MEMORY.md`. ~400 LOC if done well.
9. **Config loader**: TOML via `github.com/BurntSushi/toml` or `github.com/pelletier/go-toml`, env-var parsing. ~200 LOC.
10. **Registry / entry points**: `plugin` package or manual. ~100 LOC.
11. **Tests**: a near-1:1 port of `tests/` (~924 LOC), no pytest fixture ecosystem.
12. **CI, mise tasks, build matrix**: rewrite mise.toml's `[tasks."dev:server"]`, `[tasks."serve"]`, `[tasks."lint:py"]`, etc. Drop `[tools] python` for `go`.

Realistic calendar: 6–10 weeks of full-time work for a single engineer who already knows oxe and Go. 4–6 weeks for a faster engineer. Either way, oxe ships no features for that entire window. v1 gets delayed by ~one quarter.

**Hidden cost:** every dep of `ddgs` is something we own now (DDG HTML scraping breakage, VQD token rotation, region/safesearch semantics). The `ddgs` library is at v9.16 in May 2026 with weekly releases specifically because this surface keeps moving. In Go we'd be the maintainers of that surface.

**Hidden cost #2:** we'd lose the Python-as-library embed path (`from oxe import make_app; make_app(...)`). The README documents `oxe` as both a binary and a Python library. The library audience is small but exists (the `make_app(cache=...)` factory in v0.2.0 was added specifically for it). A Go rewrite kills that audience.

---

## What I'd actually recommend, in order

1. **Keep Python for v1.** Ship v1.0 with the current stack. The reasons to migrate don't apply at v1; the cost is a quarter of stalled feature delivery; the hidden costs (DDG surface, loss of embed path) are real.
2. **If "easier distribution" actually becomes a complaint** (it has not been raised by any user or contributor as of this writing), the right answer is: keep Python, ship a `Dockerfile` and a Homebrew formula. That covers "no Python on host" without losing anything.
4. **If the AI/ReAct loop becomes a liability** (it isn't yet; v0.4 just shipped, tests pass), the right answer is to *remove optional AI from the default install* (`ai` extra already exists; consider making it a separate `oxe-ai` package), not to port the whole server.
5. **If concurrency pressure becomes real** (it has not), profile first. The fix is probably asyncio structured concurrency patterns or `anyio` task groups, not a language change.
6. **Document the "we considered Go for v1" decision in `.agents/MEMORY.md`** so the next agent doesn't re-litigate it.

### When Go *would* make sense

A future v2 *might* justify Go if **all three** of these are true at the same time:

- oxe has grown a fan-out crawler / scraper pool (concurrent I/O bottleneck becomes real).
- The user-facing web UI has migrated fully to a separate static host (or oxe becomes pure backend), and the Python-embed audience is gone.
- DDG scraping has stabilised into something a small Go lib can wrap (i.e., the surface stops moving), OR we've moved off DDG to a paid API with a stable Go SDK.

None of those are true today. Revisit when any one of them becomes true — and even then, weigh against "delete the AI module + slim FastAPI + add a Dockerfile" first.

---

## Sources

- oxe codebase (`oxe/*.py`, `tests/*.py`, `pyproject.toml`, `mise.toml`, `CHANGELOG.md`, `.agents/MEMORY.md`)
- PyPI + GitHub for `ddgs` (v9.16.0, May 2026; ~6.7M downloads/mo; 2.7k stars)
- Go DDG libs surveyed: `velariumai/go-ddgs` v0.2.2 (Apr 2026), `jcalvert/metawebsearch` (Mar 2026), `Djarvur/ddg-search` (Feb 2026), `kuhahalong/ddgsearch` (Dec 2024)

## Decision record (placeholder)

> **2026-09-15:** Considered migrating oxe to Go for v1.0. Decision: **don't**. Reasoning in `.agents/plans/2026-09-15-go-migration-research.md`. Re-evaluate if (a) DDG scraping stabilises, (b) we add a concurrent crawler, or (c) the Python-embed audience disappears.