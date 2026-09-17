# oxe

Local web-search proxy and cache for AI agents. Exa-compatible HTTP API + MCP, DuckDuckGo backend, SQLite TTL.

v0.5.0 is a ground-up rebuild on React 19, TanStack Router and a gated FastAPI backend. The
previous implementation (v0.4.0 and earlier) lives on the `legacy` branch and remains
installable from PyPI.

This README will be filled in against the shipped surface in wave 5 of
`.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md`.

## Development setup

One-time, after cloning:

```bash
mise run hooks:install   # git config core.hooksPath .githooks
```

This points git at `.githooks/pre-push`, which runs `mise run validate`
(the same check CI runs on every branch) before a push is allowed.

Fast local gate:

```bash
mise run validate
```
