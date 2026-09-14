# oxe

**Local web-search proxy and cache for AI agents.** Exa.ai-compatible HTTP API, StreamableHTTP MCP, DuckDuckGo backend, SQLite TTL cache. ~70 MB RSS. One Python process. No API keys.

[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Python 3.10+](https://img.shields.io/badge/python-3.10+-blue.svg)](https://www.python.org/downloads/)
[![uv-installable](https://img.shields.io/badge/uv-tool%20install-purple.svg)](https://docs.astral.sh/uv/)
[![MCP](https://img.shields.io/badge/MCP-compatible-green.svg)](https://modelcontextprotocol.io/)

```bash
uv tool install oxe
oxe
```

---

## Why oxe?

- **Don't burn your Exa/Brave/Serper free tier.** Same Exa-shaped HTTP API, served from your own machine, backed by DuckDuckGo. Cached responses are shared between HTTP and MCP — second agent hits `/search` for `python asyncio`? It's instant.
- **Two surfaces, one cache.** `POST /search` for HTTP clients; `/mcp/` StreamableHTTP transport for Claude Code, Cursor, Hermes, or any MCP-aware agent. Both pull from the same SQLite TTL cache.
- **Your agents see what you explored.** Click a result in the web UI; the URL is logged with `exa_user_history` so future agents know what's already been read.
- **Tiny footprint.** ~70 MB steady-state RAM, single `uvicorn` worker. Runs on a Raspberry Pi.

## Install

### One-liner (PyPI)

```bash
uv tool install oxe
```

### Pinned from GitHub

```bash
uv tool install "git+https://github.com/espetro/oxe@v0.1.0"
```

### With [mise](https://mise.jdx.dev/)

```toml
# mise.toml
[tools]
"pypi:oxe" = "latest"
```

### From source

```bash
git clone https://github.com/espetro/oxe
cd oxe
uv tool install -e .
```

## Run

```bash
oxe                                            # foreground
curl http://127.0.0.1:4479/health              # {"status":"ok",...}
```

Binds to `127.0.0.1:4479` by default. Override with `OXE_PORT=8080 oxe`.

Open `http://127.0.0.1:4479/` for the search UI.

## Endpoints

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/search` | Exa-compatible search (JSON in, JSON out) |
| `GET` | `/health` | liveness + cache stats |
| `GET` | `/cache/stats` | cache row count, hits, db size |
| `POST` | `/cache/invalidate` | wipe all cached rows |
| `GET` | `/` | server-rendered HTML search UI |
| `GET` | `/history` | click history |
| `POST` | `/click` | record a click (called by the UI) |
| `GET` | `/mcp/` | StreamableHTTP MCP transport |
| `GET` | `/docs` | FastAPI auto-generated OpenAPI |

### Quick search

```bash
curl -s -X POST http://127.0.0.1:4479/search \
  -H 'Content-Type: application/json' \
  -d '{"query":"python asyncio","numResults":3,"contents":{"text":true,"highlights":true}}' \
  | jq '.results[].title'
```

### MCP handshake

```bash
curl -s http://127.0.0.1:4479/mcp/ -X POST \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"cli","version":"0"}}}' \
  | head
```

## MCP tools

- **`exa_search(query, num_results=10, type="auto", contents_highlights=true, contents_text=true, include_domains=[...], exclude_domains=[...], category="")`** — Exa-shaped search response.
- **`exa_user_history(query="", query_hash="", limit=20, since_hours=168)`** — recent URLs you opened from the web UI for a given query. Call this BEFORE searching if you want to avoid re-researching what you already explored.

Both tools share the same SQLite cache as the HTTP endpoint.

## Configuration

All optional. Override via env vars:

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

DuckDuckGo does not support deep-search variants, summaries, system
prompts, or output schemas. Pass-through fields are silently ignored
with a server log warning. The following Exa fields are always
`null` for DDG results because DDG doesn't expose them:

- `publishedDate`
- `author`
- `image`

`text` and `highlights` are populated only when `contents.text=true`
and `contents.highlights=true` are requested.

## Architecture

```
┌────────────┐    POST /search    ┌──────────────────────────────┐
│  HTTP CLI  │ ─────────────────▶ │                              │
└────────────┘                    │   oxe (FastAPI + uvicorn)    │
                                  │                              │
┌────────────┐    MCP /mcp/       │  ┌────────────────────────┐  │
│  Claude /  │ ─────────────────▶ │  │  SQLite (WAL) cache    │  │
│  Hermes /  │                    │  │  + ddgs (DuckDuckGo)   │  │
│  Cursor    │                    │  └────────────────────────┘  │
└────────────┘                    │                              │
                                  │  static/app.js (vanilla JS)  │
┌────────────┐  browser           │  + <template> result cards   │
│  You, via  │ ─────▶ /  ────────▶│  + sendBeacon /click         │
│  browser   │                    └──────────────────────────────┘
└────────────┘
```

- `oxe/cache.py` — SQLite WAL, gzip values, TTL eviction.
- `oxe/exa_compat.py` — Exa request/response ↔ `ddgs` translation.
- `oxe/search.py` — shared `do_search(cache, req)` used by HTTP and MCP.
- `oxe/server.py` — FastAPI app, all routes, startup click pruner.
- `oxe/mcp_server.py` — `MCPServer` with two tools.
- `oxe/ui.py` + `oxe/static/{app.js,ui.css}` — stdlib-rendered HTML, vanilla JS.

## Memory & cost

- **RSS**: ~70 MB cold-start, ~27-35 MB steady-state. 200 MB ceiling.
- **Disk**: `~/.cache/oxe/cache.db` typically <10 MB. WAL file `<5 MB`.
- **Network**: one DuckDuckGo HTML request per unique cache miss. Cached responses replay instantly.
- **Third-party services**: none. No API keys, no telemetry.

## Run as a service

### systemd

```ini
# ~/.config/systemd/user/oxe.service
[Unit]
Description=oxe web-search proxy
After=network.target

[Service]
ExecStart=/home/you/.local/bin/oxe
Restart=on-failure
Environment=OXE_PORT=4479

[Install]
WantedBy=default.target
```

```bash
systemctl --user enable --now oxe
```

### launchd (macOS)

```xml
<!-- ~/Library/LaunchAgents/local.oxe.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>local.oxe</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Users/you/.local/bin/oxe</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>OXE_PORT</key><string>4479</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
</dict>
</plist>
```

```bash
launchctl load ~/Library/LaunchAgents/local.oxe.plist
```

## How is this different from X?

| Feature | oxe | `ddgs` direct | MCP-server competitors |
|---|---|---|---|
| Exa-compatible HTTP API | ✅ | ❌ | ❌ |
| MCP server | ✅ | ❌ | ✅ |
| SQLite TTL cache | ✅ | ❌ | ❌ |
| Click-history tool | ✅ | ❌ | ❌ |
| Single process | ✅ | n/a | ✅ |
| External API key | ❌ | ❌ | ❌ |
| Web UI | ✅ | ❌ | ❌ |

## Dependencies

Runtime: `ddgs`, `fastapi`, `uvicorn`, `pydantic`, `mcp`. All pulled by `uv tool install oxe` automatically. No system-level dependencies.

Optional host tools (not required): `portless` for `https://*.localhost/` URLs, `oxmgr` / `systemd` / `launchd` for supervision.

## License

[MIT](LICENSE).
