# oxe

Metasearch for humans (web UI) and agents (HTTP API, MCP) with a shared TTL cache. v3 is a
Rust rebuild; the authoritative plan is `.agents/plans/2026-09-21-v3-rust-core.md` and its
per-wave subplans under `.agents/plans/v3/`. This file is a summary, not a replacement.

## History

- `legacy` branch: v1 (Python, PyPI 0.4.0).
- `v2-legacy` branch: v2 (Python FastAPI + React SPA, abandoned 2026-09-21). Reusable as a
  reference for wire shapes (SearXNG JSON, Exa adapter, MCP tool names) and for the retro in
  `.agents/notes/2026-09-17-velocity-retro.md`. Do not lift code from it into v3.
- `main`: v3, orphan history started 2026-09-21.

## Project tracking

GitHub Project: https://github.com/users/espetro/projects/23/views/1

All work is linked to a refined task there (EPIC per wave, one issue per subplan step) before
implementation starts. A task is refined when it has Iteration, effort (S/M/L/XL), start and
target dates, and a classification label (`feature` / `bug` / `cosmetic` / `infra` /
`documentation`). Use `ghx` to manage it.

## How to pick up a step

Full procedure: `.agents/plans/v3/README.md`. Short form:

1. Pick an issue titled `W<n>-<nn> ...` whose dependencies are `Completed`; move it to `WIP`.
2. Read the parent plan sections 4 to 7, the wave file's "Settled inputs", then the step.
3. Worktree `~/.worktrees/oxe-<step-id>` on branch `v3/<step-id>-<slug>` from `main`.
4. Implement only the step's "Do"; its "Acceptance" list is the PR's test list; the golden
   path stays green; commits signed off (`-s`).
5. PR `Closes #<issue>`; on merge, move to `Completed` and read the step's "Follow-up".

If a step cannot be done without changing a settled contract, stop and comment on the issue
with the proposed amendment to the parent plan. Do not improvise the contract.

## Layout (target, see the plan's section 4.1)

```
crates/oxe-core          domain types, pipeline, scheduler, Store + Engine traits (MPL-2.0)
crates/oxe-store-sqlite  rusqlite Store impl (MPL-2.0)
crates/oxe-engines       declarative / exec / replay engine runtimes (MPL-2.0)
crates/oxe-server        axum, HTMX templates, SSE, MCP, Exa adapter (MPL-2.0)
crates/oxe-cli           oxe serve / search / engine test / cache / record (MPL-2.0)
engines/                 YAML engine specs + fixtures (Apache-2.0)
sdk/python               exec-protocol SDK + ddgs reference engine (Apache-2.0)
tests/e2e                golden path integration tests
.agents/                 plans/, decisions.md, MEMORY.md, notes/, docs/
```

## Modes and budgets

One binary: `oxe serve` (full: UI + API + MCP, < 80 MB idle), `oxe serve --headless` (API +
MCP, < 50 MB), `oxe mcp` (stdio only, < 40 MB). Cargo features `ui mcp ai archive semantic
postgres otlp`; defaults are `ui mcp ai` (`otlp` is non-default). Port 4479, loopback by
default.

## Enforced

Nothing yet. Gates land with wave 0 (`.agents/plans/v3/wave-0-skeleton.md`) and are tagged
here the same way v2 did: every normative line carries a `[gate: <task or path>]` marker that a
test resolves.

## Conventions

- Conventional Commits, atomic commits, no AI attribution trailers.
- Plans live in `.agents/plans/`; a plan exists on disk before implementation begins.
- Agent memory: read `.agents/MEMORY.md` at session start, write dated notes at session end.
- Verification policy is the plan's section 7: golden path on the `replay` engine first,
  live canaries gated behind `OXE_LIVE=1`, no fixture-only UI states, every mounted route in
  the routes table.
