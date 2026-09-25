# W5-03 search_archive: hybrid RRF over pages_fts + cache_fts (2026-09-25)

Findings from implementing issue #57 (`v3/w5-03-search-archive`).

## `cache_fts` title/snippet columns are INDEX-ONLY — decode `payload_json`

`cache_fts` is FTS5 external-content; its `titles`/`snippets` columns are
populated by triggers (`group_concat(json_extract(...))`) and can be
MATCHed but never SELECTed or `snippet()`'d — reads return opaque blobs.
`Store::search_cache_fts` therefore does entry-level `cache_fts MATCH`
for candidates + bm25 rank, joins `cache_entries`, decodes `payload_json`
via `rows::cached`, then filters each result with a per-result
token-cover check (every query token present in that result's
title+snippet token set, unicode61-equivalent). Consequences worth
remembering:

- Query-column-only matches emit zero hits (the query column isn't
  covered by per-result cover) — pinned by the `cache_fts_search`
  conformance check.
- Hits from one entry share the entry's bm25 rank and keep result order.
- Expired rows are still indexed (same pinned stale-serve semantic as
  `get_lexical`). W6 Postgres must mirror all of this.

## RRF fusion of two local lists

`SearchPipeline::search_archive` adapts each list to `SearchResult`,
carrying `EngineId::from("page")`/`"cached_result"` as the source marker
through `RrfMerge` (EngineId's `[A-Za-z0-9._-]+` charset makes this
free). Dedupe is on `normalize_url` — a URL both indexed and cached is
one hit boosted by both lists, and `source` reports the best-ranked
contribution (`page` wins ties because it's added as list 0). Equal
weights `1.0`: local stores have no reliability history to weigh; `k`
comes from `merge.rrf_k`, host collapse from
`merge.collapse_same_host_after`.

## MCP tool surface is pinned in two places

`TOOL_NAMES` consts exist in `crates/cauce-server/tests/mcp.rs` AND
`crates/cauce-cli/tests/e2e/mcp_stdio.rs` — a new tool must be added to
both or the stdio e2e fails. `archive`-gated tools live in the separate
`archive_tool_router` merged in `CauceMcp::new`.

## Answer loop: `search_archive` is advertised unconditionally

`AnswerLoop`'s tool list is `[search_web_spec(), search_archive_spec()]`
on every build — `SearchPipeline::search_archive` compiles without the
`archive` feature because `pages`/`cache_fts` exist on every migration
(only the fetch/extract path is feature-gated). `limit` is clamped to
`TOOL_RESULT_LIMIT` (10) so the model can't widen its own source pool;
hits join sources labeled `engine: "archive"`.
