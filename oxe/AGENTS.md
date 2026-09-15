# oxe/ — backend agent guidelines

Scope: the `oxe/` Python package (server, cache, backends, MCP, templates). Repo-wide rules in root `AGENTS.md`; UI in `ui/` follows `ui/AGENTS.md`.

## Principles

- Keep it lean: `ruff` for lint/format, `pytest` for tests. No other linters or type checkers unless the owner asks.
- Minimal runtime dependency surface; new runtime deps need justification in the commit.
- The web UI is the `ui/` SPA build, served via `OXE_UI_DIST` resolution (`$OXE_UI_DIST` -> `./ui/dist` -> packaged `oxe/ui_dist`). There are no server-rendered templates; when no bundle exists `/` serves a minimal inline notice page.

## Invariants (break these and something silently regresses)

- **Content negotiation**: the same URL serves HTML to browsers and the Exa-shaped JSON payload to `Accept: application/json` clients.
- **Cache transparency**: hit/miss, backend, duration, q_hash must remain visible (UI meta lines + API payloads).
- **Click tracking**: `POST /click` records source (`web-ui` | `mcp`); history retention follows `OXE_CLICK_RETENTION_DAYS`.
- **Local binding**: the server binds `127.0.0.1` only. Never loosen without owner approval.
- Search reuse is the product: query+result caching across agents/users. Local-first is causal (fast), not a privacy enforcement; remote cache backends are a legitimate future path.

## Workflow

- `mise run test` (pytest) and `mise run lint` (ruff) must pass before declaring work done.
- Schema changes go through `oxe/sql/` migration files; never hand-edit a live DB.
- Config changes extend the env var table in root `AGENTS.md`.

## Packaging

This directory is the published PyPI package. Anything placed here ships
(or must be explicitly excluded from `package-data` in `pyproject.toml`).
Do not add non-runtime files (notes, drafts, agent docs) here; they belong
in `.agents/`.
