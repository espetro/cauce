# Changelog

All notable changes to oxe are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
