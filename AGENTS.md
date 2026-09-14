# AGENTS.md — instructions for AI coding agents working with oxe

## What is oxe?

A local DuckDuckGo-backed web-search proxy with an Exa.ai-compatible
HTTP API and an MCP server. Built so AI agents (Claude Code, Cursor,
Hermes, etc.) can search the web without burning third-party API
quotas. Single Python process, ~70 MB RSS, SQLite TTL cache.

## Install

```bash
uv tool install oxe
```

(For a pinned version from source: `uv tool install "git+https://github.com/espetro/oxe@v0.1.0"`.)

## Run

```bash
oxe                                                    # foreground
nohup oxe >/tmp/oxe.log 2>&1 &                         # background
OXE_PORT=8080 oxe                                     # custom port
curl http://127.0.0.1:4479/health                      # health check
```

The proxy binds to `127.0.0.1` only. Default port `4479`.

## Endpoints

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/search` | Exa-compatible search (JSON in, JSON out) |
| `GET` | `/health` | liveness + cache stats |
| `GET` | `/cache/stats` | cache row count, hit total, db size |
| `POST` | `/cache/invalidate` | wipe all cached rows |
| `GET` | `/` | server-rendered HTML search UI |
| `GET` | `/history` | click history |
| `POST` | `/click` | record a click from the web UI |
| `GET` | `/mcp/` | StreamableHTTP MCP transport |

## MCP

- Mounted at `/mcp/`
- Tools:
  - `exa_search(query, num_results=10, type="auto", contents_highlights=true, contents_text=true, include_domains=[...], exclude_domains=[...], category="")`
  - `exa_user_history(query="", query_hash="", limit=20, since_hours=168)` — recent URLs the user clicked from the web UI for a query; use to avoid re-researching
- Cached responses are shared between HTTP and MCP.

## Configuration

All env vars (all optional, sensible defaults):

| Var | Default | Effect |
|---|---|---|
| `OXE_PORT` | `4479` | bind port |
| `OXE_CACHE_DIR` | `~/.cache/oxe` | SQLite directory |
| `OXE_TTL_DEFAULT` | `3600` | TTL for non-empty results (s) |
| `OXE_TTL_MAX` | `86400` | TTL ceiling (s) |
| `OXE_NEGATIVE_TTL` | `300` | TTL for empty results (s) |
| `OXE_CLICK_RETENTION_DAYS` | `30` | how long to keep click history |
| `OXE_LOG_LEVEL` | `INFO` | log level |

## Exa → DuckDuckGo translation notes

DuckDuckGo does not support deep search variants, summary, system
prompts, or output schemas. Pass-through fields are silently ignored
with a warning in the server log. Result fields are filled with
sensible defaults where DDG has no equivalent (`publishedDate`,
`author`, `image` are always `null`).

## Smoke tests

```bash
# Health
curl -s http://127.0.0.1:4479/health | jq .

# Search
curl -s -X POST http://127.0.0.1:4479/search \
  -H 'Content-Type: application/json' \
  -d '{"query":"python asyncio","numResults":3,"contents":{"text":true,"highlights":true}}' | jq '.results[0].title'

# MCP initialize handshake
curl -s http://127.0.0.1:4479/mcp/ -X POST \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"agent","version":"0"}}}' | head

# Then list tools / call exa_search with the session id from the response
```

## Common agent tasks

- **Find URLs the user has already explored for a query**: call
  `exa_user_history(query="...")` BEFORE searching, so you can cite
  what they read.
- **Track a click from your own tool**: `POST /click` with
  `{query_hash, result_id, url, title, source: "mcp"}` so the user
  sees your exploration in `/history`.
- **Override the cache directory** if your environment uses a
  non-standard home: `OXE_CACHE_DIR=/var/cache/oxe oxe`.
