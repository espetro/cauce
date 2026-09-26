---
name: testing-cauce-serve
description: How to spin up isolated `cauce serve` instances for e2e testing — config/env conventions, replay-engine fault injection, port/dir layout, and UI/API check tactics.
---

# Testing `cauce serve` end-to-end

## Spawn conventions

- Binary: `cargo build -p cauce-cli` → `target/debug/cauce`. Run from the **workspace root**
  so the replay engine's default `fixtures_root = engines/fixtures` resolves.
- Per-instance env: `CAUCE_DATA_DIR` (SQLite store), `CAUCE_CONFIG_DIR` (holds `config.toml`),
  `CAUCE_LOG=info`. Give each instance its own pair of dirs — a settings save writes
  `$CAUCE_CONFIG_DIR/config.toml` and would clobber a shared file.
- Start: `CAUCE_DATA_DIR=… CAUCE_CONFIG_DIR=… cauce serve --bind 127.0.0.1 --port <fixed>`.
  The e2e harness uses `--port 0` + parses `addr:` from stderr; for browser testing prefer a
  fixed port. `/health` answers 200 when up (≈1–2 s).

## Engine config

- `config.toml` `[[engines]]` entries default `enabled = true`. Builtins `replay`
  (enabled=false) and `ddgs` (exec, **tier 2**, enabled=true) always exist; a file entry
  with the same id wins.
- `CAUCE_ENGINES=id1,id2` pins the enabled set to exactly those ids — use it to isolate
  test engines and suppress builtin ddgs, which would otherwise join the tier-2 hedge set.
- Engine ids must match `[A-Za-z0-9._-]+` (hyphens OK, e.g. `replay-t2`).
- `tier = 2` in an `[[engines]]` entry defers that engine to the W3-01 hedge wave.
- Replay kind honours entry `id`/`tier`, so `[[engines]] id="replay" kind="replay"` plus
  `[[engines]] id="replay-t2" kind="replay" tier=2` runs two replay instances.

## Fault injection is SHARED env — design around it

`CAUCE_REPLAY_LATENCY_MS`, `CAUCE_REPLAY_EMPTY`, `CAUCE_REPLAY_FAIL_EVERY`,
`CAUCE_REPLAY_BLOCKED`, `CAUCE_REPLAY_PAGE_LIMIT` apply to **all** replay instances in the
process — you cannot give tier-1 and tier-2 different latencies from env. Multiple
`cauce serve` processes with different env is the workaround.

- `CAUCE_REPLAY_EMPTY=1` → instant `no_results` answers (good for the `reason="few"` early
  hedge fire).
- `CAUCE_REPLAY_LATENCY_MS=N` → every replay call sleeps N ms (good for the `reason="slow"`
  floor fire; with shared latency the t2 batch always lands ~hedge_point ms after t1's, so
  a "t2 answers first" ordering cannot be produced at serve level).
- Hedge knobs: `search.min_results`, `search.hedge_floor_ms`, `search.hedge_ceiling_ms` in
  config or `CAUCE_SEARCH_*` env (env also greys the /settings input + shows a "set by …"
  hint). Defaults: min_results=5, floor=300, ceiling=1500, deadline=3000.
- The tier-1 latency histogram (P90 hedge point) is **in-memory**: restart resets it → the
  first uncached search hedges at the floor; afterwards P90≈latency → hedge point clamps to
  the ceiling when latency > ceiling. A late-fired t2 whose remaining deadline < latency
  gets clipped (`failed: timeout`, `deadline_hit:true`); enough clips open its breaker and
  the hedge then reports `engines_skipped` instead of `hedged` — that is intended fire-time
  gating, not a bug.
- `CAUCE_SEARCH_HEDGE_FLOOR_MS > CAUCE_SEARCH_HEDGE_CEILING_MS` panics `Duration::clamp`
  per request (502 "in-flight search task vanished") — no validation exists (bug).

## Check tactics

- API: `GET /api/search?q=…` → `meta.hedged`, `meta.hedge_at_ms`, `meta.engines_used`,
  `meta.engines_skipped`, `meta.source` (`"network"` vs `{cache:{…}}`), `meta.deadline_hit`.
- SSE: `GET /api/search/stream?q=…` → `event: results` per engine completion then terminal
  `event: meta`; streamed searches populate the cache too.
- Metrics: `GET /metrics` → `cauce_hedge_total{reason="slow"|"few"}` (unlabeled `0` before
  the first fire), `cauce_engine_breaker_state{engine}` (0 closed / 1 half-open / 2 open).
- Browser URL-bar navigation to `/api/search?q=…` returns **HTML** (Accept negotiation),
  not JSON. To show raw JSON in the page for a recording, use devtools console:
  `fetch('/api/search?q=…').then(r=>r.json()).then(j=>{document.body.innerHTML='<pre>'+JSON.stringify(j.meta,null,2)+'</pre>'})`.
  `/metrics`, `/settings`, `/history`, `/search` render fine directly.
- `/settings` form PUTs to `/api/config` (hx-put); **"changes apply on restart"** — a saved
  value does not affect the running process, only the on-disk `config.toml`.
- Shell kills: never `pkill -f "port 4479"` — the pattern also matches your own wrapping
  shell command and kills it (same for `pkill -f stub.py` etc). Kill by PID from
  `ss -ltnp | grep :PORT`.

## AI provider e2e (`/answer`, `[ai]`)

- **Devin session secrets inject `CAUCE_AI_*` env vars into every shell**
  (`CAUCE_AI_BASE_URL`, `CAUCE_AI_API_KEY`, `CAUCE_AI_MODEL`, `CAUCE_AI_ENABLED`), and env
  overrides beat `config.toml`. A serve launched without explicit values silently talks to
  the secret base_url (e.g. `https://openrouter.ai/api/v1/v1/messages` → 404). Always set
  all four (plus `CAUCE_AI_PROTOCOL`) explicitly on the serve command.
- Protocol: `[ai] protocol = "openai"` (default, `POST {base}/chat/completions` + Bearer)
  or `"anthropic"` (`POST {base}/v1/messages`, `x-api-key` + `anthropic-version: 2023-06-01`
  headers, no Bearer). `CAUCE_AI_PROTOCOL` env overrides. Unknown values fail `Config::load`
  → `cauce serve: invalid config: unknown variant …` + exit 2 (server never binds).
- Stub the provider instead of real egress: a ~100-line Python `http.server` replaying
  `crates/cauce-core/fixtures/ai/<proto>/sse_*.raw` bodies (`text/event-stream`) in call
  order works; add `GET /v1/models` → `models.json` and a `/log` endpoint so the request
  log (method, headers, fixture index) can be opened in the browser as evidence. Count
  POSTs separately for fixture ordering — any GET probe otherwise shifts the order.
- Search side of the answer loop: pin `CAUCE_ENGINES=replay` and point cassettes at
  `CAUCE_REPLAY_FIXTURES_DIR=<repo>/evals/ai/cassettes CAUCE_REPLAY_CASSETTE_ENGINE=replay`
  (files are `<dir>/<engine>/<sha8(normalized query)>.json`; the eval tokyo-weather pair
  covers the two fixture queries in `sse_toolcall*.raw`).
- `/answer?q=…` auto-POSTs `/api/answer` via inline fetch-SSE: `step` frames →
  `#answer-steps` `<li>`s, `delta` → `#answer-text` (literal text — markdown is NOT
  rendered), `sources` → numbered `.source-card`s, `done` → status + `confidence: N`.
  A fixture answer without a `{"confidence":…}` JSON tail ends `confidence=0` and is NOT
  cached (grounded cache needs ≥4), so repeat runs stay clean.
- Two answer modes share `/api/answer` (W7-02+): the tool loop sends a `"tools"` array
  (serve `sse_toolcall.raw` then `sse_answer.raw` per pair); the SERP **Assist** card
  POSTs `{q, context_results}` with NO tools key — dispatch on `'"tools"' in body` and
  serve a no-tools fixture (`sse_confident.raw` gives `[1]`/`[2]` cites + confidence).
  Assist reuses the answer cache keyed by query, so a second Assist click on the same
  query may serve instantly without hitting the stub — use a fresh query per click.
- In debug builds `rust-embed` reads `crates/cauce-server/assets/` from disk at runtime,
  so a rebuilt `app.js` (esbuild/`pnpm` in `crates/cauce-server`) is picked up without
  `cargo build` — but Askama template or Rust changes still need `cargo build -p cauce-cli`.
  Canary: `curl localhost:PORT/ | grep -o <bundled-fn-name>` proves the new bundle is served.

### Live provider runs (real OpenRouter egress)

- **Model choice decides whether the tool loop converges.** `openrouter/free` and most
  free-tier models emit tool calls every turn until the 5-iteration cap
  (`"exceeded max iterations (5) without a final answer"` error frame) — reasoning models
  (`nvidia/nemotron-3.5-lightning:free`) additionally hit the fixed 60 s per-call provider
  budget on the answer turn (`"provider request timed out"`), and popular free pools
  429 mid-loop. `liquid/lfm-2.5-2.6b:free` (the model in `evals/ai/transcripts/`) converges
  when its upstream isn't rate-limited but sometimes hallucinates tools (`visit_url`). A
  small paid model (`openai/gpt-4o-mini`, ~$0.000003/call) converged every time — check the
  key has credits via `GET {base}/credits`, and override with `CAUCE_AI_MODEL=` on the serve
  command (env beats config; disclose the override in the report).
- **Synthetic replay results make models keep searching.** Seed cassettes for the queries
  the model actually sends: watch the `step` frames' `query` fields on a first live run,
  then drop cassettes at `<fixtures>/replay/<sha8(normalized query)>.json` covering those
  phrasings (`evals/ai/cassettes/replay/` has a seed set). Once the first search returns
  realistic results the model usually answers on the next turn.
- **Cached answers are deterministic chip-state proof.** A grounded done
  (≥1 source, confidence ≥4) writes an `answers` row; reloading the same `?q=` replays
  `sources` + `done{cached:true}` with **no provider call** — immune to rate limits.
  The cache key includes the model, so the row persists across restarts under the same
  `CAUCE_MODEL` inside `CAUCE_DATA_DIR`.
- If the model wraps the metadata tail in a ```` ```json ```` fence, `parse_final_answer`
  misses it → `done.confidence=0` and the fenced JSON renders as literal answer text.
  Pre-existing parser behavior; pick a better-behaved model when a nonzero confidence
  chip matters for evidence. Observed live with `openai/gpt-4o-mini` on follow-up
  turns (turn 1 clean, turns 2+ fenced).

### Multi-turn `/answer` threads (W7-04)

- Follow-up probes must use a pronoun/referent (`who created it…`, `does it use a
  garbage collector?`) so a missing `history` is visually obvious — without thread
  replay the model can't resolve "it" and asks for clarification instead of naming
  Rust/Hoare.
- DOM spot-checks between turns (browser console):
  `document.querySelectorAll('.answer-turn').length` for turn count, per-turn cards
  `[...t.querySelectorAll('.source-card')].map(c=>c.id)` should be `src-<turn>-*`,
  `[...document.querySelectorAll('[id]')]` filtered for dupes should be empty
  (cloned turns are class-only), and `[n]` cite anchors in `.answer-text` carry
  `href="#src-<turn>-<n>"`. The follow-up input/button go `disabled` while a turn
  streams (`#answer-stream[aria-busy="true"]`).
- A fenced-metadata turn (`confidence 0/10`) still counts as a completed turn — it
  enters `history` and the follow-up bar stays usable; only `error` frames exclude
  the turn.

## Archive / click-beacon e2e (`/api/pages`, W5-01+)

- `archive.index_on_click` defaults true; `CAUCE_ARCHIVE_INDEX_ON_CLICK=false` removes the
  `fetch("/api/pages")` block from the results template — grep the rendered `/search` HTML to
  verify the beacon is present/absent. The beacon is a delegated `document` click listener on
  `#results a` (SSR, streaming and pagination alike); the `<article>` `hx-post="/api/click"`
  rows write the `clicks` table independently of the flag — a click with the flag off records
  `clicks` but no `pages` row (easy negative test).
- The archive `Fetcher` has an **egress guard** (#189): private/reserved targets (loopback,
  RFC 1918, link-local, CGNAT, multicast, `localhost`) are refused with a 4xx before any
  connect, on every redirect hop. Loopback fetch targets need the documented opt-in
  `CAUCE_ARCHIVE_ALLOW_PRIVATE=true` (`[archive] allow_private`); the scheme allowlist
  (`http`/`https` only) always applies. Synthetic replay
  results point at real hosts with fabricated paths (`https://en.wikipedia.org/guide/q-0`),
  so click-beacon indexing is non-deterministic against them. For a deterministic click →
  `pages` row, serve `crates/cauce-core/tests/fixtures/archive/` on a loopback port
  (`python3 -m http.server 8123 --bind 127.0.0.1 --directory crates/cauce-core/tests/fixtures/archive`)
  and hand-author a cassette whose result URLs point at it:
  `CAUCE_REPLAY_FIXTURES_DIR=<dir> CAUCE_REPLAY_CASSETTE_ENGINE=replay` with
  `<dir>/replay/<sha8(normalized query)>.json` in the Cassette schema
  (`{query,engine,recorded_at,results:[{url,title,snippet,engine,published,score}]}` —
  every field incl. `published:null` and `score` required). sha8 = first 8 hex of
  sha256(`" ".join(q.split()).lower()`).
- `sqlite3` CLI is NOT installed on the box — use `python3 -c "import sqlite3; ..."` against
  `$CAUCE_DATA_DIR/cauce.db` (`SELECT url,title,length(markdown),byte_len FROM pages`;
  `clicks` for the hx-post). `put_page` upserts: a second click on the same URL refreshes
  `fetched_at`, `count(*)` stays 1.
- HTTP surface: `POST /api/pages {"url"}` → 201 `PageRow` (`source_query_hash` null for direct
  calls, set for beacon calls); `GET /api/pages/{percent-encoded-url-as-one-segment}` →
  200 row or `{"error":{"code":"not_found",...}}`; unparseable url → 400 `bad_request`
  envelope, non-http(s) scheme → 403 `url_blocked`. Browser URL-bar GETs render raw JSON fine.
- Egress-guard wire detail (#189): 403 `url_blocked` covers BOTH refusal classes —
  private/reserved *addresses* (literal IPs `127.0.0.1`, `169.254.169.254`, ... in `check_url`
  pre-connect, and DNS names resolving to them — `localhost` — inside `GuardedResolver`)
  AND non-http(s) *schemes* (`file:///etc/passwd` → `Blocked` up front in `fetch_and_index`;
  a redirect hop to `file:` hits `check_scheme`). 400 `bad_request` is only for URLs that
  don't parse at all (`"not a url"` → `InvalidUrl`). On MCP both refusal classes are
  `invalid_params` (-32602). `CAUCE_ARCHIVE_ALLOW_PRIVATE=true` disarms only the range
  check — the scheme refusal stays. Side evidence for "guard refused before connect":
  the origin's access log shows no request (fixture `http.server` log = only
  allow_private-instance GETs).
- MCP probe without rmcp: `POST /mcp` works over plain curl. initialize with
  `accept: application/json, text/event-stream` and body
  `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"0.0.0"}}}`
  → response headers carry `mcp-session-id: <uuid>`. Send `notifications/initialized` then
  `tools/call` with header `mcp-session-id: <uuid>`; replies are SSE frames (`data: {json}`).
  A guard refusal on `fetch_and_index` surfaces as a JSON-RPC `error` object
  `{"code":-32602,"message":"blocked by egress guard: ..."}` — not a tool-level `isError`.
- stderr log stays empty; request evidence is in `$CAUCE_DATA_DIR/logs/cauce-YYYY-MM-DD.jsonl`
  (`path`, `client`, `method` per request — shows the `ui` POST /api/click and POST /api/pages
  pair ~5 ms apart on a click).

## Devin Secrets Needed

None — all local, no auth.

## Verified runtime details (JS-bundle refactor era)

- In **debug builds**, rust-embed reads `crates/cauce-server/assets/` from disk at
  runtime — a rebuilt `assets/app.js` needs no `cargo build`; Askama templates and Rust
  code DO need one. Quick embed check: `curl localhost:PORT/ | grep -o cauceEventSource`.
- Deterministic `/answer` e2e without cassettes: pin `CAUCE_ENGINES=replay` (synthetic
  results cover any query) and stub the provider so `POST /chat/completions` returns
  `sse_toolcall.raw` on the first call and `sse_answer.raw` thereafter — exercises the
  full step→tool-search→sources→delta→done loop in the UI.
- Clipboard verify without permissions prompts: click a `.request-id`/copy control, then
  Ctrl+V into a visible input — the pasted text proves `navigator.clipboard.writeText`
  fired. The `copy-json` link also flashes the `data-copied-label` text for ~1.5 s.
- Engines-page i18n error path: the "Run" test form hx-GETs `/api/search`; a second
  serve instance with `CAUCE_REPLAY_FAIL_EVERY=1` makes it 502 → `htmx:responseError` →
  `data-i18n-test-failed` text in `.test-results`. Success path returns a results fragment.
- UI canary for "bundle didn't run": submitting the search form must land on
  `/search?q=…&stream=1` — a dead bundle falls back to a plain GET `/search?q=…`.
