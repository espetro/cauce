# Contributing

oxe is developed in waves. Every change maps to a refined issue on the GitHub
Project; there is no orphan work.

## Picking up a step

The pickup procedure (issue selection, worktree layout, branch naming,
definition of done) lives in [`.agents/plans/v3/README.md`](.agents/plans/v3/README.md).
Read it before starting a step.

## Commits

- Conventional Commits, atomic commits.
- Every commit carries a `Signed-off-by:` trailer. Commit with `git commit -s`.
  The sign-off certifies the Developer Certificate of Origin 1.1 (see `DCO`).
  CI rejects commits without it.
- No AI attribution trailers of any kind.

## Licensing

- `crates/*` and the repository root are MPL-2.0; see `LICENSE`.
- `engines/` and `sdk/` are Apache-2.0; see `engines/LICENSE` and `sdk/LICENSE`.
- New files inherit the license of the tree they live in. Copyright line:
  `Copyright (c) 2026 Joaquin Terrasa and oxe contributors`.
