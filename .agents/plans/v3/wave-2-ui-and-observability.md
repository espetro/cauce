# W2: usable UI and observability pages

Iteration 3 (2026-10-20 to 2026-11-02). Priority P1. EPIC issue: #3.
Parent: `../2026-09-21-v3-rust-core.md`. Index: `README.md`. Previous: `wave-1-engines-and-agents.md`.

## Goal

Every surface the parent plan's routes table lists as HTML exists, is server-rendered with
HTMX, works on a phone width, and reads the same data planes the API does. Progressive
results over SSE. The owner uses the UI daily for a week and files findings.

## Settled inputs

- W0/W1 settled inputs. All pages are `askama` templates embedded with `rust-embed`; HTMX 2.x
  vendored; one CSS file with CSS variables for light/dark (`prefers-color-scheme` plus a
  toggle stored in `localStorage`); no JS framework, no build step.
- Pages read only via the same handlers as `/api/*` (content negotiation on `Accept`), never
  a second data path.
- Every page shows the `request_id` of the data it rendered (footer, copyable).
- The v2 screen specs (`../../docs/screens/*.md`) are requirement input: use their checkpoint
  lists to decide what a page must show; ignore their component/token prescriptions.
- Copy is English only in v3.0; strings live in one `strings.rs` so i18n is a later mechanical
  change, not a rewrite.

## Exit criteria

1. Owner's usage week completed with findings triaged (W2-10).
2. Every page renders at 390 px and 1280 px; no horizontal scroll at 390 px (manual
   acceptance note, checked during W2-10).
3. No page state exists that the replay engine's fault-injection cannot reach (empty, error,
   slow, blocked, stale).

## Steps

### W2-01 SSE search stream and progressive results
- Issue #33 · Effort M · Label feature · Team Product Builders · Branch `v3/w2-01-sse-stream`
- Depends on: W1-11
- Do: `GET /api/search/stream?q=` (routes table) emitting `event: results` per engine
  batch (`{engine, results, elapsed_ms}`), `event: meta` at the end, `event: error`; the
  pipeline gains a `search_stream()` variant yielding batches as engines return (merge
  state kept incrementally, final RRF order sent in `meta.order`); the HTMX search page uses
  the SSE extension (`hx-ext="sse"`) to append batches below the fold and a small
  "N new results above" pill when a later batch outranks visible items, never reordering
  what is on screen; falls back to the non-streaming page without JS.
- Acceptance: with two replay engines (`latency_ms` 100 and 2000) the first `results` event
  arrives < 300 ms and `meta` at ~2000 ms; the page test sees the first titles before the
  slow engine's.
- Follow-up: W2-08.

### W2-02 History page
- Issue #34 · Effort S · Label feature · Team Product Builders · Branch `v3/w2-02-history`
- Depends on: W1-11
- Do: `/history`: table from `search_log` joined with `clicks` (query, when, source/tier,
  engines, result count, latency, client, clicks), filters `since` (24h/7d/30d/all) and `q`
  substring as URL params, 200-row cap, re-run link (`/search?q=`), copy-json link
  (`/api/search?q=` cached), delete-row (audited). Empty state on replay `empty`.
- Acceptance: page test after 3 replay searches shows 3 rows with the right sources; filter
  `since=24h` hides a row backdated in the temp DB.
- Follow-up: W2-10.

### W2-03 Dashboard page
- Issue #35 · Effort M · Label feature · Team Product Builders · Branch `v3/w2-03-dashboard`
- Depends on: W1-09
- Do: `/dashboard` from `/api/stats`: searches per day (cache vs network, inline SVG bars,
  no charting lib), hit rate by tier, TTFR and full-latency p50/p90/p99, client split
  (ui/api/mcp names), top queries, zero-result queries, deadline-hit and stale-served rates,
  engine table (median/p80/p95 http vs parse, reliability, breaker state) linking to
  `/engines`, cache panel (rows, unexpired, db size, newest). Window selector 7/30 days.
- Acceptance: page test after mixed replay traffic shows non-zero hit rate and the engine
  rows; "no data yet" states render on an empty DB.
- Follow-up: W2-10.

### W2-04 Cache page
- Issue #36 · Effort S · Label feature · Team Product Builders · Branch `v3/w2-04-cache-page`
- Depends on: W1-11
- Do: `/cache`: paginated list of `cache_entries` (query, created, expires/age, hits,
  engines, size), search box, row expand to the stored payload (pretty JSON), delete row,
  "delete expired", "delete all" with confirm (all audited with actor `ui`).
- Acceptance: delete via the page removes the row and writes an `audit` row; next search is
  `Network`.
- Follow-up: W2-10.

### W2-05 Engines page
- Issue #37 · Effort S · Label feature · Team Product Builders · Branch `v3/w2-05-engines-page`
- Depends on: W1-06, W1-09
- Do: `/engines`: one card per configured engine: kind (declarative/exec/replay), tier,
  enabled, breaker state with time remaining, EWMA, last ok, last error, p95, reliability,
  requests today; actions: reset breaker (audited), run a test query (calls
  `/api/search?engines=<id>` and shows results inline), enable/disable (writes config,
  audited).
- Acceptance: with a `blocked` replay engine the card shows `Open` and reset flips it to
  `HalfOpen`.
- Follow-up: W2-10.

### W2-06 Audit page
- Issue #38 · Effort S · Label feature · Team Product Builders · Branch `v3/w2-06-audit-page`
- Depends on: W1-11
- Do: `/audit` from `/api/audit`: newest first, filters by actor and action, details
  expandable, `request_id` link to `/trace/<id>` which renders the `oxe trace` timeline as
  HTML (new route, same code path as the CLI).
- Acceptance: a cache delete from W2-04's test appears with actor `ui`; `/trace/<id>` of a
  replay search lists the engine span.
- Follow-up: W2-10.

### W2-07 Settings page
- Issue #39 · Effort M · Label feature · Team Product Builders · Branch `v3/w2-07-settings`
- Depends on: W0-11, W1-11
- Do: `/settings` editing the TOML through `PUT /api/config`: search (deadline, ttl,
  hedge threshold placeholder for W3), engines (enable, tier, egress proxy), admission,
  logging retention, AI section (base_url, api_key template shown verbatim, env var
  status "set/not set", model picker from `GET {base_url}/models` with free-text fallback,
  enabled toggle, greyed until W4). Templates are never resolved in the form; saving
  preserves them. Validation errors inline.
- Acceptance: page test edits `search.deadline_ms`, saves, reloads and sees the value; the
  `${env:BIFROST_API_KEY}` template survives a save round-trip byte-for-byte.
- Follow-up: W4-01.

### W2-08 Theme and mobile layout
- Issue #40 · Effort S · Label cosmetic · Team Product Builders · Branch `v3/w2-08-theme-mobile`
- Depends on: W2-01 to W2-07
- Do: light/dark via CSS variables and a header toggle; header collapses to a menu below
  700 px; tables become cards below 640 px; a cheap DOM assertion instead of screenshot
  baselines: an axum integration test renders each page on replay within a sane element
  count and asserts the viewport meta and the light/dark CSS variables exist.
- Acceptance: DOM assertion test green for every page; no horizontal scroll at 390 px
  (checked manually).
- Follow-up: W2-10.

### W2-09 `oxe tail`
- Issue #41 · Effort S · Label feature · Team Systems · Branch `v3/w2-09-tail`
- Depends on: W0-05
- Do: `oxe tail [--follow] [--request <id>] [--level warn] [--engine bing]` pretty-prints
  the JSONL logs with colour, one line per event, spans collapsed to `engine=bing 640ms ok`.
- Acceptance: unit test renders a fixture JSONL and asserts the collapsed engine line.
- Follow-up: none (tooling complete for v3.0).

### W2-10 Owner usage week and triage
- Issue #42 · Effort S · Label infra · Team Product Builders · Branch none
- Depends on: W2-01 to W2-09
- Do: owner uses v3 daily (browser default search via W2-11, agents via MCP) for 7 days;
  findings filed as `bug`/`cosmetic` issues under this EPIC or, when they change behaviour,
  as new steps proposed for W3; at the end, `.agents/notes/<date>-v3-usage-week.md` records
  what was fixed and what moved.
- Acceptance: the note exists and every filed issue is triaged (`Scheduled` or closed).
- Follow-up: W3-01.

### W2-11 OpenSearch descriptor and `oxe search` CLI formats
- Issue #43 · Effort S · Label feature · Team Product Builders · Branch `v3/w2-11-opensearch-cli`
- Depends on: W1-12
- Do: `GET /opensearch.xml` (routes table) with `/search?q={searchTerms}` and a
  suggestions URL placeholder; `<link rel="search">` in the page head so browsers offer
  "add search engine"; `oxe search "<q>" [--json|--table|--urls] [--engines ...]` calling
  the pipeline in-process (no server needed, shares the DB) with the `request_id` printed to
  stderr.
- Acceptance: `oxe search --json x` on replay prints a `SearchResponse`; `/opensearch.xml`
  validates against the OpenSearch 1.1 schema in a test.
- Follow-up: none.

## Out of scope for W2

Hedging, fairness, AI answer page (W4), archive pages (W5), auth (W6), i18n.

## Follow-up

W3 `wave-3-tail-tolerance.md`, seeded by W2-10's findings.
