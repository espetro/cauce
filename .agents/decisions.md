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
