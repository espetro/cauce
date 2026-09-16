# Changelog

All notable changes to oxe are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-09-15

### Added

- Web UI rewritten as a Preact SPA in a new `ui/` workspace (Vite + daisyUI), served by the Python server from the built bundle. AI answer mode with SSE streaming (tool steps, sources, related queries), DDG-style pill search bar with segmented Search/AI toggle, letters pager, theme switcher, cache badge with web-refresh, pagination, `?settings=open` URL state.
- AI answers end to end: `POST /answer` (SSE), `GET /v1/models` (provider model listing via lazy SDK imports), answer caching. `oxe[ai]` optional extra (aisuite with openai/anthropic).
- `GET /settings` / `PUT /settings`: read and write the `[ai]` section of `config.toml` (api_key redacted on read, preserved on write when omitted). `{env.NAME}` interpolation in config string values and `.env` loading from the working directory (`oxe.config`).
- `GET /ac`: DuckDuckGo autocomplete proxy for UI suggestions; `GET /suggest`: OpenSearch suggestions over own query history.
- Search pagination end to end (`page` in the cache key and DDGS page kwarg); `_cached_at` on cached payloads and a `_refresh` flag to force network refresh.
- Dev observability: `OXE_DEV=1` emits structured JSON events (search, suggest, ac, answer, models); `mise run logs` tails the dev server log.
- `POST /row/{key}/delete` is idempotent: 204 for non-HTML clients on an already-deleted row (UI refresh flow), 303 redirect for browsers.
- `GET /api/history` (merged click-history view) and `GET /api/stats` (dashboard metrics: searches per day, cache hit rate, latency percentiles, client split), plus a `/dashboard` SPA shell that renders them.
- Pydantic contract models for the typed endpoints with a JSON error envelope, and an OpenAPI export pipeline (`/docs`).
- Error states for search: backend failures are distinguished from genuinely empty results via `_error` and `_error_kind` payload fields and an `X-Cache: HIT/MISS` response header; failed searches are not cached.
- `POST /settings/test` to verify AI provider credentials; env-template-preserving saves and conf-gated answer caching with greeting guard.
- Web UI: self-hosted fonts (Plus Jakarta Sans + Apfel Grotesk), i18n via paraglide (EN-only), motion system with status state machines and error boundaries, virtualized continuous-scroll result list, openapi-generated types with a single typed request helper.

### Changed

- `oxe/server.py` split into an `oxe/server/` package of APIRouters grouped by resource.
- The server serves per-route prerendered SPA shells with a root fallback for faster first paint.

### Changed

- The server serves the SPA from a built bundle resolved as `$OXE_UI_DIST`, `./ui/dist` (repo checkout), then packaged `oxe/ui_dist`. When no bundle exists, `/` serves a minimal inline page explaining how to get the UI; the JSON API and MCP work regardless.
- The legacy server-rendered UI was removed: `oxe/ui.py` and `oxe/static/` are gone. Content negotiation on `/search` is unchanged (HTML browsers get the SPA or the no-UI page, `Accept: application/json` clients get Exa JSON).
- `mise run build:ui` builds `ui/dist` and copies it to `oxe/ui_dist` for packaging; package-data ships `ui_dist` when present.
- Ruff configuration now lives in `pyproject.toml` (select E,F,W,I,B,SIM,RUF,C4,UP,BLE,TRY with pragmatic ignores); `mise run lint:py` is clean.
- AI clients are closed per request and managed with context managers; readonly stats connections are closed deterministically.
- UI size budgets raised to 45KB gzipped JS / 35KB gzipped CSS for feature headroom.

## [0.3.1] - 2026-09-15

### Added

- `auto` as a named backend in `OXE_BACKENDS` (maps to ddgs's `backend="auto"` multi-engine sweep).
- Fuzzy query matching for `/cache` and `/history` search: rapidfuzz when installed, stdlib `difflib` fallback otherwise. Typos like "pyton" still find "python" rows.
- `rapidfuzz` is now an optional dependency; without it the SQL layer still works fully (fuzzy degrades to difflib).

### Changed

- All SQL moved from inline strings in `oxe/cache.py` and `oxe/stats.py` to `oxe/sql/*.sql` files, loaded via [aiosql](https://github.com/nackjicholson/aiosql). Queries are named, documented, and validated at import time.
- `sqlite3.Row` row access throughout the cache layer; positional tuple unpacking removed.
- Optional cache/history filters use a static null-or-filter pattern instead of dynamic WHERE building.

## [0.3.0] - 2026-09-14

### Added

- Multi-backend support. `OXE_BACKENDS` env var (JSON) selects and composes search backends; unset means plain DuckDuckGo as before. Any backend registered under the `oxe.backends` entry-point group works by name, including everything the bundled `ddgs` library supports.
- Built-in compositors: `fallback` (first backend that returns results wins) and `fanout` (query all backends concurrently, merge and dedupe by URL) via `oxe.backends.FallbackBackend` / `FanoutBackend`.
- `oxe.registry` with `discover()` (builtins + engine names + entry-point discovery), `resolve(spec)`, and `build_from_env()`. Third-party packages can ship a backend by declaring an `oxe.backends` entry point.
- `DdgsBackend(engine)`: drives any engine the bundled `ddgs` library supports (bing, brave, google, mojeek, yahoo, yandex, wikipedia) through the same Exa-compatible translation layer; `DDGBackend` is now a thin `ddg` specialization of it.
- `BackendError` exported for backend implementers; new `SearchBackend` protocol members documented in the README.
- `BackendError`, `FallbackBackend`, `FanoutBackend`, `SearchBackend`, and `oxe.registry.build_from_env` are exported from the package root.

### Changed

- The cache key now includes the backend name, so results from different backends never collide in the same database.

## [0.2.0] - 2026-09-15

### Added

- Search log: every search (HTTP and MCP) writes a `search_log` row (query text + hash, cache/network source, result count, duration in ms, client) to the cache database, pruned by `OXE_SEARCH_LOG_RETENTION_DAYS` (default 30).
- `oxe stats` CLI subcommand: builds a static stats dashboard (`index.html`, six inline-SVG panels, zero JavaScript) from the search log. Opens the SQLite file read-only, so it is safe against a live WAL database. Flags: `--db`, `--out`, `--days`.
- `GET /search?q=...` with content negotiation: browsers get the rendered results page, `Accept: application/json` clients get Exa-shaped JSON from the same URL. Includes a share row (canonical link, copy JSON, copy link) and result paging via `&p=`.
- `/?q=X` now redirects to `/search?q=X`, so `.../search?q=%s` can be registered as a browser search engine.
- `make_app(cache=None, on_result=None)` factory in `oxe.server` for embedding: bring your own cache, or attach an `on_result` observer called with every search payload.
- Result cards are mirrored server-side (favicon + domain, title, snippet, collapsed page-text preview), so `/search?q=` renders even before JavaScript runs.

### Changed

- Page templates extracted from Python string literals in `oxe/ui.py` to `oxe/static/*.html`, loaded via `importlib.resources` (packaged as package data).
- Web UI reworked to a Google-style results anatomy per the screen specs in `.agents/docs/screens/`.
- Search-log latency: `do_search(..., with_duration=True)` returns `(payload, duration_ms)`; the MCP tool uses it to record real network latency.

### Fixed

- MCP `exa_search` search-log rows recorded `duration_ms = NULL` on every network search because the duration was only attached to the observer copy of the payload. Latency is now returned explicitly and logged for both HTTP and MCP searches.
- Duplicate `panel_client_split_rows` definition (one with an incomplete body) removed from `oxe/stats.py`.

## [0.1.3] - 2026-09-14

### Fixed

- README "Browser-friendly URLs" section now documents a working setup. The previous `portless oxe oxe` snippet was broken because `oxe` reads `OXE_PORT`, not the generic `PORT` env var that portless sets for the proxied process. The new section recommends `portless alias search 4479` (static route pointing at an oxmgr/systemd/launchd-supervised oxe) and provides a `portless run` fallback that explicitly stops the supervisor first.

## [0.1.2] - 2026-09-14

### Fixed

- `/health` and `/openapi.json` now report the actual installed version (was hardcoded to `0.1.0`; now reads `from oxe import __version__`).

### Documentation

- New README sections: Browser-friendly URLs (portless + `https://search.localhost/`), Use as a Python library, Multi-device setups (with explicit warning about SQLite over NFS/SMB), Observability roadmap (SSG dashboard plan).
- Comparison table now reflects library + multi-device capabilities.

## [0.1.1] - 2026-09-14

### Removed

- Dropped the legacy `ex-search-proxy` console-script alias. Use `oxe` only.
  The alias was kept in v0.1.0 for migration; it is no longer installed.

## [0.1.0] - 2026-09-14

### Added

- Initial public release.
- Exa.ai-compatible HTTP `POST /search` endpoint backed by DuckDuckGo via `ddgs`.
- StreamableHTTP MCP server at `/mcp/` with `exa_search` and `exa_user_history` tools.
- SQLite WAL cache with gzip-compressed JSON values, TTL policy (1h default, 24h ceiling, 5m for empty results).
- Server-rendered web UI at `/` — search bar, result cards, click history view.
- Click tracking: `POST /click` records every URL the user opens from the UI.
- `exa_user_history` MCP tool exposes click history to agents.
- Startup pruner removes clicks older than `OXE_CLICK_RETENTION_DAYS` (default 30).
- One Python process, ~70 MB RSS steady-state.
- MIT licensed.
