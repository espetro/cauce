# Decisions log

Running log, one entry per locked decision: decision, one-line rationale, date. Append, don't
rewrite history — if a decision is later reversed, add a new entry that supersedes the old one
rather than editing it away.

- **Component layer: daisyUI 5 for looks, `@base-ui/react` for behavior.** Splits the "looks vs.
  focus/keyboard/portal" concern into exactly one owner each, so there's never a question of
  which layer a given fix belongs in. — 2026-09-17
- **State: URL first, then other non-opaque state, then `useReducer` FSM, then
  `@xstate/store`.** Names one approved rung per kind of state instead of leaving React's six
  ways to hold state all on the table. — 2026-09-17
- **Server state: TanStack Router `loader()`; SSE streams into a rung-3 reducer.** Keeps data
  fetching out of `useEffect` entirely, including the streaming case. — 2026-09-17
- **History: `git branch -m main legacy`, keep the `v0.4.0` tag and GitHub release, orphan a new
  `main`.** Treats the 54-commit exploration branch as exploration rather than rewriting it in
  place; PyPI 0.4.0 stays installable and referenceable. — 2026-09-17
- **Search contract: SearXNG shape is the only canonical surface; Exa HTTP and MCP are thin
  frozen adapters over it.** Exa's shape is externally owned and frozen at its published schema,
  so it can't drift under us; SearXNG's richer shape is where new capability actually goes.
  — 2026-09-17
- **Python: basedpyright strict, no bare `dict` across module boundaries, no broad `except`
  outside one handler.** Mirrors the frontend's state-ladder narrowing for the backend, where
  Python's own permissiveness was the actual defect source (61 bare dicts, 49 broad excepts on
  `legacy`). — 2026-09-17
- **Docs: every normative line carries an enforcement tag, and a test asserts the tag
  resolves.** The retro's central finding was that documented rules drifted while scripted
  rules didn't; this makes "documented" and "enforced" two different, both-visible states
  instead of one that pretends to be the other. — 2026-09-17
- **`@xstate/fsm` is retired; the store dependency is `@xstate/store`, not `@xstate/fsm`.**
  Corrects an earlier sketch of the stack — `@xstate/fsm` has no active successor path of its
  own, `@xstate/store` is the maintained replacement for the theme/toast-store use case.
  — 2026-09-17
- **The Base UI dependency is `@base-ui/react`, not `@base-ui-components/react`.** The latter
  package name is frozen at `1.0.0-rc.0` and unmaintained under that name; `@base-ui/react` is
  the actively released package (1.8.0 as of this plan). — 2026-09-17
- **v2 archived; every v2 stack decision above is void.** `main` (Python FastAPI + React SPA)
  was branched to `v2-legacy` and `main` restarted as an orphan history for the Rust v3. The
  daisyUI/Base UI, state-ladder, TanStack loader, Python strictness and doc-tag decisions above
  describe a tree that no longer exists on `main`; only the doc-tag idea carries forward as a
  convention. Rationale and the new locked decisions: `.agents/plans/2026-09-21-v3-rust-core.md`
  section 3. — 2026-09-21
- **v3 core is Rust, single binary; web UI is server-rendered HTML + HTMX + SSE.** Owner chose
  the slower path for a modular SearXNG alternative; single-binary, embedded plugin runtime and
  sub-80 MB footprint are natural in Rust, and a server-rendered UI removes the codegen/drift
  pipeline that cost v2 a third of its effort. Reverses the 2026-09-15 "don't migrate" record.
  — 2026-09-21
- **License: MPL-2.0 for core crates, Apache-2.0 for `engines/` and `sdk/`.** File-level
  copyleft on the core, permissive on the parts contributors and enterprises embed. LICENSE
  file switch is pending the copyright-line decision (plan section 11). — 2026-09-21
- **Storage: SQLite (bundled rusqlite, FTS5) behind a `Store` trait; Postgres as the second
  impl.** libSQL is superseded by the pre-1.0 Turso Database, pg0 spawns a daemon, vector-native
  stores are overkill at 5k-100k rows. — 2026-09-21
- **Providers: Bing + Brave HTML natively, `ddgs` bridged through the `exec` engine, `replay`
  as the deterministic engine.** DuckDuckGo HTML fails 4/5 queries from a residential IP and
  cannot page; Bing and Brave were the reliable ones measured on 2026-09-21. — 2026-09-21
- **`Source::Cache.ttl_s` is the remaining TTL** (seconds until expiry at serve time), not the
  TTL the entry was written with; the `response.rs` doc comment was stale and is corrected in
  W0-08. Matches the W0-10 UI line ("ttl 58 min"). — 2026-09-22
- **`EngineError::NoResults` is an answer, not a failure.** A fan-out where at least one
  engine answered (`Ok` or `NoResults`) yields a 200-shaped `SearchResponse`, possibly empty;
  `AllEnginesFailed` requires every engine to error/timeout/panic. Kills the v2 "page 2 always
  502" defect; `engines_used` still reports `Failed(NoResults)` for honesty. — 2026-09-22
- **Store errors degrade, never fail a search.** `get_exact` failure → warn + treat as miss;
  `put` failure → warn + serve; `log_search` failure → warn only. `/health` (W0-09) owns
  store-failure surfacing. — 2026-09-22
- **Exec engine cancellation contract: a dropped/cancelled `search` must not leave a pending
  response in a reused child.** `ExecEngine::search` takes the `ChildIo` out of `state` for
  the round trip and only puts it back on a fully decoded success; every other path drops it
  and `kill_on_drop` reaps the process. Protocol v1 has no request-correlation field, so a
  stale line would decode as the next query's answer — v2 should add a correlation field
  (follow-up issue #88). — 2026-09-22
- **Partial-unknown engine pins truncate, not 400.** `?engines=replay,nosuch` runs against the
  known subset for wave 0; only a pin matching nothing at all is 400 `unknown_engines`. A
  400-with-unknown-list response is deferred — this is a wire-visible semantic, revisit
  deliberately. — 2026-09-22
- **Config precedence: CLI flags > `OXE_*` env > config file > defaults.** One ordering, no
  per-subcommand exceptions; `oxe record` resolves `--engine` through `Config::load()` like
  `serve` does. — 2026-09-22
- **The JSONL observability layer has a fixed `info` floor independent of
  `RUST_LOG`/`OXE_LOG`.** The env filter scopes the stderr and OTLP layers only; the JSONL
  file is `oxe trace`'s only input, so an env-set `warn` must not silently empty it.
  — 2026-09-22
- **`requires` strings on ROUTES are runtime mount gates, not cargo features.** `RouterOptions`
  decides what mounts; real `ui`/`mcp`/`ai` feature stripping is deferred to the wave that
  measures the headless/MCP memory budgets. — 2026-09-22
- **Maintenance-area review applied to the v3 plan.** Engine specs move from the archived
  `serde_yaml` to `serde_norway`; metrics become an owned in-process registry rendered as
  Prometheus text on `/metrics` instead of the OTel SDK + `opentelemetry-prometheus` path
  (which is discontinued), while OTLP export stays opt-in behind a now non-default `otlp`
  feature. W3-04 per-client fairness, W5-04/05 semantic tier, and W6-02/03/04
  Postgres/auth/deploy docs are deferred to `v3/later/` stubs; W3-06 shrinks to a failing
  nightly canary (the failed run is the report); W2-08 drops Playwright baselines for a
  DOM assertion; #84 expands into W1-13, a loopback Host/Origin guard plus refusal to
  start on non-loopback bind; keyed-API engine specs are added as a `later/` escape hatch.
  The Anthropic Messages protocol (W4-05) is KEPT per the owner: supporting both protocols
  is adoption-critical since not every user runs Bifrost. Source:
  `/tmp/oxe-maintenance-area-review.md`. — 2026-09-22
