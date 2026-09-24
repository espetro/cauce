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

## 2026-09-24 — W3-01 review round 2 (PR #148, e85f6a3)

- Second Devin Review round found the deferred gate-before-wait leaked
  `probe_in_flight`: `admission` claims the HalfOpen probe at gate time and only
  a spawned task releases it (`probe_guard`/`record_*`), so a timed-out or
  aborted permit wait left the engine permanently unprobeable.
- Fix direction (the reviewer's alternative): acquire BEFORE gating.
  `gate_waves` now gates only the t=0 wave; `promote_deferred` acquires then
  gates (RateLimited never claims); `queue_hedge` only spawns the acquire and
  `finish_hedge` gates at fire time after a `remaining.is_zero()` recheck
  (also fixes expired hedges spawning zero-budget calls).
- Regression-test trick: `pipe.health().record_err(id, .., EngineError::Blocked,
  request_id)` opens a breaker directly — the only way to have Open + all
  permits held, since every permit-holding call also gates (a held permit can
  never coexist with an Open breaker through engine calls alone).
- Singleflight gotcha: pinned occupier searches with the SAME query dedupe
  into followers — only one holds the permit. Distinct queries per holder.

## 2026-09-24 — W3-08 pipeline/test-surface decomposition

- `pipeline.rs` (2466 LOC) → `pipeline/` module dir, same `impl
  SearchPipeline` blocks split across files. Splitting an `impl` across a
  module dir works because child modules see the parent's private fields —
  the only visibility changes needed are `pub(super)` on items shared
  between siblings (types `Gated`/`Waves`/`FanOut`/`FetchCtx`/`StreamCtx`
  and every method a sibling calls).
- Map: `mod.rs` public API + admission/singleflight/leader-follower;
  `cache.rs` tier-1/2 lookups + stale overflow + background refresh
  (W3-02's surface); `waves.rs` breaker gate + primary/deferred split;
  `fanout.rs` spawn/hedge/deadline; `merge.rs` `RrfMerge` (W3-03's
  surface); `persistence.rs` response shaping/`put`/`search_log`.
- tests/support/mod.rs → `engines.rs` (Gate/Dial), `store.rs` (StubStore),
  `requests.rs` (req/replay_at); `pub use` re-exports mean zero call-site
  churn — needs `#[allow(unused_imports)]` since each test binary compiles
  support separately and uses a different subset.
- `scripts/loc-report.sh` + `mise run loc` + a `loc report` CI step:
  non-gating totals/largest-files/largest-functions report (thresholds
  >1000 review / >1500 decompose / fn>150 extract). First run already flags
  config.rs at ~1.9k LOC as the next candidate.
- Test-shape adoption: the 13 `interpolation_*` config tests became one
  `InterpCase` table; `merge.rs` gained a shift-left proptest pinning RRF
  completion-order independence (W3-03's property at the new boundary).

## 2026-09-24 — W3-08 follow-through: repo-wide hygiene batch (PRs #162–#169)

- Same privacy-boundary split applied repo-wide, all zero-behaviour PRs:
  `handlers.rs` → `handlers/` (#162), `html.rs` → `html/` by page domain (#163),
  Exa wire adapter + error mappers out of `mcp.rs` (#164),
  sqlite `store.rs` → `store/` with thin delegating `impl Store` (#165),
  `config.rs` (1.9k) → `config/` (mod facade + tree/redact/interpolate/resources +
  `tests_support`) (#166), `conformance.rs` → `conformance/` with `seed_*`/`verify_*`
  helpers, no fn >150 (#167).
- `cauce-server/tests/support/` now exists like core's — shared `state`/`http`/`env`
  harness; each test binary still compiles support separately so `pub use` re-exports
  need `#[allow(unused_imports)]`. `tests/routes.rs` split into routes_table/sse/
  host_guard/config_api. `FieldCase` table in settings; `search_stream_*`/`history_page`
  clusters stayed named (divergent setups — don't force-fit tables).
- Proptest findings worth remembering: SQLite truncates bound text at embedded `\0`
  in `LIKE`/`MATCH` args — no escaping can carry a NUL through; tests exclude `\0`
  rather than "fix" it. The SSRF http(s)-only pin lives in `parse.rs::resolve_url`,
  not in `redirect.rs` — redirect unwrap only declines on host/path miss.
- Parallel-dispatch gotcha: org SWE-2 cap is 7 concurrent sessions (the parent
  counts); terminate settled children to release slots before spawning more.
- Case-table pattern that worked: `CfgCase`/`InterpCase`-style
  `{env, file, want}` rows in config tests; `FieldCase {name, form_body, want_status,
  want_error_substr}` for per-field form errors.

## 2026-09-24 — W3-02 stale-while-revalidate

- SWR lives pre-admission in `run`/`run_stream` (after `runnable`+`ttl`,
  before `admission.enter`): `stale_lookup` (`store.get_cache` +
  `expires_at <= now <= expires_at + stale_grace`) serves
  `Source::Cache{stale:true}` then `spawn_refresh` dedupes the refetch
  via the same singleflight the W1-07 overflow path uses. Serving before
  `enter` means a stale-served request never joins the watch channel —
  and W1-07's overflow serve is untouched (it keeps its own `admission`
  reason).
- `cauce_stale_served_total` is now `{reason}`-labeled
  (`grace|admission|engines_unhealthy`); the scalar
  `AdmissionStats.stale_served` aggregate still feeds `/api/stats`.
- `HealthTracker::peek` (read-only `admission`) is what the
  `engines_unhealthy` check needs: calling `admission` there would claim
  a `HalfOpen` probe that is never followed by a call, leaking the slot.
- `Store::evict_expired(grace)` takes the window as a param; every caller
  (periodic task, `DELETE /api/cache?expired=true`, MCP `cache_invalidate`)
  passes `cache.stale_grace_s` — expired-but-servable rows are never
  garbage. The `cache_page` expired-delete test now asserts `removed: 0`
  for an in-grace row.
- Hygiene in `finish_fetch`: `resp.results.is_empty()` short-circuits
  `put` (two pre-W3-02 tests asserting "empty is cached" were updated);
  `Failed`/`deadline_hit` fan-outs cap the stored TTL at
  `cache.degraded_ttl_s` (`ctx.ttl.min(degraded)`).
- `CacheConfig` needed a manual `Default` impl once nonzero defaults
  (`stale_grace_s` 6 h, `degraded_ttl_s` 60) landed; new keys get
  `CAUCE_CACHE_STALE_GRACE_S`/`CAUCE_CACHE_DEGRADED_TTL_S` env overrides.
- UI: `strings::search::STALE_BADGE` ("stale · refreshing") is shared by
  the SSR badge arm and the streaming page's `S` bundle (JS checks
  `meta.source.cache.stale` before falling back to `S.cached`).
