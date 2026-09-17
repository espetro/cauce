# oxe

Local web-search proxy and cache for AI agents: Exa-compatible HTTP API + MCP, DuckDuckGo
backend, SQLite TTL cache. v0.5.0 is a ground-up rebuild; see
`.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md` for the full plan (authoritative — this
file is a summary, not a replacement).

## Layout

```
oxe/            FastAPI backend. See oxe/AGENTS.md for the data/error ladders.
ui/             Vite + React 19 + TanStack Router frontend. See ui/AGENTS.md.
tests/          Python tests (unit, property, contract) plus tests/e2e/ (Playwright). See tests/AGENTS.md.
.agents/        plans/, decisions.md, MEMORY.md, notes/, docs/ (screen specs).
.githooks/      pre-push -> mise run validate.
.github/        CI workflow (mise run validate on every branch).
```

Toolchain is pinned via `mise.toml`: Python 3.12, `uv`, `bun` 1.2. Backend deps managed with
`uv` (`uv.lock` committed). UI deps managed with `bun` (never `npm`/`npx` in this repo).

## Enforced

- `mise run validate` is the fast merge gate (ruff lint, ruff format check, basedpyright
  strict, pytest, oxlint, UI build+tsc) and runs in CI on every branch push, not just PRs.
  [gate: validate]
- Git hooks point at `.githooks/pre-push`, which runs the same `mise run validate` gate before
  a push is allowed. Installed via `mise run hooks:install`. [gate: hooks:install]
- `uv.lock` is committed (not gitignored) so installs are reproducible across machines and CI.
  [gate: uv.lock]
- `.agents/plans/` is explicitly un-ignored in this repo's `.gitignore` even though a global
  gitignore silences it elsewhere, so plans stay reviewable in diffs. [gate: .gitignore]
- Every `AGENTS.md` in this repo splits into an Enforced section (each line tagged with a gate
  marker) and a Conventions section (explicitly not gated); `tests/test_agents_rules.py`
  asserts every gate tag resolves to a real `mise.toml` task or an existing repo-relative file
  path. [gate: tests/test_agents_rules.py]

## Conventions

- Commit messages follow Conventional Commits (`feat:`, `fix:`, `chore:`, etc.).
- Commits are atomic: one self-contained logical change per commit. A multi-component change
  gets one commit per component, not one commit for the whole pipeline.
- No `Co-Authored-By` / AI-attribution trailers in commit messages (repo owner's global policy).
- All work is linked to a refined task in a project tracker (Iteration, effort estimate,
  start/target dates, classification label) before implementation starts — see
  `.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md`'s "Open item" section.
  **Open item**: there is currently no GitHub Project set up for this restart. Until one
  exists and its URL lands here, treat this as an unresolved gap, not silent policy.
- Prefer lifting tested code from `legacy` (see the plan's "What is lifted from legacy" table)
  over rewriting from scratch, except where the plan explicitly calls for a rewrite.
