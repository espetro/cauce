# W0: skeleton, real results, traceable requests

Iteration 1 (2026-09-22 to 2026-10-05). Priority P0. EPIC issue: #1.
Parent: `../2026-09-21-v3-rust-core.md`. Index and pickup rules: `README.md`.

## Goal

A single binary (`oxe`) that, started with `oxe serve`, answers `GET /api/search?q=` with
real results through the `ddgs` exec bridge, caches them in SQLite, logs every request as
JSONL with a `request_id` that `oxe trace <id>` can replay, and renders an HTMX search page.
CI runs the golden path against the `replay` engine. Nothing else.

## Settled inputs (do not change without amending the parent plan)

- Crate layout, parent section 4.1. Dependency direction is one-way toward `oxe-core`.
- `SearchRequest`, `SearchResult`, `SearchMeta`, `SearchResponse`, `Engine`, `EngineError`,
  `Store` shapes, parent section 4.2. `CacheKey` = sha256 over (normalised q, page, lang,
  time_range, safesearch) plus `engines` only when explicitly pinned.
- Tables and their single writer, parent section 5. `search_log` is written for every
  request, cache hit or not. `audit` table: `ts, actor, action, target, details_json,
  request_id`.
- Routes table, parent section 6, minus everything marked W1+ there. No Exa HTTP route.
- Verification policy, parent section 7. Golden path assertions are listed in W0-12.
- Config: TOML at `$OXE_CONFIG_DIR/config.toml` (default `~/.config/oxe/`), data at
  `$OXE_DATA_DIR` (default `~/.local/share/oxe/`: `oxe.db`, `logs/`). Interpolation
  `${env:NAME}`, `${env:NAME:-default}`, `${env:NAME:?msg}`, `${file:PATH}`, `$$` escape;
  templates preserved on save. Precedence defaults < file < env (`OXE_*`).
- Resource-adaptive defaults: read available memory and cores at startup (`sysinfo`); derive
  SQLite `cache_size`/`mmap_size`, upstream concurrency, test threads. Never a fixed large
  allocation.
- Port 4479, loopback bind by default.
- Licenses: MPL-2.0 for `crates/*`, Apache-2.0 for `engines/` and `sdk/`. Copyright line
  `Copyright (c) 2026 Joaquin Terrasa and oxe contributors`. DCO sign-off required.
- Toolchain: `mise.toml` pins `rust` (latest stable at W0-02 time), edition 2024. One gate:
  `mise run validate`.

## Exit criteria

1. `mise run validate` green locally in < 3 min on the owner's 8 GB machine and in CI in < 8 min.
2. Golden path (W0-12) green on `replay` in CI.
3. `OXE_ENGINES=ddgs oxe serve` then `curl 'localhost:4479/api/search?q=tanstack+router'`
   returns real results; second call returns `meta.source.cache.age_s > 0`.
4. `oxe trace <request_id>` prints the fan-out timeline of that request.
5. `/search?q=` renders results and the cached badge with age in a browser.

## Steps

### W0-01 License split, DCO, repo hygiene
- Issue #9 · Effort S · Label infra · Team Systems · Branch `v3/w0-01-license`
- Depends on: none
- Do: replace root `LICENSE` with MPL-2.0 (copyright line above); add `engines/LICENSE` and
  `sdk/LICENSE` as Apache-2.0; add `DCO` file (Linux Foundation text) and `CONTRIBUTING.md`
  (sign-off, pickup procedure link); `.gitignore` for Rust (`target/`), keep the
  `!.agents/plans/` override; `.editorconfig`; `deny.toml` skeleton for `cargo-deny` with
  license allowlist (MPL-2.0, Apache-2.0, MIT, BSD-*, ISC, Unicode, Zlib; deny GPL/AGPL).
- Acceptance: `cargo deny check licenses` passes on the empty workspace of W0-02; a commit
  without `Signed-off-by` fails the DCO check in CI (W0-02 wires the action).
- Follow-up: W0-02.

### W0-02 Workspace, toolchain, validate gate, CI, pre-push
- Issue #10 · Effort M · Label infra · Team Systems · Branch `v3/w0-02-workspace`
- Depends on: W0-01
- Do: Cargo workspace with the five crates as empty libs/bin (`oxe-cli` builds `oxe`);
  `mise.toml` with `rust` pinned and tasks `fmt`, `lint` (clippy `-D warnings`), `test`,
  `deny`, `validate` (depends on all), `dev` (`cargo run -- serve`); `.githooks/pre-push`
  running `mise run validate`, `mise run hooks:install`; `.github/workflows/validate.yml` on
  every push with `Swatinem/rust-cache`, `concurrency: cancel-in-progress` per ref, DCO
  action; `.github/workflows/nightly.yml` stub (cron, runs `mise run validate:nightly`, empty
  for now). Rust `test` uses `cargo nextest` if trivially available, else `cargo test`, with
  thread count from available cores.
- Acceptance: CI green on the branch; pre-push blocks a `cargo fmt` violation locally; a
  `.gitignore`d `target/` never appears in `git status`.
- Follow-up: W0-03.

### W0-03 oxe-core domain types and traits
- Issue #11 · Effort M · Label feature · Team Systems · Branch `v3/w0-03-core-types`
- Depends on: W0-02
- Do: implement parent 4.2 verbatim in `oxe-core`: request/result/meta/response types
  (serde, `#[serde(deny_unknown_fields)]` on inbound), `Engine` and `Store` traits
  (`async_trait`), `EngineError`, `EngineId`, `Tier`, `ClientKind` (`Ui`, `Api`,
  `Mcp(String)`, `Cli`), `Source` enum, `CacheKey::from(&SearchRequest)` with the pinned-engines
  rule, URL normalisation (`normalize_url`: lowercase scheme/host, strip `utm_*`, `fbclid`,
  `gclid`, trailing slash, default ports, fragment). Property tests (`proptest`) for
  `CacheKey` stability under engine-order permutation and for `normalize_url` idempotence.
- Acceptance: `cargo test -p oxe-core` green; a doc test shows `CacheKey` equality for
  `engines=None` vs `engines=Some(default set)` is **not** equal (pinned differs) and
  `engines=None` twice is equal.
- Follow-up: W0-04, W0-05, W0-06, W0-07, W0-11 (parallel).

### W0-04 oxe-store-sqlite: schema, migrations, Store impl
- Issue #12 · Effort L · Label feature · Team Systems · Branch `v3/w0-04-sqlite-store`
- Depends on: W0-03
- Do: `rusqlite` (`bundled`, `fts5`), WAL, busy timeout; migrations as numbered SQL files
  embedded with `include_str!` and a `schema_version` table; tables `cache_entries`,
  `cache_fts` (FTS5 external content + triggers), `search_log`, `clicks`, `audit`,
  `engine_health` per parent section 5 (`cache_vec`, `answers`, `pages` are later waves);
  `Store` impl for every method the parent lists except `get_semantic`; a `tokio` interval
  task `evict_expired` every 5 min; a `Store` **conformance test module** (`oxe-core` tests
  behind a `pub mod conformance` feature) that W6 reuses for Postgres. Connection is a
  single writer behind a `tokio::sync::Mutex`, reads on a small pool; all calls via
  `spawn_blocking`. PRAGMA `cache_size`/`mmap_size` from the resource-adaptive config (W0-11
  provides the struct; until merged, take a `StoreTuning` argument with defaults).
- Acceptance: conformance suite green on a temp file; `put` then `get_exact` round-trips a
  `SearchResponse`; expired rows are invisible to `get_exact` and removed by `evict_expired`;
  `get_lexical("tanstack")` finds a row whose title mentions TanStack; `log_search` on every
  call is asserted by the pipeline test in W0-08.
- Follow-up: W0-08.

### W0-05 Observability foundation: JSONL, request_id, audit, `oxe trace`
- Issue #13 · Effort M · Label feature · Team Systems · Branch `v3/w0-05-observability`
- Depends on: W0-03
- Do: `tracing` + `tracing-subscriber`: JSON layer writing `logs/oxe-YYYY-MM-DD.jsonl` via
  `tracing-appender` (daily rotation, retention `logs.retention_days`, default 7) and a
  pretty stderr layer when `OXE_LOG_PRETTY=1` or a TTY; `tracing-opentelemetry` +
  `opentelemetry-otlp` behind cargo feature `otlp` (default on), active only when
  `OTEL_EXPORTER_OTLP_ENDPOINT` is set; a `RequestId` (UUIDv7) generated per inbound request
  (HTTP middleware in W0-09, MCP in W1-08, CLI in W0-12) and attached as a span field so every
  event carries it; `Store::audit()` helper plus `audit=true` field on the JSONL event;
  `oxe trace <request_id>` reads the JSONL files (newest first), filters by id, prints an
  ordered timeline (engine spans with durations, cache tier decisions, errors); `oxe tail`
  is W2-09.
- Acceptance: a unit test drives a fake pipeline span tree and asserts `oxe trace` output
  contains the engine names in order with durations; JSONL lines parse with `serde_json`
  and contain `request_id`, `level`, `target`, `ts`.
- Follow-up: W0-08, W0-09, W1-09.

### W0-06 `replay` engine and `oxe record`
- Issue #14 · Effort M · Label feature · Team Systems · Branch `v3/w0-06-replay-engine`
- Depends on: W0-03
- Do: `oxe-engines::replay`: cassette format `engines/fixtures/<engine>/<sha8(query)>.json`
  (`{query, engine, recorded_at, raw_response_path?, results: [SearchResult]}`); `Replay`
  implements `Engine`, keyed by normalised query, falls back to synthetic mode when no
  cassette (deterministic results from a seeded RNG, N=10, plausible titles/URLs/snippets);
  fault injection via config/env: `latency_ms`, `fail_every` (nth call returns
  `EngineError::Transport`), `blocked` (always `Blocked`), `empty` (zero results),
  `page_limit`. `oxe record --engine <id> --query <q>` runs a real engine and writes a
  cassette (works once W0-07/W1 engines exist; on W0 it records the exec engine).
- Acceptance: same query twice yields identical results; `fail_every=2` fails exactly
  calls 2, 4, 6; synthetic mode returns 10 results with unique normalised URLs.
- Follow-up: W0-08, W0-12.

### W0-07 `exec` engine runtime, Python SDK, `ddgs_auto.py`
- Issue #15 · Effort M · Label feature · Team Systems · Branch `v3/w0-07-exec-engine`
- Depends on: W0-03
- Do: `oxe-engines::exec`: spawn `command args...` once, keep warm, JSON lines over
  stdin/stdout per parent 4.3 (`v: 1`), one request in flight per process, kill and respawn
  on crash or on deadline overrun, stderr forwarded to tracing at `warn`; config
  `[[engines]] id="ddgs" kind="exec" command="python3" args=["sdk/python/oxe_engine_sdk/ddgs_auto.py"]`.
  `sdk/python/`: `oxe_engine_sdk` package (`run(fn)` stdio loop, dataclasses, ~60 lines,
  zero deps) and `ddgs_auto.py` calling `DDGS().text(q, backend="auto", max_results=10,
  page=...)`; `uv`-managed `pyproject.toml` in `sdk/python` with `ddgs` as an optional extra.
- Acceptance: integration test spawns a tiny Python echo engine from the test dir and
  asserts round-trip, crash respawn, and deadline kill; `OXE_LIVE=1` test runs `ddgs_auto.py`
  for one query and asserts >= 1 result.
- Follow-up: W0-08.

### W0-08 SearchPipeline v0
- Issue #16 · Effort L · Label feature · Team Systems · Branch `v3/w0-08-pipeline`
- Depends on: W0-04, W0-05, W0-06, W0-07
- Do: `oxe-core::pipeline`: `search(req) -> SearchResponse`: normalise; tier-1
  `get_exact` (fresh hit returns with `Source::Cache{tier:1, age_s, ttl_s, stale:false}`);
  on miss fan out to all configured engines in parallel with `tokio::time::timeout` at the
  hard deadline (default 3000 ms, config `search.deadline_ms`), collect what arrived, merge
  (dedupe by `normalize_url`, RRF k=60 across engines, stable), build `SearchMeta`
  (`engines_used` with per-engine status/latency/result count, `deadline_hit`), `put` with
  TTL (default 3600 s, per-request `ttl_s` capped at 86400), then `log_search`
  unconditionally (also on the cache-hit path). Errors: all engines failed ->
  `PipelineError::AllEnginesFailed(Vec<(EngineId, EngineError)>)`; zero results with at
  least one engine OK is a valid empty response. Every stage is a `tracing` span with the
  request_id. No hedging, no breaker, no tier 2 yet (W1/W3).
- Acceptance: with two `replay` engines (one `latency_ms=5000`) the response arrives at
  ~3000 ms with `deadline_hit=true` and the fast engine's results; a cache hit is served in
  < 5 ms and still appends a `search_log` row with `source=cache`; RRF puts a URL returned
  by both engines first.
- Follow-up: W0-09.

### W0-09 axum server: JSON routes and the routes-table test
- Issue #17 · Effort M · Label feature · Team Product Builders · Branch `v3/w0-09-routes`
- Depends on: W0-08
- Do: `oxe-server`: `ROUTES: &[RouteSpec]` (method, path, kind: Json/Html/Sse/Mcp, wave)
  as the single declaration; a builder mounts them on axum; middleware sets `RequestId`,
  `X-Request-Id` response header, `ClientKind` from path/headers; routes in this wave:
  `GET /api/search`, `GET /api/history`, `POST /api/click`, `GET /api/stats`,
  `GET /api/cache`, `GET /api/cache/{key}`, `DELETE /api/cache/{key}`,
  `DELETE /api/cache?expired=true|all=true` (logs `audit`), `GET /api/audit`, `GET /health`,
  `GET /api/config` (redacted templates shown as-is, resolved secrets never), `PUT /api/config`
  (validates, preserves templates, audits). Errors as a typed JSON envelope
  `{error: {code, message, request_id}}`. Test: the routes table equals the live router's
  route list and equals the table in the parent plan section 6 filtered to `wave <= 0`.
- Acceptance: routes-table test green; `DELETE /api/cache/{key}` then search is `Network`;
  `/api/config` never contains a resolved `${env:...}` value.
- Follow-up: W0-10, W0-12, W1-08.

### W0-10 HTMX search page
- Issue #18 · Effort M · Label feature · Team Product Builders · Branch `v3/w0-10-search-page`
- Depends on: W0-09
- Do: `askama` templates embedded via `rust-embed` (HTMX 2.x and a small CSS file vendored,
  no CDN); `GET /` landing with the search form, `GET /search?q=` results (same handler as
  `/api/search`, content-negotiated on `Accept`), result rows (favicon via
  `https://icons.duckduckgo.com/ip3/<host>.ico`, title, host, snippet), meta line with
  `N results`, `cached · 12 s ago · ttl 58 min` or `live · 640 ms · bing, brave`, request id
  (small, copyable), `more` button loading page 2 via `hx-get`. Click on a result sends the
  `POST /api/click` beacon (`hx-post`, `hx-swap=none`) before following the link. No AI
  toggle, no streaming yet (W2-01). Use the v2 screen specs under `../../docs/screens/` as
  requirement input only.
- Acceptance: an `axum` integration test renders `/search?q=x` on `replay` and asserts the
  titles and the badge text; the golden path (W0-12) asserts the click beacon lands in
  `clicks`.
- Follow-up: W0-12, W2-01.

### W0-11 Config loader, interpolation, XDG dirs, resource-adaptive defaults
- Issue #19 · Effort S · Label feature · Team Systems · Branch `v3/w0-11-config`
- Depends on: W0-03
- Do: `oxe-core::config`: typed `Config` (serde, `deny_unknown_fields`), loaded from
  defaults < TOML < `OXE_*` env; interpolation per settled inputs; `Config::save` writes the
  raw (template) tree back, never resolved values; `Resources::detect()` (sysinfo) ->
  `StoreTuning`, `upstream_concurrency`, `test_threads`; `oxe config show` (redacted) and
  `oxe config path`. Defaults: `ai.base_url = ""`, `ai.api_key = ""` (an
  `${env:...}`/`${file:...}` template), `ai.enabled = false` until W4.
- Acceptance: unit tests for every interpolation form incl. `:?` failing startup and `$$`
  escape; save/load round-trip preserves `${env:CAUCE_AI_API_KEY}` literally; a 4 GB machine
  yields a smaller `cache_size` than a 32 GB one (inject the numbers).
- Follow-up: W0-08 (consumes `StoreTuning`), W0-12.

### W0-12 Golden path e2e and budget test
- Issue #20 · Effort M · Label infra · Team Systems · Branch `v3/w0-12-golden-path`
- Depends on: W0-09, W0-10, W0-11
- Do: `tests/e2e/golden_path.rs`: build the `oxe` binary (`env!("CARGO_BIN_EXE_oxe")`),
  start `oxe serve --port 0` with `OXE_ENGINES=replay`, temp `OXE_DATA_DIR`/`OXE_CONFIG_DIR`,
  then assert in one flow: search -> `Network`; same search -> `Cache{tier:1}` with
  `age_s >= 0`; `/api/history` has 2 rows (sources network, cache); `/api/stats` hit rate
  0.5; `POST /api/click` -> history shows the click; `/search?q=` HTML contains titles and
  `cached`; `DELETE /api/cache/{key}` -> search is `Network` again; `/api/audit` has the
  delete with actor `api`; `oxe trace <request_id of first search>` (run the binary) prints
  the replay engine span. `tests/e2e/budget.rs` (nightly only, `OXE_NIGHTLY=1`): release
  binary size < 30 MB, RSS after 100 replay searches < 80 MB. `OXE_LIVE=1` variant of the
  first two assertions on the `ddgs` exec engine.
- Acceptance: golden path green in CI fast gate in < 60 s; nightly workflow runs budget +
  live.
- Follow-up: W1-01 (unblocks all of W1).

## Out of scope for W0

Declarative engines, breaker/EWMA, hedging, tier-2/3 cache, MCP, metrics endpoint, SSE,
any page other than landing/search, AI, headless mode. Each has a step in W1+.

## Follow-up

W1 `wave-1-engines-and-agents.md`. Before starting W1-11 (cutover), the owner runs W0 for
two days via `OXE_ENGINES=ddgs` and files findings as issues labelled `bug` under the W1 EPIC.
