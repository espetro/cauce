# W5: archive and semantic tier

Iteration 6 (2026-12-01 to 2026-12-14). Priority P2. EPIC issue: #6.
Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`. Previous: `wave-4-ai-mode.md`.

## Goal

The Hister-like dimension: pages a human or agent actually opened become a local, searchable
archive (`fetch_and_index`, `search_archive`), and the optional semantic tier makes
paraphrased queries hit the cache. Both are features, off by default where they cost memory.

## Settled inputs

- Tables `pages(url PK, fetched_at, title, markdown, byte_len, source_query_hash?)` and
  `pages_fts` (FTS5 external content), `cache_vec(key PK, embedding BLOB, dim)`; one writer
  each (`fetch_and_index`, `Pipeline::persist` when semantic enabled).
- Readability: `readability-rs` or `dom_smoothie` (pick at W5-01 by fixture quality on 10
  saved pages), then HTML to markdown; size cap 1 MB fetched, 200 KB markdown stored.
- Embedder: `fastembed` with an INT8 small model (`bge-small-en-v1.5` quantised or
  `all-MiniLM-L6-v2`), cargo feature `semantic` (default on for the build, runtime
  `cache.semantic.enabled` default false); loads only if available memory > 512 MB at
  startup, else logs and stays lexical; model file cached under `$OXE_DATA_DIR/models/`.
- Tier 3 lookup: brute-force cosine over `cache_vec` for < 50k rows (single-digit ms in
  Rust), `sqlite-vec` static build as the follow-up when a deployment exceeds that; hit
  threshold cosine >= 0.92, same page/lang.
- MCP tools: `search_archive(query, limit?)` (hybrid RRF over `pages_fts` + `cache_fts` +
  vectors when enabled), `fetch_and_index(url)`. Agents get markdown back.

## Exit criteria

1. Clicking a result from the UI, or an agent calling `fetch_and_index`, yields a `pages`
   row and `search_archive` finds it by a phrase from its body.
2. With semantic enabled, `ffmpeg convert m4a to mp3` hits the cache row of
   `convert m4a to mp3 with ffmpeg` as `tier:3`.
3. RSS with the embedder loaded stays under 80 MB idle on the owner's machine (measured in
   the nightly budget test, `semantic` on).

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
- Do: hybrid RRF over `pages_fts`, `cache_fts` (cached result snippets) and, when enabled,
  vectors; returns `{url, title, snippet, source: page|cached_result, score}` plus
  `request_id`; wired as a tool for the W4 answer loop.
- Acceptance: rmcp client test finds an indexed page and a cached result for the same
  phrase, ranked by RRF.
- Follow-up: W5-04.

### W5-04 Embedder feature and resource guard
- Issue #58 · Effort M · Label feature · Team Systems · Branch `v3/w5-04-embedder`
- Depends on: W0-11, W5-03
- Do: `oxe-core::embed` behind `semantic`: `Embedder` trait with `FastEmbed` impl, lazy
  model download with checksum, `Resources`-gated load, `oxe embed "text"` debug command;
  metrics `oxe_embed_duration_ms`; budget test variant with `semantic` on.
- Acceptance: unit test embeds two paraphrases with cosine > 0.9 and two unrelated strings
  < 0.5; startup with a fake 256 MB free memory reading skips loading and logs why.
- Follow-up: W5-05.

### W5-05 Tier-3 semantic cache and hybrid RRF
- Issue #59 · Effort M · Label feature · Team Systems · Branch `v3/w5-05-semantic-tier`
- Depends on: W5-04
- Do: `Store::get_semantic(embedding, threshold)` and `put_embedding`; pipeline computes the
  query embedding only on tier-1 and tier-2 miss; `Source::Cache{tier:3, matched_query}`;
  `search_archive` uses the same embeddings for pages (chunked by heading, 512 tokens);
  settings toggle and threshold.
- Acceptance: exit criterion 2 as a test with the real small model (marked `#[ignore]`
  unless `OXE_MODEL_TESTS=1`, cached model) plus a mocked-embedder variant in the fast gate.
- Follow-up: W6-01.

## Out of scope for W5

Browser extension, full-site crawling, chunk-level answer grounding beyond top snippets,
`sqlite-vec` ANN (follow-up in `later/`).

## Follow-up

W6 `wave-6-postgres-and-multi-instance.md`.
