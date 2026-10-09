---
name: testing-cauce-serve
description: How to spin up isolated `cauce serve` instances for e2e testing — config/env conventions, replay-engine fault injection, port/dir layout, and UI/API check tactics. Includes SPA-era (post-FX-06) UI testing notes.
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
- **Backgrounding**: `setsid env … ./cauce serve … &` from a one-shot exec can get reaped
  when the command's process tree exits. For a serve that must outlive the command, run it
  in a persistent TTY shell or with `nohup … & disown` in a reused `shell_id` session.

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
  config or `CAUCE_SEARCH_*` env (env also greys the /app/settings input + shows a "set by …"
  hint). Defaults: min_results=5, floor=300, ceiling=1500, deadline=3000.
- The tier-1 latency histogram (P90 hedge point) is **in-memory**: restart resets it → the
  first uncached search hedges at the floor; afterwards P90≈latency → hedge point clamps to
  the ceiling when latency > ceiling. A late-fired t2 whose remaining deadline < latency
  gets clipped (`failed: timeout`, `deadline_hit:true`); enough clips open its breaker and
  the hedge then reports `engines_skipped` instead of `hedged` — that is intended fire-time
  gating, not a bug. A `commit_config` fan-out rebuild (see `/app/settings` hot-apply) also
  resets the histogram.
- `CAUCE_SEARCH_HEDGE_FLOOR_MS > CAUCE_SEARCH_HEDGE_CEILING_MS` panics `Duration::clamp`
  per request (502 "in-flight search task vanished") — no validation exists (bug).

## SPA-era UI surface (post-FX-06)

- The Svelte 5 SPA at `/app` is the only UI. Routes: `/app` home, `/app/search`,
  `/app/answer`, `/app/history`, `/app/dashboard`, `/app/archive`, `/app/admin?tab=…`,
  `/app/settings`. Legacy `/`, `/search`, `/answer` 308 to their `/app` twins (query
  preserved); `/history`, `/dashboard`, `/archive`, `/settings` intentionally 404 — only
  the three canonical entry points redirect.
- Residual server-rendered pages that are NOT the SPA: `/answer/{id}` (persisted answer
  log — a completed ask navigates there so reloads don't re-run the loop), `/trace/{id}`
  (audit deep-links), `/opensearch.xml`, `/favicon.ico`.
- SPA asset regression to keep checking: `/app/assets/index-<hash>.js` must answer
  `text/javascript` (a `text/html` answer bricks the whole SPA — #276).
- Nav adapts at <700px: operator links collapse into a `more` disclosure.
- The displayed result count reads `engines_used[].result_count` (engine-reported) while
  rendered rows are post-merge-collapse — a "10 results" header over 9 rows is the merge's
  `collapse_same_host_after`, not a missing row bug. Cached-path renders show the
  post-collapse count.
- Ask flow: `step` frames → `Searching:` step chips, `delta` → answer text, `sources` →
  numbered source cards, `done` → status + confidence + model chip + related links; an
  ungrounded answer renders "cites no sources — it may be ungrounded" and is NOT cached.
- Click → archive: `<article>` links are `target="_blank"`; the click fires `POST /api/click`
  (clicks row) + `POST /api/pages` (archive index). Replay URLs are fabricated paths on real
  hosts → upstream 404 → nothing stored; the archive staying at 0 pages is correct.

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
- `/app/settings` form PUTs to `/api/config`; hot keys (`search.*`, `admission.*`,
  `cache.*`, `merge.*`, `health.*`, `ai.*`, `archive.*`, `ui.*`, `engines.*` incl. `tier`,
  `server.public_url`) apply on save; `server.host`/`server.port`/`auth.*`/`logs.*` report
  `requires_restart`. The save-status line reads `saved HH:MM` or
  `saved HH:MM — restart needed: <paths>`. JSON (non-form) PUTs get `applied`,
  `requires_restart`, `effective_after_restart`.
- Proving a hot engine-tier change in one process: `/app/admin?tab=engines` renders the
  **live pipeline's** effective tier — a row flipping `t1`→`t2` is the rebuild evidence.
  `meta.hedged` only goes `true` when the t=0 wave is non-empty AND a deferred engine
  exists AND `merge.map.len() < min_results` at the hedge point: replay returns 10 results
  ≥ default `min_results=5`, so the hedge self-cancels — seed `search.min_results` high
  (e.g. `100`) to see `hedged:true`. With the whole live set at tier 2,
  `promote_deferred` runs it at t=0 instead (waves.rs) and `hedged` stays `false` —
  correct, not a bug.
- Shell kills: never `pkill -f "port 4479"` — the pattern also matches your own wrapping
  shell command and kills it (same for `pkill -f stub.py` etc). Kill by PID from
  `ss -ltnp | grep :PORT`.

## AI provider e2e (`/app/answer`, `[ai]`)

- **Devin session secrets inject `CAUCE_AI_*` env vars into every shell**
  (`CAUCE_AI_BASE_URL`, `CAUCE_AI_API_KEY`, `CAUCE_AI_MODEL`, `CAUCE_AI_ENABLED`), and env
  overrides beat `config.toml`. A serve launched without explicit values silently talks to
  the secret base_url. Always set all four (plus `CAUCE_AI_PROTOCOL`) explicitly on the
  serve command — or keep them if a live-provider run is intended.
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
- Since #232 the cache gate is deterministic groundedness (≥1 source, every prose
  sentence carries an in-range `[n]`, no out-of-range cites) — the verbalized
  `confidence` score is display-only. An ungrounded answer is NOT cached, so repeat
  runs stay clean.
- Two answer modes share `/api/answer`: the tool loop sends a `"tools"` array; the SERP
  **Assist** card POSTs `{q, context_results}` with NO tools key. Assist reuses the answer
  cache keyed by query, so a second Assist click on the same query may serve instantly —
  use a fresh query per click.

### Live provider runs (real OpenRouter egress)

- `openrouter/free` (the pseudo-model) routes to whatever free upstream is healthy —
  observed `poolside/laguna-s-2.1:free` converging to a grounded answer with real source
  cards. Free-tier may still loop to max iterations, 429, or time out — a clean in-page
  error frame is acceptable evidence for provider-free environments.
- **Synthetic replay results make models keep searching.** Seed cassettes for the queries
  the model actually sends (watch the `step` frames' `query` fields on a first live run).
- **Cached answers are deterministic chip-state proof.** A grounded done writes an
  `answers` row; reloading the same `?q=` replays `done{cached:true}` with no provider
  call. The cache key includes the model.
- If the model wraps the metadata tail in a ```` ```json ```` fence, `parse_final_answer`
  strips the fence; an unrecoverable tail is dropped, never leaked (#232/#217).
- Multi-turn: follow-up probes must use a pronoun/referent (`who created it…`) so a missing
  `history` is visually obvious.

### Loop-guard / budget evidence lives in logs

- `cauce_agent::observer` events: `{"message":"tool call short-circuited","reason":
  "duplicate query"|"search budget exhausted","tool":"search_web"}`.
- Span counting recipe (jsonl, by record kind — substring grep overcounts):

      python3 -c "import json,collections
      opens=collections.Counter()
      for l in open('$LOG'):
          r=json.loads(l); sp=r.get('span') or {}
          if r.get('kind')=='span_open': opens[sp.get('name')]+=1
      print(opens)"

  Provider calls = `ai_http` opens; real searches = `pipeline.search` opens; guard hits =
  `kind:event` records with `fields.reason` = `"search budget exhausted"`/`"duplicate query"`.
- `CAUCE_AI_MAX_SEARCHES=2` run → 2 `pipeline.search` spans, N−2 `search budget
  exhausted` events; `Searching:` chip count > executed searches is the one-glance tell.
- `ai.provider_budget_s`: save `1`, then a provider call that sleeps >1 s → SSE
  `event: error` `"provider request timed out"` on the FIRST call.
- Log filename varies per instance: glob both `cauce-*.jsonl` and `cauce.*.jsonl` under
  `$CAUCE_DATA_DIR/logs/`.

## Archive / click-beacon e2e (`/api/pages`)

- `archive.index_on_click` defaults true; `CAUCE_ARCHIVE_INDEX_ON_CLICK=false` disables the
  `POST /api/pages` arm on result clicks — `POST /api/click` still writes `clicks` rows
  (easy negative test).
- The archive `Fetcher` has an **egress guard** (#189): private/reserved targets refused
  with 4xx before any connect, on every redirect hop. Loopback targets need
  `CAUCE_ARCHIVE_ALLOW_PRIVATE=true`. Schemes: http/https only.
- For a deterministic click → `pages` row, serve `crates/cauce-core/tests/fixtures/archive/`
  on loopback (`python3 -m http.server 8123 --bind 127.0.0.1 --directory
  crates/cauce-core/tests/fixtures/archive`) and hand-author a cassette whose result URLs
  point at it (`<dir>/replay/<sha8(normalized query)>.json`, Cassette schema — every field
  incl. `published:null` and `score` required; sha8 = first 8 hex of sha256(
  `" ".join(q.split()).lower()`)).
- `sqlite3` CLI is NOT installed — use `python3 -c "import sqlite3; ..."` against
  `$CAUCE_DATA_DIR/cauce.db` (`SELECT url,title,length(markdown),byte_len FROM pages`;
  `clicks` for the click beacon). `put_page` upserts.
- HTTP surface: `POST /api/pages {"url"}` → 201 `PageRow`; `GET /api/pages/{pct-enc-url}` →
  200 or not_found envelope; bad scheme → 403 `url_blocked`.

## Browser-testing gotchas (Chrome for Testing on this box)

- **Omnibox inline autocomplete hijacks typed URLs** to history suggestions — after typing
  a URL press `Delete` (strips the selected suggestion tail) before Enter, or the literal
  path never gets requested. This silently broke `/` and `/app/` probes during testing.
- **Min window width ~500 CSS px** — the WM/Chrome won't resize narrower. For ~390px mobile
  checks use DevTools device toolbar (F12 → phone icon → set width 390); verify
  `document.documentElement.scrollWidth <= innerWidth` for no horizontal overflow.
- **Console/pageerror sweep via CDP**: the box's Chrome runs with
  `--remote-debugging-port=29229`. Connect to the page target's ws URL from
  `GET :29229/json` with `suppress_origin=True` (websocket-client), enable `Runtime` +
  `Log`, `Page.navigate` through every surface, collect `Log.entryAdded`
  (level≥warning) + `Runtime.exceptionThrown`. Plain `browser_console` does NOT return
  accumulated page logs — CDP is the reliable path.
- Favicon `.ico` 404s from `icons.duckduckgo.com` are third-party rate-limit flake
  (same URLs 200 on retry), not app errors — exclude them when judging "0 console errors".
- `GET /opensearch.xml` downloads as a file in Chrome (Save dialog) rather than rendering —
  the dialog itself is the "it answers" evidence.
