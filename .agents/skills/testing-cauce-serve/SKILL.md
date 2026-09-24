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
  shell command and kills it. Kill by PID from `ss -ltnp | grep :PORT`.

## Devin Secrets Needed

None — all local, no auth.
