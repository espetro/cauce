# Changelog

All notable changes to oxe are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
