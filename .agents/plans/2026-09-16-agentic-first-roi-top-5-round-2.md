# Top 5 ROI changes v2 — angles the first batch missed

Date: 2026-09-16
Mode: research / read-only.
Input: second-round subagent POVs — distribution & adoption, privacy & security, search quality & reliability, observability & ops. Cross-referenced with the v1 list in `.agents/plans/2026-09-16-agentic-first-roi-top-5.md`.

Selection rule: change appears at high severity (6+/10) in 2+ new POVs AND is *not* already in the v1 top-5. Ties broken by effort (smaller first) and how cleanly it composes with v1 picks.

---

## #1 Agent onboarding: stdio MCP transport + paste-ready `.mcp.json` snippets

POV: distribution (POV 5, severity 8, effort M); distribution (POV 14 — MCP `instructions` rewrite, severity 7, effort XS); privacy (POV 2 — provider leak transparency, severity 4, ease M).
Effort: M. ROI: conversion-funnel.

Problem. `oxe/__main__.py:21-38` runs uvicorn on `127.0.0.1:4479` with no flag for stdio MCP. The only MCP transport is HTTP at `/mcp/` (`oxe/server.py:177`). Claude Code's `~/.claude/.mcp.json` and Cursor's `~/.cursor/mcp.json` accept both, but the common patterns are:
- `{"command": "uvx", "args": ["oxe", "--mcp"], "transport": "stdio"}` — spawn-on-demand
- `{"url": "http://127.0.0.1:4479/mcp/"}` — daemon URL

oxe supports only the second. First-time Claude Code users hit a README that talks about MCP but never shows the JSON to paste. The `mcp_server.py:21-28` `instructions` string reads as a payload contract, not a value-prop. Result: 6-line copy change + stdio transport = the install → daily-use moment.

What to do.
- Add a `stdio` mode to `oxe/__main__.py`: detect `--mcp` or `--transport=stdio`, skip the HTTP server, call `mcp_server.mcp.run(transport="stdio")`. The `mcp` Python lib exposes `MCPServer.run(transport="stdio")` directly — ~30 lines.
- Rewrite `oxe/mcp_server.py:21-28` `instructions` to lead with the value: "Free local Exa.ai-compatible web search backed by DuckDuckGo + cached. Same response shape as Exa's `/search` endpoint, no API key. Call `exa_user_history` BEFORE `exa_search` to discover what the user has already read in the web UI." Then list tools.
- Add a README "MCP clients" section with three paste targets (Claude Code, Cursor, maki/Hermes), and note that `oxe --mcp` works zero-config for stdio clients while `OXE_PORT=4479 oxe` keeps the daemon URL path alive.
- Add `OXE_AI_PROVIDER_LABEL` to `config.py` and surface in the UI's first AI-mode dialog: "AI sends queries + click history to provider X (opt out: `OXE_AI_DISABLE_HISTORY_TOOL=1`)".

Why this ROI: distribution POV-5 labels it the install-to-daily-use moment; security POV-2 wants the same transparency on first AI enable. Together they're the "what does Claude Code actually see when I install oxe" answer.

---

## #2 Cache-file + config-file permissions chmod 700/600 on create

POV: privacy (POV 5+9, severity 5 / 5, both effort S) — they were the top two in that POV's roll-up; security-relevant anytime someone shares a dev host.
Effort: S. ROI: very high for shared-host scenarios.

Problem. `oxe/cache.py:23-30` and `oxe/config.py:80-85` create `~/.cache/oxe/cache.db` and `~/.config/oxe/config.toml` with whatever umask the user's shell hands them (typically `0644`). Anyone on a shared dev box can:
- `sqlite3 ~/.cache/oxe/cache.db "SELECT query_text FROM search_log"` → 30 days of queries
- `cat ~/.config/oxe/config.toml` → literal API key if the user stored one instead of using `{env.NAME}` template
- read every AI answer via the `answers` table

The threat model isn't paranoia; CI runners, agent dev boxes, and shared homes are realistic. Locking down the cache is one of those "missed on day one, painful to retrofit" things.

What to do.
- `oxe/cache.py:23-30`: after `parent.mkdir`, `os.chmod(parent, 0o700)`; after creating the sqlite file, `os.chmod(self.db_path, 0o600)`. Use `os.open(path, O_CREAT | O_WRONLY, 0o600)` when possible to avoid the race.
- `oxe/config.py:80-85`: same on the config dir + file (`chmod 0o700` on dir, `0o600` on file).
- Reset existing permissions when the user updates: in `cache.put` and `config.save_config`, idempotent `os.chmod` if the existing perms are wider than 0o600.
- Document in `AGENTS.md`: "Cache lives at `$OXE_CACHE_DIR/cache.db`, always owner-only (chmod 600)."

Why S effort: 4 file edits, no schema, no behavior change. Why high ROI: it's the single security hardener that addresses 30 days of accumulated queries & keys, and it's defensible even in solo use (catches the "logs leaked in a screenshot" path).

---

## #3 Search-result freshness + snippet sufficiency (replace DDG link-anchor snippet with extracted meta-description)

POV: search-quality (POV 1 severity 8 + POV 3 severity 8), engineering (POV 13 token efficiency), privacy (POV 15 phishing flags).
Effort: M. ROI: very high — the single biggest "trust" axis.

Problem. Three compounding issues, all rooted in `_dgr_to_exa` at `oxe/exa_compat.py:67-84`:
1. `body` is DDG's link-anchor snippet (~150-400 chars, often starts with "By clicking submit you agree...") — not even a `meta description`.
2. `publishedDate` is hardcoded `None` — there's no freshness signal even when the underlying page has one.
3. `startPublishedDate`/`endPublishedDate` are silently dropped.

For deep-research answers the LLM is hallucinating from junk. For news the user can't tell what's fresh. Query side: queries with "latest/today/breaking" get no time-bias; long queries get first-3-sentences-blindly highlights (`_extract_highlights` line 53). 

This is different from v1's #1 (full `web_fetch` for answers) — v1's #1 is per-page deep-extraction when the agent decides to fetch. THIS fix is per-result automatic enrichment on every `/search`. Composable: v1's fetcher piggybacks on the cache that this creates.

What to do.
- `oxe/exa_compat.py:67-84`: when extending `_dgr_to_exa`, run a parallel `httpx.AsyncClient().head(url, timeout=2)` for the top 5 results, parse `<meta property="article:published_time">` / `<meta name="pubdate">` / `<time datetime>` / JSON-LD `datePublished`; populate `publishedDate` when found, leave `None` when not (don't lie).
- New `oxe/snippet.py` (or fold into `exa_compat`) with `extract_meta(html) -> str`: pulls `<meta name="description">` or first `<p>` content, returns ~300 chars. Replaces DDG `body` with this cleaner snippet.
- Time-sensitivity heuristic in `build_query` (or new `oxe/query.py`): regex `\b(latest|today|breaking|this week|yesterday|20\d{2}|news)\b` → `timelimit="w"` or `"d"` automatically.
- Honor `startPublishedDate`/`endPublishedDate`: map to DDG's `df` range via the existing `timelimit` channel; cache key includes the dates so we don't serve stale filtered results when the filter changes.
- `_extract_highlights` (line 53-57): score sentences by query-term overlap (TF-IDF-lite), not first-3-blindly; set `highlightScores` to real scores; add `_snippet_origin: "first3"|"scored"` for debug.

Why M effort: parallel HEAD fetches + a small fetch helper (use the same `oxe/fetch.py` planned in v1 #1 to share infra). Why very high ROI: search-quality POV-1+3 explicitly called this out as the "trust axis"; v1 #1 (full-page fetch for answers) depends on the per-URL metadata cache this builds; fixes #1 of Perplexity POV too (recency chips).

---

## #4 Observability: AI good-rate logging + answer_log table + provider-error counts

POV: observability (POV 5 severity 8, severity 9 for the "cache poisoning via low-confidence answer" line; POV 7 severity 7 — per-step latency missing; POV 12 severity 6 — no SSE-transcript test); engineering (POV 4 — answer cache invalidation drift).
Effort: S-M. ROI: answers-fail-silently-today.

Problem. `oxe/ai.py` parses confidence from the SSE `done` event, but `confidence` is never persisted anywhere. With `CACHE_MIN_CONFIDENCE=4` (`ai.py:29`), a regressing model that emits confidence=4 answers will **silently poison the answer cache for 24h** (`ANSWER_TTL_DEFAULT=86400`). The maintainer has no way to detect this; users get worsening answers but all dashboards say "ok".

Companion gaps:
- `_friendly_provider_error` (`ai.py:464-480`) maps 401/404/429/402/timeout to short strings, then **discards the original error class**.
- Per-step tool-call latency inside the ReAct loop is never collected (operator sees one total wall-clock for an answer).
- `/api/stats` aggregator (`stats.py`) has a 400-line payload but the SPA dashboard at `ui/src/routes/dashboard.tsx` shows hardcoded "no search log data" placeholders — there's no `apiStats()` helper in `ui/src/lib/api.ts:144`.

What to do.
- New `oxe/sql/answers_log.sql` table: `(ts, query_text, query_hash, model, confidence, n_steps, duration_ms, error_class, client)`. Migration via `oxe/sql/schema.sql`. One row per `done` event from `/answer`.
- `oxe/cache.py::TTLCache.log_answer(...)` — append helper.
- `oxe/ai.py::stream_answer` (line 396-445): after the terminal yield, call `c.log_answer(...)`. In the cached path (`server.py:474-489`), same.
- In `_friendly_provider_error`'s call sites (`ai.py:288-298`, caught path), derive `error_class` from the regex (e.g. `re.search(r'\b(401|404|429|402|timeout)\b', str(e).lower())`), include in the dev event but NOT in the SSE user message.
- Emit `_dev_event("ai.tool_call", tool=..., query=..., duration_ms=..., results=N)` around each tool dispatch in `ai.py:390-410`.
- `ui/src/lib/api.ts:144`: add `export async function apiStats()` hitting `GET /api/stats`.
- `ui/src/routes/dashboard.tsx:25-43`: rewrite four panels to consume `apiStats()` payload; add 30s polling and "last refreshed" stamp.
- Add `stat-backend-split` to `oxe/sql/search_log.sql` (`stat-backend-split(cutoff)` GROUP BY `backend`).

Why S-M effort: the answers_log table is one migration + one helper + one call; the rest is plumbing. Why high ROI: closes the "answers are flaky today — how do I see it?" loop observability POV flagged at severity 9 (cache poisoning via low-confidence answer).

---

## #5 Search-result ranking improvements (source-authority re-rank + dedupe + technical-query bias) + cache-poisoning guards

POV: search-quality (POV 2 severity 7, POV 13 severity 7, POV 10 severity 6); Perplexity POV-9 (result diversity severity 7).
Effort: M. ROI: directly addresses "agent cites SEO spam" complaints.

Problem. DDG html backend returns Bing's index in their order — for "asyncio.gather example" the top hit is rarely `docs.python.org`; for "Python regex email match" the top hits are SEO blogs. `_dgr_to_exa` (`exa_compat.py:151`) consumes results verbatim with no re-ranking, no dedupe, no source authority bias. The Perplexity POV-9 finding (multi-engine blends) addresses breadth; THIS addresses the within-result-list quality.

Worse: cache poisoning. SEO/spam/phishing URLs that DDG surfaces get cached for 1h (`OXE_TTL_DEFAULT=3600`) and re-served. An agent calling `exa_search` 30 minutes later with the same query will cite the same malicious URL.

What to do.
- New `oxe/rerank.py` (post-processor invoked from `exa_compat.search` line 151):
  - Static authority tier dict: `netloc → tier`. Tier 0 (boost, +N positions): `github.com`, `stackoverflow.com`, `docs.python.org`, `developer.mozilla.org`, `arxiv.org`, `*.wikipedia.org`, `*.gov`, `acm.org`, `ieee.org`, `nature.com`, `sciencedirect.com`. Tier 2 (demote, -N positions): known SEO patterns (substring match on `best-`, `-review-`, `top-10-`, `medium.com/?source=`).
  - Near-dup dedupe: normalize URL host (`urlparse(...).netloc.lower().lstrip("www.")`), strip tracking params (`utm_*`, `fbclid`, `gclid`), merge duplicate `reg-domain + path-prefix` entries keeping the first-ranked.
  - Query-class bias: if `re.search(r"\b(regex|equivalent of|how to (use|implement|write)|example|api|tutorial)\b", q)`, prepend `OR (site:github.com OR site:stackoverflow.com OR site:docs.python.org OR site:developer.mozilla.org)` to the DDG query.
- Cache-poisoning guard (`oxe/cache.py::set`, line 54-66): on write, drop any URL whose registrable domain matches a small static `data/blocklist.txt` (Spamhaus-style curated, ~30 entries to start), and enforce a `max 30% same registrable domain` floor across the result list.
- New `oxe/data/trusted_domains.txt` + `oxe/data/blocklist.txt`, both shipped (small).

Why M effort: ~250 lines for the rerank + dedupe, plus the data files. No new deps. Why high ROI: closes the "agent cites SEO spam" complaint (search POV-2), aligns with Perplexity-quality recall, and the poisoning guard addresses a real agent-safety concern.

---

## What's NOT on this list (close-but-deferred)

| Candidate | Why deferred |
|---|---|
| DNS-rebinding protection on + trusted-host middleware (privacy POV 3, severity 6, effort M) | Important, but unimportant while binding 127.0.0.1 only — ship when LAN-bind becomes a thing. Add to v0.6. |
| Auto-bearer-token for mutating endpoints (privacy POV 4, severity 5, effort M) | Important for shared-host; one-liner for 127.0.0.1 users adds friction. Same v0.6 window. |
| OpenAI vision / multimodal input (Perplexity POV 8) | Already deferred from v1. |
| `OXE_BIND=0.0.0.0` README drift fix (privacy POV 13) | One-line doc fix; trivially folded into README update for #1. |
| CLI surface (`oxe doctor`, `oxe cache wipe`, `oxe install-mcp`) (distribution POV 9, severity 4) | Real wins; bundled into "MCP clients section + stdio" PR for #1. |
| `ddgs` upper-bound pin in `pyproject.toml` (privacy POV 11, severity 4, effort S) | Two-character change (`<10`); fold into the next pyproject bump. |
| Settings UI: align `schema.ts:14` provider list with backend `config.py:31` (OpenRouter/Ollama) (distribution POV 6, severity 5, effort S) | Three-line PR; ship alongside stdio MCP work in #1. |
| `startPublishedDate` / `endPublishedDate` honoring (search POV 3) | Folded into #3; same PR. |
| WAL/vacuum/diagnostics endpoint (observability POV 10) | S effort; ship alongside #4. |
| Self-test endpoint `/health?self_test=1` (observability POV 16, severity 5, effort M) | Folded into the same observability PR as #4 (`/ready` + `/live` split + self-test). |
| Per-step tool-call latency in dev events (observability POV 7, severity 6, effort S) | Folded into #4. |
| `--version`, startup banner (distribution POV 9) | Two-liner; ships with #1. |

## Suggested sequencing for v0.5 — combined v1 + v2

1. **PR 1 (v1 #1)**: page-content fetcher (`oxe/fetch.py` + `trafilatura` extra + `web_fetch` tool + `exa_get_contents` MCP).
2. **PR 2 (v2 #2)**: chmod 600/700 cache and config dirs — one tiny security PR between bigger features.
3. **PR 3 (v2 #3)**: meta-description snippet + freshness metadata + `startPublishedDate` honoring. Builds the per-URL cache that PR 1's fetcher reuses.
4. **PR 4 (v2 #4)**: `answers_log` table + provider-error class + per-step latency events + dashboard wiring.
5. **PR 5 (v2 #1)**: stdio MCP transport + paste-ready `.mcp.json` snippets + rewritten `instructions` string.
6. **PR 6 (v1 #3)**: empty-results-vs-error plumbed through UI (smoke PR).
7. **PR 7 (v1 #2)**: parallel tool loop + default-fallback backend.
8. **PR 8 (v2 #5)**: rerank + dedupe + technical-query bias + cache-poisoning guard.
9. **PR 9 (v1 #5)**: MCP `exa_answer` + URL normalization + cached-`/answer` SSE delta fix. (Depends on PR 5.)
10. **PR 10 (v1 #4)**: History URL state + copy/regenerate in Answer view (UI polish).

## Net effect at v0.5.0 (combined v1 + v2)

oxe goes from "search proxy with a single-shot ReAct loop" to:
- A foundation that returns **actual page content** (not snippets) to both `/answer` and MCP tools.
- A search layer that surfaces **fresh, deduplicated, authority-ranked** results with **freshness metadata**.
- An **installable agent tool** (one-paste stdio MCP) the same way Exa / Tavily / Jina MCPs are.
- A **self-debugging service** (answers-log, provider-error class, dashboard rendered live) so the maintainer sees "answers are flaky today" in stats before the user tells them.
- A **trust** axis hardened (cache perms, blocklist, dedupe, technical bias) so an agent citing oxe isn't citing SEO spam.
- A **security baseline** (file perms, transparent provider disclosure) that's defensible even when oxe accidentally binds wider than 127.0.0.1.

## Subagent reports (round 2, ~400 lines each)

- Distribution & adoption: 15 POVs from "first 90 seconds" through CHANGELOG hygiene. Headline finds: stdio MCP missing (severity 8), MCP `instructions` is dry contract (severity 7), README lacks `.mcp.json` snippets (severity 6).
- Privacy & security: 16 POVs enumerating every outbound destination + every persistence path. Headline finds: chmod missing (severity 5), DNS rebinding off (severity 6), `ddgs>=` open-ended (severity 4).
- Search quality & reliability: 18 POVs from snippet sufficiency through provenance. Headline finds: snippet is DDG link-anchor junk (severity 8), freshness is null (severity 8), ranking has no authority layer (severity 7), poisoning guard absent (severity 7).
- Observability & ops: 16 POVs from event format through self-test endpoints. Headline finds: confidence never logged → silent cache poisoning (severity 9), `_dev_event("answer")` discards per-step latency (severity 7), dashboard route dead (severity 5).
