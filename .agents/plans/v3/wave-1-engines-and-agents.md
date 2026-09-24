# W1: real engines and agent surfaces

Iteration 2 (2026-10-06 to 2026-10-19). Priority P0. EPIC issue: #2.
Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`. Previous: `wave-0-skeleton.md`.

## Goal

Bing and Brave as native declarative engines with fixtures, engine health and admission
control so parallel agents on one IP do not get it blocked, MCP over streamable HTTP and
stdio, tier-2 lexical cache, a metrics endpoint, and the cutover of the owner's agent wiring
(`~/SEARCH.md`, oxmgr app `oxe` on 4479) from v1 to v3.

## Settled inputs

- Everything in W0's settled inputs.
- Engine YAML schema, exec protocol, replay format: parent 4.3.
- Politeness: per-engine token bucket 1 req/s burst 3 (config), one fixed realistic UA per
  engine, no UA rotation, no proxy pool. `Egress` trait with `Direct` and `StaticProxy(url)`.
- Breaker: `RateLimited`/`Blocked` open 15 min; 3 consecutive `Timeout` open 5 min;
  HalfOpen lets one probe through. EWMA alpha 0.3. Persisted in `engine_health`.
- Admission: singleflight on `CacheKey`; bounded per-engine queue with wait budget
  (`admission.max_wait_ms`, default 1500); on overflow serve stale if any, else HTTP 429 with
  `Retry-After` and MCP error `rate_limited` with `retry_after_s`.
- MCP tool names and argument shapes: `search_web(query, page?, engines?, lang?, ttl_s?)`,
  `cache_status()`, `cache_invalidate(key? | expired? | all?)`, `exa_search(query,
  num_results?, type?, source?, exclude_domains?, category?)` returning the Exa result shape
  from v2 (`v2-legacy:oxe/api/exa.py` is the reference; typed, frozen). Every tool result
  carries `request_id`.
- Metrics: owned in-process registry in `oxe-core::metrics` (counters, gauges, HDR
  histograms per engine, ~150 LoC) rendered as Prometheus text on `GET /metrics`
  (loopback, no auth); `/api/stats` computes the same numbers. OTLP export of
  traces+metrics stays opt-in via `tracing-opentelemetry` + `opentelemetry-otlp` behind
  the `otlp` cargo feature, which is non-default. Metric names in the step. If
  pull-metrics scope grows, revisit `metrics-exporter-prometheus`.
- Modes: `oxe serve` (full), `oxe serve --headless` (no templates/pages), `oxe mcp` (stdio,
  no listener). One binary. Cargo features `ui`, `mcp`, `ai`, `archive`, `semantic`,
  `postgres`, `otlp`; defaults `ui mcp ai` (`otlp` non-default).

## Exit criteria

1. `OXE_LIVE=1` smoke green for Bing, Brave, Wikipedia (3 queries each).
2. Claude Code and Hermes reach `exa_search` and `search_web` on `https://search.localhost/mcp`
   served by the v3 binary; `~/SEARCH.md` updated.
3. Two agents hammering the same query concurrently produce one upstream call (singleflight
   asserted by a test with a counting replay engine).
4. `/metrics` exposes per-engine latency histograms and cache hit counters; `/api/stats`
   renders the same numbers.

## Steps

### W1-01 HttpClient, Egress trait, token bucket, UA policy
- Issue #21 · Effort M · Label feature · Team Systems · Branch `v3/w1-01-http-egress`
- Depends on: W0-12
- Do: `oxe-core::http`: `HttpClient` wrapping `reqwest` (rustls, HTTP/2, connection pool,
  per-request timeout from the engine budget), `Egress` trait (`Direct`, `StaticProxy`),
  per-engine `governor` token bucket, per-engine fixed UA and `Accept-Language`, response
  size cap (2 MB), redirect cap 3, `tracing` span per upstream call with status/bytes/ms.
  Config `[engines.<id>.egress] proxy = "socks5://..."`. If reqwest's TLS fingerprint
  becomes the blocker on Brave/Bing, the named fallback HTTP client is `rquest` (TLS
  impersonation), swapped in behind `HttpClient`.
- Acceptance: unit test with a local `wiremock` server asserts the bucket delays the 4th
  burst call, the proxy setting is honoured (mock proxy sees the request), and the size cap
  aborts a 3 MB body with `EngineError::Parse`.
- Follow-up: W1-02.

### W1-02 Declarative engine runtime and `oxe engine test`
- Issue #22 · Effort L · Label feature · Team Systems · Branch `v3/w1-02-declarative-runtime`
- Depends on: W1-01
- Do: `oxe-engines::declarative`: parse the YAML schema (parent 4.3) with `serde_norway`
  (maintained drop-in fork of `serde_yaml`) into a validated `EngineSpec` (compile CSS
  selectors once with `scraper`); `request`
  templating (`{q}` url-encoded, `{page}`, `{page0}`, `{offset}` = (page-1)*page_size,
  `{lang}`); `parse.kind: html | json` (json uses a small JSONPath subset via
  `serde_json_path`); field extraction `css`/`attr`/`text`/`regex`; `detect.blocked`
  substrings and `rate_limited_status` map to `EngineError`; relative URL resolution;
  tracking-redirect unwrapping (`bing.com/ck/a?...&u=a1<base64>` and `r.search.yahoo.com`
  patterns as a `unwrap_redirect` list in the spec). `request.headers` values may contain
  `${env:NAME}`/`${file:PATH}` templates resolved through the config interpolator (needed
  for keyed-API specs later). `oxe engine test <spec.yaml>` runs
  every fixture pair in `engines/fixtures/<id>/*.html` + `.expected.json` and diffs;
  `--live "<query>"` fetches once, prints parsed results, and `--record` writes a new fixture
  pair. Loading: specs embedded from `engines/*.yaml` at build (`include_dir`) and
  overridable from `$OXE_CONFIG_DIR/engines/*.yaml` at runtime.
- Acceptance: a synthetic spec + HTML fixture in the crate tests parses 10 results; a
  fixture whose selectors match nothing yields `EngineError::Parse("0 results, selector
  ...")` not an empty Ok; blocked-substring fixture yields `Blocked`.
- Follow-up: W1-03, W1-04, W1-05 (parallel).
- Amended 2026-09-24 (issue #110): the schema gains an optional `request.market` map
  (`lang -> market` codes) feeding a new `{market}` template token — e.g.
  `mkt={market}` on `bing.yaml`, replacing the pinned `mkt=en-US` that sent `lang=fr`
  queries to the en-US market. Resolution is deterministic: exact lang
  (case-insensitive, per BCP-47), then `-x`
  subtags stripped (`en-GB` -> `en`), then a key the lang is a prefix of (`pt` ->
  `pt-BR`), then the reserved `default` key, else the first map entry; a spec using
  `{market}` without `market:` fails compilation. A template filter was considered and
  rejected: market vocabularies are engine-specific (Bing `mkt`, Brave `country`,
  Yahoo `vl`), so the data belongs in the spec, not the runtime.

### W1-03 `bing.yaml` with fixtures
- Issue #23 · Effort S · Label feature · Team Systems · Branch `v3/w1-03-bing`
- Depends on: W1-02
- Do: tier 1, `https://www.bing.com/search?q={q}&first={offset+1}`, results `li.b_algo`,
  title `h2 a`, url `h2 a@href` (unwrap `bing.com/ck/a` redirect), snippet `.b_caption p`
  or `p`; detect `429`, `captcha`. Record two fixtures with `oxe engine test --record` and
  commit them; Apache-2.0 header.
- Acceptance: `oxe engine test engines/bing.yaml` green on fixtures; `--live` returns >= 5
  results for `tanstack router docs`.
- Follow-up: W1-06, W1-11.

### W1-04 `brave.yaml` with fixtures
- Issue #24 · Effort S · Label feature · Team Systems · Branch `v3/w1-04-brave`
- Depends on: W1-02
- Do: tier 1, `https://search.brave.com/search?q={q}&offset={page0}&source=web`, results
  `div.snippet[data-type="web"]` (verify against a live fetch at implementation time and
  adjust; the fixture is the truth), title/url/snippet selectors, detect `429`, `Sorry`
  block page. Two fixtures.
- Acceptance: as W1-03.
- Follow-up: W1-06, W1-11.

### W1-05 `wikipedia.yaml` with fixtures
- Issue #25 · Effort S · Label feature · Team Systems · Branch `v3/w1-05-wikipedia`
- Depends on: W1-02
- Do: tier 3, `enabled: false` by default, `parse.kind: json` over the OpenSearch API
  (`https://{lang}.wikipedia.org/w/api.php?action=opensearch&search={q}&limit=10&format=json`),
  fields from the parallel arrays. Documented in the SDK README and MCP tool description as
  the engine to pin (`engines=["wikipedia"]`) or `replay` while iterating, because it is
  keyless and does not rate-limit like web engines.
- Acceptance: fixtures green; `OXE_LIVE=1` returns results without any key.
- Follow-up: W1-11.

### W1-06 Engine health: EWMA and circuit breaker, persisted
- Issue #26 · Effort M · Label feature · Team Systems · Branch `v3/w1-06-engine-health`
- Depends on: W0-12
- Do: `oxe-core::health`: `EngineHealth` per engine (EWMA latency, consecutive failures,
  breaker state/until, last_ok, last_error), updated after every call, persisted via
  `Store::put_health` (debounced 1/s), loaded at startup; pipeline skips `Open` engines,
  probes `HalfOpen` with one call; `GET /api/engines` (list with health) and
  `POST /api/engines/{id}/reset` (audited) added to the routes table.
- Acceptance: replay engine with `blocked=true` opens after one call and is skipped for the
  next; after the window it is probed once; restart of the test binary keeps the `Open`
  state (read from the temp DB).
- Follow-up: W2-05, W3-01.

### W1-07 Admission: singleflight, bounded queue, stale/429
- Issue #27 · Effort M · Label feature · Team Systems · Branch `v3/w1-07-admission`
- Depends on: W0-12
- Do: `oxe-core::admission`: in-flight map keyed by `CacheKey` so concurrent identical
  requests await one upstream future (`tokio::sync::watch`/`Shared`); per-engine bounded
  `Semaphore` + FIFO wait with `max_wait_ms`; on overflow: stale row if present
  (`Source::Cache{stale:true}` + background refresh enqueued) else `PipelineError::
  RateLimited{retry_after_s}` -> HTTP 429 + `Retry-After`, MCP error with `retry_after_s`.
  Metrics hooks (counters) left as `tracing` events until W1-09.
- Acceptance: 20 concurrent identical requests against a counting replay engine produce 1
  upstream call and 20 responses; with `max_wait_ms=1` and a slow engine the 4th request
  gets 429 with `Retry-After`; with a stale row present it gets the stale row instead.
- Follow-up: `later/per-client-fairness.md`.

### W1-08 MCP server: streamable HTTP and stdio
- Issue #28 · Effort L · Label feature · Team Product Builders · Branch `v3/w1-08-mcp`
- Depends on: W0-09
- Do: `oxe-server::mcp` with `rmcp` (`transport-streamable-http-server`, `transport-io`):
  tools `search_web`, `cache_status`, `cache_invalidate`, `exa_search` (frozen Exa result
  shape; `source='history'` short-circuit from v2 maps to `clicks`), each returning
  `request_id`; server instructions mention `replay`/`wikipedia` for iteration; MCP client
  name captured into `ClientKind::Mcp(name)`; mounted at `/mcp` in the routes table;
  `oxe mcp` runs the stdio transport against the same DB with no HTTP listener.
- Acceptance: integration test with the `rmcp` client over HTTP lists 4 tools, calls
  `search_web` on replay and gets results with `request_id`; stdio test spawns `oxe mcp` and
  does the same; `exa_search` result validates against the frozen Exa JSON schema copied from
  `v2-legacy` (`tests/fixtures/exa_schema.json`).
- Follow-up: W1-11, W1-12.

### W1-09 Metrics: owned registry, `/metrics`, `/api/stats` extension
- Issue #29 · Effort M · Label feature · Team Systems · Branch `v3/w1-09-metrics`
- Depends on: W0-05
- Do: `oxe-core::metrics` as an owned in-process registry (counters, gauges, HDR
  histograms per engine, ~150 LoC); instruments:
  `oxe_search_requests_total{client,source,tier}`, `oxe_search_duration_ms` histogram
  `{source}`, `oxe_ttfr_ms` (time to first engine result), `oxe_engine_requests_total
  {engine,outcome}`, `oxe_engine_duration_ms{engine,phase=http|parse}`,
  `oxe_engine_results{engine}` histogram, `oxe_engine_breaker_state{engine}` gauge,
  `oxe_cache_entries` gauge, `oxe_admission_wait_ms`, `oxe_admission_rejected_total{reason}`,
  `oxe_deadline_hit_total`, `oxe_stale_served_total`; the registry renders Prometheus
  text on `GET /metrics`; `/api/stats` gains `engines[]` with median/p80/p95 split
  http/parse, result count, reliability % (SearXNG parity) and the cache/admission
  aggregates; `/api/stats` day series still come from `search_log`. If pull-metrics
  scope grows, revisit `metrics-exporter-prometheus`.
- Acceptance: `/metrics` text parses with a Prometheus text parser crate in the test and
  contains the instruments above after one replay search; `/api/stats.engines[0].p95_ms`
  is a number.
- Follow-up: W2-03, W2-05.

### W1-10 Cache tier 2: FTS5 lexical
- Issue #30 · Effort M · Label feature · Team Systems · Branch `v3/w1-10-tier2-fts`
- Depends on: W0-12
- Do: on tier-1 miss, `Store::get_lexical(q, 5)` over `cache_fts` (query text weighted 3x,
  titles 2x, snippets 1x, BM25); accept a hit when the top row's query, after
  normalisation and stopword removal, shares >= 80 % tokens with the request (Jaccard) and
  the same page/lang; serve as `Source::Cache{tier:2}` with the matched query in `meta`;
  config `cache.lexical.enabled` (default true) and threshold.
- Acceptance: after caching `tanstack router docs`, the request `docs tanstack router`
  returns `tier:2`; `tanstack query docs` (Jaccard 0.5) does not.
- Follow-up: `later/semantic-tier.md` (tier 3 reuses the acceptance shape).

### W1-11 Cutover: oxmgr, `~/SEARCH.md`, live smoke
- Issue #31 · Effort S · Label infra · Team Systems · Branch `v3/w1-11-cutover`
- Depends on: W1-03, W1-04, W1-05, W1-06, W1-07, W1-08
- Do: `tests/live/smoke.rs` (`OXE_LIVE=1`): 3 queries each for bing, brave, wikipedia,
  assert >= 3 results and no `Blocked`; nightly workflow runs it; install docs
  (`docs/install.md`: `cargo install --path crates/oxe-cli`, launchd via oxmgr sample entry
  replacing `/Users/.../uv/tools/oxe/bin/oxe` with the v3 binary on 4479, portless alias);
  update `~/SEARCH.md` text is the owner's, but the PR ships `docs/agents.md` with the new
  MCP wiring for Claude Code, Hermes, maki. Owner performs the switch; issue stays `WIP`
  until Claude Code completes one `search_web` through v3.
- Acceptance: live smoke green in nightly; owner confirms wiring on the issue.
- Follow-up: W2-01, W2-10.

### W1-12 Modes: `--headless` and cargo features
- Issue #32 · Effort S · Label feature · Team Systems · Branch `v3/w1-12-modes`
- Depends on: W1-08
- Do: `oxe serve --headless` skips template routes and static assets (routes table entries
  carry `requires: ui`); cargo features `ui`, `mcp`, `ai`, `archive`, `semantic`,
  `postgres`, `otlp` gate the corresponding modules with `cfg` and the routes table filters
  by compiled features; `otlp` becomes non-default here (flip the W0-05 default) so
  tonic/prost leave the default build; `mise run validate` builds default features and
  `--no-default-features --features mcp` to keep both compiling. Budget test gains
  headless < 50 MB and `oxe mcp` < 40 MB idle.
- Acceptance: both feature sets build in CI; `--headless` returns 404 for `/search` and 200
  for `/api/search`; routes-table test filters correctly.
- Follow-up: W2-11.

### W1-13 Loopback request guard (Host/Origin + non-loopback refusal)
- Issue #84 · Effort S · Label infra · Team Systems · Branch `v3/w1-13-loopback-guard`
- Depends on: W0-09
- Do: `oxe-server` Host/Origin check middleware (~50 LoC): reject requests whose `Host`
  header is not the configured bind host (`localhost`, `127.0.0.1`, `::1`, portless
  aliases), and on mutating methods require `Origin` absent or same-host. Mirror W6-03's
  auth line: `auth.enabled` defaults false on loopback and is forced when bind is not
  loopback; auth itself is deferred to `later/postgres-and-multi-instance.md`, so for now
  a non-loopback bind refuses to start with a clear error.
- Acceptance: routes-table test asserts mutating routes carry the guard; a test with a
  foreign Host gets 403.
- Follow-up: W2-01.

## Out of scope for W1

SSE streaming, pages other than search, hedging, fairness, AI, archive.

## Follow-up

W2 `wave-2-ui-and-observability.md`. Findings from the owner's two days on W0 (`bug` issues
under this EPIC) are fixed inside W1 before W1-11.
