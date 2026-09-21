# v3 subplans: index and dispatch rules

Parent plan: `../2026-09-21-v3-rust-core.md` (architecture, contracts, data model, verification
policy). Each wave below is one subplan file, one EPIC issue on the GitHub Project
(https://github.com/users/espetro/projects/23), and one 2-week iteration. Each step inside a
wave is one issue, one branch, one PR.

| Wave | File | Iteration | Dates | Priority | Exit criterion |
|---|---|---|---|---|---|
| W0 | `wave-0-skeleton.md` | 1 | 2026-09-22 to 10-05 | P0 | golden path green on `replay`; `oxe serve` returns real results through the `ddgs` exec bridge; every request traceable by id |
| W1 | `wave-1-engines-and-agents.md` | 2 | 10-06 to 10-19 | P0 | Bing + Brave native; MCP over HTTP and stdio; `~/SEARCH.md` wiring cut over to v3 on 4479 |
| W2 | `wave-2-ui-and-observability.md` | 3 | 10-20 to 11-02 | P1 | all HTMX pages usable; owner uses it daily for a week |
| W3 | `wave-3-tail-tolerance.md` | 4 | 11-03 to 11-16 | P1 | hedging, stale-while-revalidate, fairness, nightly relevance evals |
| W4 | `wave-4-ai-mode.md` | 5 | 11-17 to 11-30 | P2 | streamed, grounded answers via Bifrost; AI evals with baseline |
| W5 | `wave-5-archive-and-semantic.md` | 6 | 12-01 to 12-14 | P2 | `fetch_and_index`, `search_archive`, optional semantic tier |
| W6 | `wave-6-postgres-and-multi-instance.md` | 7 | 12-15 to 12-28 | P2 | Postgres `Store` passes the conformance suite; deployment docs |
| later | `later/*.md` | none | none | P3 | stubs only; not v3.0 |

## Dependency graph

```
W0-01 license ─┐
W0-02 workspace ┴─► W0-03 core types ─┬─► W0-04 sqlite store ─┐
                                      ├─► W0-05 observability ┤
                                      ├─► W0-06 replay engine ┼─► W0-08 pipeline v0 ─► W0-09 routes ─► W0-10 HTMX page ─► W0-12 golden path
                                      ├─► W0-07 exec + ddgs   ┘                            ▲
                                      └─► W0-11 config ─────────────────────────────────────┘
W0 ─► W1-01 http/egress ─► W1-02 declarative runtime ─► W1-03/04/05 engine specs ─► W1-11 cutover
   ├► W1-06 health, W1-07 admission, W1-10 tier-2 FTS  (need W0-08)
   ├► W1-08 MCP (needs W0-09), W1-09 metrics (needs W0-05), W1-12 modes (needs W1-08)
W1 ─► W2-* (UI pages, each needs its route from W0/W1) ─► W2-10 usage week ─► W3-*
W3 ─► W4 (AI) ─► W5 (archive, semantic; W5-04 embedder needs resource-adaptive config from W0-11)
W5 ─► W6 (Postgres needs W6-01 conformance suite first)
```

Rule: a step whose dependencies are not `Completed` on the board is not picked up. If you
believe a dependency is wrong, comment on the issue; do not start anyway.

## How to pick up a step (agents)

1. Read `../2026-09-21-v3-rust-core.md` sections 4 to 7, then the wave file, then the step.
2. Move the issue to `WIP` on the project (`ghx project item-edit`), assign yourself.
3. `git worktree add ~/.worktrees/oxe-<step-id> -b v3/<step-id>-<slug> main`.
4. Implement only what the step's "Do" says. The "Settled inputs" section of the wave file lists
   contracts you may not change; if the step cannot be done without changing one, stop and
   comment on the issue with the proposed amendment to the parent plan.
5. Acceptance assertions are the PR's test list. The golden path must stay green.
6. PR title `<type>(<scope>): <summary>` (Conventional Commits), body `Closes #<issue>`,
   commits signed off (`git commit -s`, DCO).
7. On merge: issue `Completed`, then read the step's "Follow-up" line for what unblocks next.

## Step ID format

`W<wave>-<nn>` (e.g. `W0-05`). The step heading in the wave file, the issue title prefix, the
branch name and the worktree directory all use it, so any of them leads to the others.
