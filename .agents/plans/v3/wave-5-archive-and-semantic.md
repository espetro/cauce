# W5: archive

Iteration 6 (2026-12-01 to 2026-12-14). Priority P2. EPIC issue: #6.
Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`. Previous: `wave-4-ai-mode.md`.

## Goal

The Hister-like dimension: pages a human or agent actually opened become a local, searchable
archive (`fetch_and_index`, `search_archive`).

## Settled inputs

- Tables `pages(url PK, fetched_at, title, markdown, byte_len, source_query_hash?)` and
  `pages_fts` (FTS5 external content); one writer each (`fetch_and_index`).
- Readability: `readability-rs` or `dom_smoothie` (pick at W5-01 by fixture quality on 10
  saved pages), then HTML to markdown; size cap 1 MB fetched, 200 KB markdown stored.
- MCP tools: `search_archive(query, limit?)` (hybrid RRF over `pages_fts` + `cache_fts`),
  `fetch_and_index(url)`. Agents get markdown back.

## Exit criteria

1. Clicking a result from the UI, or an agent calling `fetch_and_index`, yields a `pages`
   row and `search_archive` finds it by a phrase from its body.

## Steps

### W5-01 `fetch_and_index`: readability to markdown
- Issue #55 · Effort M · Label feature · Team Systems · Branch `v3/w5-01-fetch-index`
- Depends on: W4-05
- Do: `oxe-core::archive`: fetch through the shared `HttpClient` (same politeness, own
  token bucket keyed by host), readability extraction, markdown conversion, `pages` write,
  `pages_fts` trigger; `POST /api/pages` (url) and `GET /api/pages/{url}`; UI click beacon
  optionally triggers indexing (`archive.index_on_click`, default true); MCP
  `fetch_and_index`. Ten fixture HTML pages with expected markdown snapshots.
- Acceptance: fixtures produce markdown within 5 % of the snapshot length and containing
  the sampled phrases; a 3 MB page is capped and logged.
- Follow-up: W5-02.

### W5-02 Pages FTS and archive page
- Issue #56 · Effort S · Label feature · Team Product Builders · Branch `v3/w5-02-archive-page`
- Depends on: W5-01
- Do: `/archive` page: search box over `pages_fts` with snippets and highlights, open
  markdown view, delete (audited); `GET /api/archive?q=`.
- Acceptance: page test indexes a fixture and finds it by body phrase.
- Follow-up: W5-03.

### W5-03 MCP `search_archive`
- Issue #57 · Effort S · Label feature · Team Product Builders · Branch `v3/w5-03-search-archive`
- Depends on: W5-02
- Do: hybrid RRF over `pages_fts` and `cache_fts` (cached result snippets); returns `{url,
  title, snippet, source: page|cached_result, score}` plus
  `request_id`; wired as a tool for the W4 answer loop.
- Acceptance: rmcp client test finds an indexed page and a cached result for the same
  phrase, ranked by RRF.
- Follow-up: W6-01.

## Out of scope for W5

Browser extension, full-site crawling, chunk-level answer grounding beyond top snippets,
semantic tier (`later/semantic-tier.md`), `sqlite-vec` ANN (follow-up in `later/`).

## Follow-up

W6 `wave-6-postgres-and-multi-instance.md`.
