# Agent memory process (cauce)

Project-scoped agent memory with time-bounded recall ("dreaming"). Per the repo owner's global
memory policy (`~/MEMORY.md`), memory lives inside this repo under `.agents/`, never in a
global store, and is never shared with or copied into another project.

```
.agents/MEMORY.md          this file: process definition + compaction log (committed)
.agents/notes/              durable dated findings (committed)
.agents/docs/                permanent docs, incl. screens/ (committed)
.agents/drafts/              raw intermediate research (gitignored)
.agents/plans/                implementation plans (tracked here — see root .gitignore's
                              `!.agents/plans/` override)
```

## Process

1. **Session start**: read this file, then skim `.agents/notes/` newest-first for anything
   relevant to the task at hand.
2. **Session end** (or before context compaction): write durable findings as
   `.agents/notes/YYYY-MM-DD-<topic>.md`. If a new note supersedes an older one, mark the old
   note with `Superseded by <note>.` at its top rather than deleting it outright.
3. **Periodic dream** (notes directory > ~15 files, or each milestone): merge same-topic note
   series into one note, delete notes superseded more than one milestone back, archive stale
   `.agents/drafts/` and `.agents/plans/` entries, and update the compaction log below.

## Compaction log

- **2026-09-17 — reset.** `main` was orphaned from the 54-commit `feat/v0.4.0-web-ui`
  exploration (now `legacy`; see `.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md`, Wave 0).
  `legacy`'s `.agents/MEMORY.md` held four dated entries of v0.4.0 implementation detail (AI
  pipeline internals, shipped-feature summary, P0 fixes, a six-issue UI audit) plus one
  2026-09-17 entry with the v0.5.0 planning research and a velocity retrospective. The
  implementation-detail entries describe code that no longer exists in this tree and were not
  carried forward. The retrospective findings were carried forward into
  `.agents/notes/2026-09-17-velocity-retro.md` — read that note for what the last attempt's
  cost actually was (11,565 lines of retrofit churn, zero CI runs, the specific bugs that
  motivated each new gate) before re-deriving any of it from scratch.
- **2026-09-21 — v3 restart.** `main` (v2) branched to `v2-legacy`; `main` restarted as an
  orphan for the Rust v3. `.agents/` carried over whole: the v2 screen specs under
  `docs/screens/` are now requirement input for the HTMX pages, not designs; the two UI-loop
  notes and the velocity retro stay as evidence. New plan: `plans/2026-09-21-v3-rust-core.md`;
  subplans per wave under `plans/v3/`.
- **2026-09-22 — maintenance-area review applied.** `serde_yaml` to `serde_norway`; metrics
  are an owned registry rendering Prometheus text (OTLP opt-in, `otlp` feature now
  non-default); W3-04, W5-04/05, W6-02/03/04 deferred to `plans/v3/later/`; W3-06 is a
  failing-canary signal only; W2-08 drops Playwright; #84 becomes W1-13 loopback guard;
  W4-05 Anthropic kept per owner. Full entry in `decisions.md`.

## 2026-09-22 — wave-1 cutover done

- v3 supervises under oxmgr as `cauce` on 4479 (`/Users/josocjoq/.cargo/bin/cauce serve`), portless alias `search.localhost`.
- Gotcha: `CAUCE_ENGINES` pin cannot name embedded specs (bing/brave/wikipedia); it validates against `[[engines]]` + builtins only. Specs auto-register when the pin is unset. ddgs disabled via `enabled=false` entry (exec needs repo cwd + uv venv). Docs fixed in #117.
- rmcp `StreamableHttpService` Host allowlist was exact-match only; `*.localhost` aliases needed our own handling (PR #116). `/mcp` exact path; `/mcp/` 404s.
- Verified live: singleflight collapses 3 concurrent identical queries to 1 upstream call; `/metrics` exposes engine histograms + breaker state + admission wait; `/api/stats` mirrors. `search_web`+`exa_search` green via MCP.
- #90: decided strict 400 naming unknown ids (reversed the earlier won't-fix recommendation after UX research — SearXNG silently widens on fully-unknown pins, industry norm is fail-loud-with-names). Issue commented with acceptance.
- SearXNG UX-complaint research (`/tmp/oxe-searxng-ux-research.md`) applied to the v3 plans: W3-02 cache hygiene, W2-01 engines_skipped/failed rendering, W2-03 outcome label, W3-06 page-2 canary checks, new W3-07 degraded breaker (#119), empty→NoResults bug (#120); compat shim + param forwarding deferred to later/ (#121, #122).
- Rename decided: project oxe → cauce, CLI `cauce`; rename step #123 scheduled in It 3 before W2 Batch I dispatch.

## 2026-09-23 — W3-01 hedge landed (PR #148)

- Hedge design settled: tier-1 + tier-3 are the t=0 wave; tier-2 is deferred and
  breaker-gated only at fire time (gating at t=0 would leak `probe_in_flight` claims
  for probes that never run). All-primary-empty promotes the deferred set to t=0
  instead of `BreakerOpen`.
- `gate_waves` returns the hedge point as `clamp(p90 pooled over runnable tier-1
  rolling windows, hedge_floor_ms, hedge_ceiling_ms)`; empty history → P90=0 →
  floor (300 ms). Both `fetch` and `fetch_stream` share `drive_fan_out`: fold each
  outcome inline as it joins, incremental `RrfMerge`, fire when merged <
  `min_results`, early-fire (`reason=few`) once all primaries answered short,
  cancel permanently once merged ≥ min_results.
- Bonus from the inline fold: `record_ttfr` on the collect path now measures the
  real first answer instead of the join barrier.
- `meta.hedged`/`meta.hedge_at_ms` are per-request like `engines_skipped`: set
  only when ≥1 tier-2 actually spawns (all-skipped at fire time = no hedge), and
  reset on cache-hit rebuilds.
- Replay now honours `[[engines]]` `id`/`tier` (was hardcoded `replay`/T1 with a
  warning) — needed to run a tier-2 replay beside the tier-1 one for hedge tests
  and future hedge e2e.
- Tooling: no mise on the box (`mise run validate` unavailable); system rustc
  1.97.1 + rustfmt + clippy work; cargo-deny/cargo-nextest absent — CI covers
  them. `git config` is blocked in this environment so `.githooks` can't be
  installed; run `cargo fmt`, `cargo clippy --all-targets -D warnings`,
  `cargo test --workspace` manually.
- Project board: token still lacks Projects write; WIP note went on the issue
  as a comment instead (#44).

## 2026-09-23 — W3-01 review fixes (PR #148, 9d4f0b5)

- Devin Review on the hedge PR found 4 real bugs; all fixed + threads resolved,
  flags got assessment replies (kept raw-sample P90 — the settled "EWMA history"
  means the rolling window, not re-percentiled averages; no `promoted` reason —
  `hedged:false` + `engines_used` already distinguishes it).
- Admission permits now match what actually spawns: callers acquire only the
  t=0 (non-T2) wave; promoted tier-2 acquires inside `fetch`/`fetch_stream`
  (`promoted_permits`, `min(remaining, max_wait)`); a triggered hedge acquires
  inside `drive_fan_out` via a spawned `acquire_within` task raced against
  outcomes. `RateLimited` from fetch falls back to `overflow` like an
  exhausted primary queue.
- Hedge clock moved to `fan_started` (drive_fan_out entry): pre-fan-out work
  (lexical, permit wait) no longer eats the floor; `hedge_at_ms` measures
  from fan-out too. `ctx.started` still anchors the hard deadline.
- Late hedges are cancelled: wake capped at `min(fan_started+at, hard_deadline)`,
  and `queue_hedge` returns early on zero remaining budget BEFORE `breaker_gate`
  (a claimed probe must always precede a spawn).
- P90 pools `waves.gated`, not `runnable` — a skipped engine's history can't
  delay the hedge.
- Gotcha worth remembering: `tokio::select!` evaluates EVERY branch's async
  expression even for `if`-disabled branches — `.as_mut().unwrap()` in a
  select expr panics on `None`; use a match that returns `pending()`.
- Test support: `DialEngine` (dialable latency so history ≠ current behaviour)
  and `StubStore.lexical_delay_ms` added in tests/support/mod.rs.
