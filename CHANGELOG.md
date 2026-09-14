# Changelog

All notable changes to oxe are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
