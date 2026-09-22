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
- Rename decided: project cauce → cauce, CLI `cauce`; rename step #123 scheduled in It 3 before W2 Batch I dispatch.
