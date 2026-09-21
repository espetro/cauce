# Top 5 ROI changes to make oxe agentic-first / Perplexity-competitive

Date: 2026-09-16
Mode: research / read-only
Inputs: four parallel subagent POVs (Perplexity/Gemini/ChatGPT comparison, agent-developer integration, end-user UX, engineering architecture) + direct code read

Cross-reference rule used: a change only makes the top 5 if it surfaces as high-severity in 2+ POVs AND the underlying code seams are clean (no invasive surgery).

---

## #1 Snippet-only answers are the foundation gap

POV: Perplexity comparator (#4, severity 9/10), engineering (#3, severity 7/10), agent-dev (#1, severity 7 — no `web_fetch`).
Effort: L. ROI: highest.

Problem. `oxe/exa_compat.py:67-84` sets `text = body` where `body` is DDG's ~200-char snippet. The `web_search` tool then truncates the same `text` to 800 chars (`oxe/ai.py:124`). The LLM is reasoning over a snippet no matter what model you pick.

What to do.
- Add `trafilatura>=1.6` as a new `oxe[ai]` extra in `pyproject.toml`.
- New `oxe/fetch.py` with `extract(url, timeout=5.0) -> str` (trafilatura with description fallback).
- Add a second tool to `oxe/ai.py::TOOLS_SPEC`: `web_fetch(url)` that returns the page body (or first 4 KB). The model now decides when a snippet isn't enough — no need for the tool loop to know.
- Add a new MCP tool `exa_get_contents(urls)` in `oxe/mcp_server.py` so external agents get this through MCP too.
- Keep the HTTP `/search` payload snippet-only (don't blow the cache); extraction only happens inside the AI pipeline.

Why this is the foundation: every other gap (better citations, freshness, related questions depth, multi-query research) hits a ceiling if the model can't read the pages it cites. Doing this first means #2-#5 all layer cleanly on top.

Why two tools (`web_search` + `web_fetch`) instead of fusing: lets the model be smart about when to spend tokens on a full read.

---

## #2 Parallelize the ReAct tool loop + default-fanout backend

POV: engineering (#1, severity 6) + engineering (#2, severity 8) + Perplexity (#10, severity 7) + agent-dev (#5, severity 6).
Effort: M. ROI: high.

Problem. Two compounding sequential costs:
- `oxe/ai.py:390-416` runs `tool_calls` in a `for tc in tool_calls:` loop — 3 sub-queries = 3× sequential DDG RTTs. The LLM round-trip is held idle while the network is the only thing doing work.
- `oxe/registry.py:84-98` defaults to bare `DDGBackend()` when `OXE_BACKENDS` is unset. The `FallbackBackend` and `FanoutBackend` composers (`oxe/backends.py:127-195`) exist and work, but defaults to single-engine DDG = the brittleness described in `.agents/MEMORY.md` (`_PAGE_TTL=300`).

What to do.
- `oxe/ai.py:390-410`: change the per-round loop to `asyncio.gather(*[asyncio.to_thread(fn, **args) for tc in tool_calls])`, then append results in tool_call_id order to preserve the conversation contract.
- `oxe/registry.py:84`: when `OXE_BACKENDS` is unset, return `FallbackBackend([DdgsBackend("ddg"), DdgsBackend("wikipedia"), DdgsBackend("bing")])` instead of `DDGBackend()`. The composers are battle-tested and already honor per-engine cache namespacing.
- Concurrent safety: the fanout fallback stays sequential by design; the per-round gather happens *inside* the single chosen backend's tool run.

Why this matters together: parallelizing the tool loop only helps if the underlying backends are reliable enough to come back with results in parallel. Default-fanout-fallback eliminates the "DDG died, agent loop dies" failure mode that pairs with parallelization to make the answer engine actually trustworthy.

Tests needed: `test_parallel_tool_calls` (use `time.sleep` fixture to assert gather semantics); `test_fallback_default` (assert `build_from_env()` returns a `FallbackBackend` when env unset).

---

## #3 Empty-results indistinguishable from backend failure

POV: agent-dev (#5, severity highest blocker listed), engineering (POV implicity), Perplexity (POV-5 indirectly).
Effort: S. ROI: high (debuggability for every agent that integrates oxe).

Problem. When DDG fails, `oxe/exa_compat.py:150-153` returns `{"results": []}` with `last_err` only logged. `FallbackBackend` (`oxe/backends.py:154`) raises `BackendError`, but the default `DDGBackend` path swallows it. At the HTTP level, "no hits" and "DDG is rate-limited" are indistinguishable — agents in retry loops can't tell.

What to do.
- `oxe/exa_compat.py:150`: when `raw` is empty AND `last_err is not None`, return `{"results": [], "_error": str(last_err)[:200], "_error_kind": "upstream"}`.
- `oxe/backends.py:154` (`FallbackBackend`): similarly include the failed backend's error in the payload when the whole chain fails.
- Add `X-Cache: HIT | MISS` header on `/search` (`oxe/server.py:205-242`).
- Extend `ExaRequest` response model in pydantic to admit `_error`, `_error_kind`, `_duration_ms`, `_cached_at` so clients can read them.
- Surface `_error` in the UI empty/error state at `ui/src/routes/search.tsx:206-225` (already has retry + ask-AI; just add a small "DDG is rate-limited — try again" line).

Why S effort: it's plumbing, not feature work. Why this ROI: agent-dev specifically called it the #1 integration blocker.

---

## #4 History page deep-linkable (since / qf / force URL state) + copy-answer / regenerate in AI view

POV: end-user UX (#3 history severity 6, #2 severity 3 wins low-cost), agent-dev (every POV uses URL as canonical state).
Effort: S-M. ROI: high (perceived ParityTax reduction).

Problem. `ui/AGENTS.md:64` lists `since`, `qf`, `force`, `suggest` URL params as "planned/questionable". `grep` confirms they're not wired anywhere — the History page's time/query filters live in component state only and vanish on refresh. Sharing a filtered history view is impossible. Separately, `AnswerView` (`ui/src/features/answer/AnswerView.tsx:17,30-40`) lacks `copy answer` and `regenerate` controls that every Perplexity/ChatGPT user expects.

What to do.
- `ui/src/routes/history.tsx:42` (+`:139` filter UI): read `since`/`qf` from `useLocation().query` on mount using the same pattern as `search.tsx:57-64`; write back via `route()` on filter change. ~30 lines.
- Add `Regenerate` button (re-submit current query, force `?nocache=1`) at `ui/src/features/answer/AnswerView.tsx:35-40`.
- Add `Copy` button (clipboard write of the rendered markdown text, fallback to plain text).
- Implement `?force=error|ai-off|empty` stubs behind `import.meta.env.DEV` (covers the QA checkpoint gap flagged in `userflow-checkpoints.md:42-44`).

Why S-M effort: the URL-state pattern is already proven in the codebase (`search.tsx`). Copy/regenerate is two new buttons in one file. Why this ROI: end-user POV's #1 and #3 highest-impact wins; opens the door to agent QA on URL-state contracts.

---

## #5 MCP tool surface: add `exa_get_contents` (page extraction) + `exa_answer` (server-side orchestration shortcut) + URL canonicalization

POV: agent-dev #1 (no `web_fetch` blocker), agent-dev #8 (cached /answer hangs), Perplexity #6.
Effort: M. ROI: high (changes oxe from "search proxy" to "answer engine for agents").

Problem. oxe exposes exactly two MCP tools (`oxe/mcp_server.py:32-104`): `exa_search`, `exa_user_history`. An agent that gets URLs from `exa_search` has no oxe-native way to follow them. An agent that wants a one-shot answer instead of driving its own ReAct loop has to either re-implement the orchestrator or call `/answer` (HTTP only, not MCP). Reference surfaces (Tavily, Jina, Kagi, Exa MCP): typically offer 4-6 tools covering search / fetch / summarize / answer.

What to do.
- `oxe/mcp_server.py:32`: add `@mcp.tool(name="exa_get_contents", description="fetch and extract page body from one or more URLs")` that wraps `oxe/fetch.extract()` (introduced in #1).
- `oxe/mcp_server.py:32`: add `@mcp.tool(name="exa_answer", description="server-orchestrated streaming answer with cited sources, returns final {answer, sources, related_questions, confidence}")`. Reuse a sync wrapper around `stream_answer` collected to its terminal event.
- New `oxe/exa_compat.py::normalize_url()` stripper (utm, fbclid, gclid, ref, mc_cid/eid, lowercase host, drop fragment). Apply in `_dgr_to_exa`, `record_click`, `_sources_from_messages`. Dedupe cache rows and click history.
- Fix the cached `/answer` SSE bug from agent-dev #8: at `oxe/server.py:480-489`, emit a synthetic `{"type":"delta", "text":""}` event before `done` so consumers that wait for `delta` don't hang.
- Add `X-Cache: HIT | MISS` and `_duration_ms` on `/search` and `/answer` (see #3 plumbing).
- Optional env gate `OXE_MCP_TOOLS` to opt into the new tools if any downstream consumes the old 2-tool schema strictly.

Why M effort: 3 small files + tests + URL normalization. Why this ROI: closes the gap oxe has versus Jina/Tavily MCP servers; makes the `/answer` HTTP path usable as an MCP tool, which is what an agent dev wants.

---

## What's deliberately NOT on this list (close-but-not-top-5)

| Candidate | Why deferred |
|---|---|
| Multi-query planner (Perplexity POV-3, severity 8, effort L) | Requires prompt engineering + planner prompt; ROI depends on #1 shipping first so the planner has rich content. Add in v0.6. |
| Rich markdown rendering — fenced code, tables, images (UX POV-2) | Spec explicitly chose card-less / "lite" markdown to stay under 40 KB JS budget. Cost-to-value unfavorable until users complain. |
| Multimodal input (Perplexity POV-8) | Effort L, severity 4 for agentic use. Defer. |
| Default fanout to 2 engines (engineering #2 / Perplexity POV-9) | Folded into #2 above (default to `FallbackBackend([ddg, wikipedia, bing])` — gets diversity *and* resilience). |
| Provider-errors stats table (engineering #10) | Operationally nice; not user-visible; needs SQL migration. v0.6. |
| Answer cache version hash (engineering #4) | Real bug but low-frequency trigger (only when TOOLS_SPEC changes); cheap to ship alongside #1. |
| Settings hot-reload memo (engineering #11) | Already works correctly. |
| URL normalize, SSE cancel events, etc. | Folded into #5 (URL norm) and tracked separately for #1's fetch helper. |

## Suggested sequencing for v0.5

1. **PR 1**: #1 (fetcher) — foundation.
2. **PR 2**: #3 (empty-results-vs-error) — quick, ships independently, unblocks agents.
3. **PR 3**: #2 (parallel tool loop + default-fallback backend) — depends on PR 1.
4. **PR 4**: #5 (MCP tools `exa_get_contents` + `exa_answer` + URL norm + cached /answer delta fix) — depends on PR 1.
5. **PR 5**: #4 (History URL state + copy/regenerate) — pure UI, independent.

Net effect at v0.5: oxe goes from "search proxy with a single-shot ReAct loop" to "agent-native answer engine with full-text extraction, parallel research, MCP parity with Jina/Tavily, and a UI that deep-links its own state."

## Subagent reports (full, ~400 lines each)

Saved to the per-scratch dir as four markdown reports. Reproduce / dig-in:
- Perplexity/Gemini/ChatGPT comparator — 10 POVs (freshness, citation depth, multi-query, content fetch, structured answers, MCP depth, streaming UX, multimodal, diversity, latency).
- Agent-dev integrator — 8 POVs (MCP ergonomics, HTTP ergonomics, caching, observability, error handling, concurrency, auth, SSE shape).
- End-user UX — 10 POVs (results presentation, answer view, history, settings, landing, mobile, onboarding, AI toggle, related questions, empty/error states).
- Engineering/architecture — 15 POVs (sync tool loop, DDG brittleness, no extraction, answer-cache drift, freshness, search-cache key, SQLite ceiling, streaming DDG, hard-coded MAX_ITERS, provider-error stats, hot-reload, field drift, token efficiency, SSE cancel, URL canonicalization).
