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
