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

## 2026-09-26 — TS migration step 1 (#212, branch v3/ts-toolchain)

- `typescript@7` is the native tsgo build (`tsc` bin works; strict `tsc --noEmit`
  is the new gate between `pnpm install` and `pnpm run build` in `[tasks.web]`).
- esbuild → rolldown: `build(options)`/`watch(options)`; the MPL banner must go
  through `output.postBanner` (runs post-minify — `banner`/`/*!` get stripped or
  mangled), target via `transform.target`. Rolldown warns on htmx's internal
  `eval`; expected.
- `htmx.org` ESM (`dist/htmx.esm.js`) does NOT assign `window.htmx` — `app.js`
  must set it before extensions register. `htmx-ext-json-enc@2.0.2` self-
  registers on import and matches the previously vendored build.
- rolldown's minifier rewrites string literals to backtick quotes — the
  `html.rs` test asserting `defineExtension('json-enc'` accepts all three quote
  forms now.
- esbuild stays in `pnpm-workspace.yaml` `allowBuilds`: vite/vitest pull it in
  transitively even though the bundler is gone.
- TS narrowing does NOT flow into hoisted `function` declarations inside a
  function body — use arrow `const` or an alias for narrowed values (hit on
  `initThemeToggle`'s `btn`).

## 2026-09-27 — TS migration step 2 (#213, branch v3/ts-wire-types)

- ts-rs 12 + serde-compat: a field is optional in TS only with BOTH
  `#[serde(default)]` and `skip_serializing_if`; `#[ts(optional)]` requires
  `Option<T>` (IsOption bound). `#[serde(transparent)]` trips serde-compat's
  parser → keep `no-serde-warnings` enabled or `clippy -D warnings` fails.
  `TS::output_path()` is always `Some`, so `export_all(&cfg)` works without
  `#[ts(export)]`; the MPL banner has to be prepended by the export test
  itself (ts-rs emits no header).
- rust-i18n 4.2: `t!` returns `Cow` whose lifetime is tied to the KEY —
  a `tr(key: &'static str) -> Cow<'static, str>` wrapper covers Askama
  templates (which can't call macros in `{{ }}`); `{{ crate::i18n::tr("m.k") }}`
  parses as a path call. `&'static str` struct fields/fn returns holding
  strings become `Cow<'static, str>`; `plural`/`&str` params take `&t!(..)`
  via deref.
- The `i18n!` proc macro embeds `locales/*.yaml` at expansion time but does
  NOT track the files — `cargo build` after an `en.yaml` edit silently
  reuses the stale catalog. cauce-server's `build.rs` prints
  `rerun-if-changed=locales`; without it tests and gen_i18n both go stale.
- YAML 1.1 bool trap in the catalog: a bare `n`, `yes`, `on`, `off`, `true`
  value parses as boolean (`col_results: "n"` rendered `false` in a `<th>`
  until quoted). PyYAML's safe_load does NOT treat `n`/`y` as bool —
  pyyaml-based audits miss it; serde_yaml does.
- serde_json `to_value` vs direct `to_string`: `to_value` round-trips f32
  through `Value::Number` (f64, e.g. `0.016393441706895828`) and emits
  BTreeMap-sorted keys; the typed `ResultsFrame` emits struct declaration
  order and f32 shortest repr (`0.016393442`). Same JSON values — page/
  SSE parity checks must normalize these.

## 2026-09-27 — TS migration step 3 (#214, branch v3/ts-stream-modules)

- Rebased onto main mid-task when PR #216 (W7-04 threads) landed: it had
  already migrated `answer.js`→`answer.ts` itself, so the rebase conflict
  resolved as "theirs + re-apply our deltas" (shared `parseSseFrame`/
  `pumpSse`, `SseFetch`, `AnswerStrings`, `var SA`). Check whether an
  in-flight PR touches your target files before starting a migration.
- `var S` on `/answer` was renamed to `var SA` (template-only) so each
  Window global keeps one literal i18n shape — no union-narrowing needed.
- `window.fetch` can't be handed wholesale to a `SseFetch`-typed param —
  a callable `interface` (`(input, init) => Promise<SseResponse>`) is the
  minimal surface tests can fake (status/json/body) without DOM Response.
- vitest `vi.fn()` bare types `mock.calls` as `any[]`; give the impl real
  param types (`(input: string, init: RequestInit)`) so `calls[0][1]`
  destructures typed. `init.headers`/`init.body` still need `as
  Record<string,string>`/`as string` at the use site.
- tsconfig here has no `exactOptionalPropertyTypes` — `{value: undefined,
  done: true}` satisfies a fake ReadableStream reader chunk.
- `pkill -f "cauce serve"` kills the wrapping shell too — kill serve
  instances by PID (ss -ltnp), as the serve skill already warns.

## 2026-09-28 — answer markdown server-side (#226, branch v3/answer-markdown)

- ammonia 4.2 footguns: listing `rel` in a tag's `tag_attributes` while the
  (default-on) `link_rel` is set panics — omit `rel`, ammonia applies it to
  every `a[href]`. Same panic for `class` in `tag_attributes` +
  `allowed_classes` on the same tag. `url_relative` defaults to PassThrough,
  which is what keeps `#cite-n` fragment hrefs alive.
- ammonia can't add `target` — emit markdown link opens as raw
  `Event::InlineHtml` before sanitize; ammonia still validates the href scheme.
- The `web` task's freshness gate (`git diff --exit-code` on app.js / wire
  types / i18n) compares worktree vs index — it only passes AFTER the
  generated artifacts are staged/committed. Commit first, then
  `mise run validate` (matches the pre-push-hook flow it mirrors).
- `CAUCE_AI_MODEL=openrouter/free` cannot converge the answer tool loop
  (tool call every iteration → max-iterations). e2e needs a real model
  (`openai/gpt-4o-mini` worked); the change itself is model-agnostic.

## 2026-09-28 — Hot settings apply (#225, branch v3/hot-settings)

- `AppState` now owns a `Runtime` (pipeline + answer loop + archiver) behind
  `Arc<RwLock>`, rebuilt wholesale on save inside the config critical section;
  handlers clone `Arc`s out — in-flight requests finish on the old runtime.
- `engine_factory` is injected (`with_engine_factory`) so cauce-server keeps no
  cauce-engines dep; absent → `engines.*` changes report restart-required.
- The shared `Arc<HealthTracker>` survives pipeline rebuilds
  (`with_health_tracker` + `set_policy`), so EWMA/breaker state carries over;
  the per-tier latency histogram does not — first post-save search hedges at
  the floor.
- MutexGuard can't split-borrow disjoint fields (`inner.map` + `inner.policy`) —
  clone the POD `HealthPolicy` out before `inner.map.entry()`.
- `changed_config_paths` granularity is the changed TABLE: a brand-new `[logs]`
  section diffs as `logs`, not `logs.retention_days`. Seed the section in
  configs when a test asserts the leaf path.
- Single-engine fan-out moved to t2 → `promote_deferred` runs it at t=0 and
  `meta.hedged` stays false (correct). `hedged:true` needs a live t1 engine +
  a deferred one AND `merge.map.len() < min_results` at the hedge point —
  replay returns 10 ≥ default 5, so seed `search.min_results` high (~100).
- `CAUCE_AI_*` env vars are provisioned in Devin shells — `env -u` all four
  when the e2e needs a user-submittable `ai.enabled` on /settings.

## 2026-10-05 — Eval calibration (#234, branch v3/eval-calibration)

- `groundedness` moved to `cauce_core::ai::grounded` so `score_frames`
  runs the identical predicate the `answers` gate applies —
  `cauce_agent::groundedness` is now a re-export, callers unchanged.
- `score_frames(case, frames, observed_cache_write)`: the third arg is
  the runner's post-run `answers` probe, consulted only when the case
  asserts `expect_cache_write`; `cache_rate` then folds into the score
  as a fourth component (the mean stays three-way otherwise). Outcomes
  carry `verbalized_conf`/`grounded`/`cache_write` — the calibration
  triple — skipped when the run never reaches `done`.
- The report's `calibration` block scores every `conf >= t` (1..=10)
  against groundedness and suggests the least restrictive max-agreement
  cutoff; `[ai].cache_min_confidence` in evals/thresholds.toml is the
  derived, informational value a future `[ai].verify` reads — it never
  gates.
- Corpus finding: a grounded answer scored conf 2 and ungrounded ones
  claimed conf 6-9, so the derived cutoff is 1 — v2's hardcoded
  CACHE_MIN_CONFIDENCE=4 would have refused a grounded write. Live
  gpt-4o-mini confirmed the same skew: conf 9 on every run, zero
  grounded answers (`[1, 2]` and markdown links are not `[n]` cites).
- Loop-guard transcripts are hand-authored like the rest: exhaustion
  needs `max_turns` tool_call turns (8) + the forced-synthesize answer
  as turn 9; dedup/`search budget exhausted` resolve in-band so their
  Step chips still need a cassette on disk to avoid the no-cassette
  note.
- `cauce eval ai evals/ai/loop.jsonl --tag loop --gate` is the new-case
  e2e; run the harness from the repo root or the default
  fixtures/transcripts dirs won't resolve.

## 2026-10-09 — FX-07 instance modes (branch v3/fx-07-instance-modes)

- `Capabilities`/`InstanceInfo` wire types (ts-rs): `{mode, role, flags:{admin_surface, server_history, archiving, shared_stats}}` + `index_on_click`, `admin`, `engine_ids`. `GET /api/capabilities` resolves role from `Authorization: Bearer` against `[auth].admin_tokens`; anonymous = `user`.
- Server is authoritative: `[auth].admin_tokens` (list), `[server].public_instance`, `[archive].enabled`; `require_admin` middleware 401s (no/unknown token → `unauthorized`) on every `RouteAuth::Admin` row — UI gating is only UX.
- SPA `caps` store (`lib/capabilities.svelte.ts`): defaults are LOCAL-mode (all flags on) until the bootstrap fetch settles — features MUST `await loadCapabilities()` before branching on flags, and chrome gating keys on `capabilities.loaded &&` so nothing renders open pre-resolution.
- Public mode: `search_log`/`clicks` are never written server-side (probed via sqlite in tests); the SPA writes the same row shape to localStorage (`localHistory.ts`). Archive splits shared content (`GET /api/pages/{url}` open) vs per-user index (localStorage); `archiving=false` drops `/archive*` rows + shows the disabled arm.
- `/app/admin` merged tabs (instance · engines · cache · audit): ops telemetry lives there; `/app/dashboard` public mode shows only the instance card.
- Threat model kept honest: config PUT can't flip `public_instance` live (restart required — test `config_put_cannot_demote_instance_mode_live`); role comes only from the bearer token, never from client state.

## 2026-10-09 — FX-06 teardown (branch v3/fx-06-teardown)

- The `/app` SPA is the ONLY UI layer: `askama`, `templates/`, `src/html/`, `*_page.rs`, vendored htmx + the `web/build.mjs` pipeline are gone. `Cargo.toml`'s `ui` feature = `rust-embed` + `urlencoding` only.
- Routes: `/`, `/search`, `/answer` 308-redirect to `/app` twins (query preserved); `/app` + `/app/{*rest}` serve the embedded bundle (`src/spa.rs`). Residual server pages `/trace/{id}` and `/answer/{id}` re-implemented without askama in `src/pages/` (shared `doc()` shell + inline token CSS) — they have no SPA/JSON twin (trace) or are the durable stored render (answer).
- `src/assets.rs` owns favicon + `opensearch.xml` (results URL now `/app/search?q=`); the SPA `index.html` carries the `<link rel="search">` + a token-styled `<noscript>` "JS required" shell (§7.5's cheap option).
- SPA entry points for future sessions: `web/src/spa/` (app shell, `lib/api.ts` client + generated `web/src/types/*`, `lib/capabilities.svelte.ts`, `features/*` slices); i18n via generated `web/src/i18n/*.json`.
- Form-encoded `PUT /api/config` stays — the SPA settings tab still PUTs urlencoded dotted-path config.
