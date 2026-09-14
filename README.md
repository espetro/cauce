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
uv tool install "git+https://github.com/espetro/oxe@v0.1.1"
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

Open `http://127.0.0.1:4479/` for the search UI, or hit it as `https://search.localhost/` if you run it under [portless](https://portless.sh) (see [Browser-friendly URLs](#browser-friendly-urls)).

## Browser-friendly URLs

Most of the time `http://127.0.0.1:4479/` is fine. But three things are nicer with a real hostname + TLS:

- **Browsers let you grant microphone / clipboard / persistent-storage per-origin.** A trusted hostname (`https://search.localhost/`) makes per-site permissions stick across tabs, and lets you bookmark `/history` cleanly.
- **Your MCP agent talks to the same URL from anywhere on your LAN.** Hermes, Claude Code, and Cursor all accept `https://search.localhost/mcp/` as a transport once you've set it up once.
- **HTTPS solves the MCP `isahc`/curl clients that ignore Mac Keychain.** Without it you'll get opaque TLS errors when an MCP client (maki, Hermes) calls your local oxe.

Install [portless](https://portless.sh) (`brew install portless` or follow the [docs](https://portless.sh/llms.txt)). `oxe` reads `OXE_PORT` (default `4479`), not the generic `PORT` env var that portless sets for the proxied process, so the cleanest pairing is the static-alias path: let your supervisor (oxmgr / systemd / launchd) own the port, then point portless at it.

```bash
# One-time: pin a hostname to the port oxe is already listening on.
# oxe must be up first (oxmgr start oxe / your supervisor); the alias itself
# survives reboots and restarts, so this is genuinely one-time.
portless alias search 4479

curl https://search.localhost/health   # {"status":"ok", ...}
```

Order does not matter for the alias itself (it just points a hostname at a
port), but `curl` only succeeds once something is listening on 4479. To undo:
`portless alias --remove search`.

If you'd rather let portless supervise the process itself, pick a port up front so the route stays stable, and stop your supervisor first so the spawned oxe can bind 4479. The `--name` / `--app-port` flags belong to portless and must come **before** the command; anything after the command is passed to `oxe` as arguments and silently ignored, and the route ends up on a random port (502). Use the `run` subcommand:

```bash
# 1. stop whatever is already on 4479 (oxmgr / systemd / launchd).
#    Note: this stays stopped across reboots until you start it again.
oxmgr stop oxe    # or:  sudo systemctl stop oxe   /   launchctl unload ~/Library/LaunchAgents/oxe.plist

# 2. let portless spawn oxe pinned to a fixed port and hostname:
nohup env OXE_PORT=4479 portless run --name search --app-port 4479 oxe >/tmp/oxe.log 2>&1 &
portless list     # -> https://search.localhost  ->  localhost:4479  (pid ...)
```

`OXE_PORT=4479` is still required: oxe reads `OXE_PORT`, not the generic
`PORT` env var portless sets for the proxied child. `--app-port 4479` keeps
the portless route stable and lets its proxy find the app. The pid-managed
route dies with the process, so teardown is just `kill <pid from portless
list>`; there is no alias to remove. To go back to the supervised setup, `oxmgr start oxe` (if
oxmgr refuses with "failed to spawn", use `oxmgr restart oxe`) and re-register
the alias if you removed it.

Open `https://search.localhost/` in any browser — the cert is automatically trusted (portless manages its own local CA). Use `portless list` to confirm the route, `portless doctor` to debug.

### Register as a browser search engine

Once oxe is reachable on a stable hostname, add it as a browser search
engine (Chrome: Settings > Search engine > Site search):

```
https://search.localhost/search?q=%s
```

(or `http://127.0.0.1:4479/search?q=%s` without portless). Typing a
query in the address bar lands on the rendered results page at
`/search?q=...`, served from the same cache your agents use.

## Endpoints

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/search` | Exa-compatible search (JSON in, JSON out) |
| `GET` | `/search?q=X` | search results: HTML for browsers, Exa JSON with `Accept: application/json` |
| `GET` | `/` | server-rendered HTML search UI (`/?q=X` redirects to `/search?q=X`) |
| `GET` | `/row/{hash}` | redirect to `/search?q=<original query>` for a cache row |
| `GET` | `/health` | liveness + cache stats |
| `GET` | `/cache/stats` | cache row count, hits, db size |
| `POST` | `/cache/invalidate` | wipe all cached rows |
| `GET` | `/` | server-rendered HTML search UI (`/?q=X` redirects to `/search?q=X`) |
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

### Share a search

Every search has a canonical, shareable URL. Browsers get the rendered
results page; agents and scripts get Exa-shaped JSON from the same path:

```bash
# Browser: rendered results, cached server-side like any other query
open 'http://127.0.0.1:4479/search?q=python+asyncio'

# Agent: same URL, Exa JSON response
curl -s 'http://127.0.0.1:4479/search?q=python+asyncio' \
  -H 'Accept: application/json' | jq '.results[].title'
```

The results page shows a share row with the canonical link and a
copy-JSON button that puts the cached Exa payload on your clipboard.
`/?q=X` (what a browser search engine sends) redirects to `/search?q=X`,
so you can register `http://127.0.0.1:4479/search?q=%s` (or
`https://search.localhost/search?q=%s` under portless) as a browser
search engine.

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
| `OXE_SEARCH_LOG_RETENTION_DAYS` | `30` | how long to keep search_log rows (feeds `oxe stats`) |
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

## Use as a Python library

`oxe` is more than a CLI: every internal seam is a normal Python import.

```python
from oxe.cache import TTLCache
from oxe.search import do_search

import oxe

cache = TTLCache("/tmp/my-cache.db")
hits = do_search(cache, {"query": "python asyncio", "numResults": 5})

# Or build a standalone app with your own cache/backend/observer:
app = oxe.make_app(cache=cache)
```

Useful entry points:

| Symbol | Where | Use it for |
|---|---|---|
| `TTLCache(db_path)` | `oxe.cache` | Reusable SQLite cache with TTL eviction, click history, stats. Threadsafe (WAL + lock). |
| `do_search(cache, req_dict, ttl=None)` | `oxe.search` | The single canonical search path; shared by HTTP and MCP. |
| `exa_compat.search(req)` | `oxe.exa_compat` | Lower-level Exa↔DDGS translation (no cache). |
| `SearchBackend` / `CacheAdapter` / `DDGBackend` | `oxe.backends` | Protocols for plugging in your own search backend or cache; `DDGBackend` is the default. |
| `make_app(cache=None, backend=None, on_result=None)` | `oxe.server` | Build the FastAPI app with your own cache/backend/observer; ready to wrap in `uvicorn` or mount under another app. Module-level `app` is the default instance. |

Configuration is via env vars (`OXE_*`, listed below). For non-trivial embedding, instantiate `TTLCache(...)` yourself and pass it where you need it; the global module-level instance in `oxe.server` is only used by the bundled CLI.

## Multi-device setups

The most common "I want this everywhere" question is whether you can install oxe on every device on your network and point them at the **same** cache file on the router. **Don't.** SQLite in WAL mode is explicitly documented as not safe over NFS or SMB:

> *"WAL does not work over a network filesystem. This is because WAL requires all processes to share a small amount of memory and processes on separate host machines obviously cannot share memory with each other."* — [sqlite.org/wal.html](https://www.sqlite.org/wal.html)

> *"the network link ... in the File I/O channel, transactions may fail ... but with the additional effect that the remote database is corrupted."* — [sqlite.org/useovernet.html](https://www.sqlite.org/useovernet.html)

Real-world post-mortems (e.g. *"the SQLite trap that corrupted my S3 metadata"*, 2026) confirm that NFSv4 lock delegation silently fails under concurrent reader + writer and corrupts the database.

Three patterns that **do** work:

1. **Run one oxe on your router, every device hits it.** Simplest and recommended. Set `OXE_BIND=0.0.0.0` (currently `127.0.0.1` only — see `oxe/__main__.py`), expose `4479`, and have devices use `http://router.lan:4479/search`. Optionally put portless on the same box and get `https://search.localhost/` everywhere.
2. **Per-device cache + [Litestream](https://litestream.io) replication to a single S3/R2 bucket.** Each box has its own local SQLite; `litestream replicate ~/.cache/oxe/cache.db s3://bucket/$HOSTNAME.db` runs alongside oxe. Disaster recovery + a shared history you can merge from on boot. Adds a tiny Go binary per box.
3. **One central writer, many readers via oxe's HTTP API.** Same as (1) but framed deliberately — devices never touch SQLite directly, they POST to the central oxe.

For the LAN case, expose the port with care: `oxe` does **not** require auth today, so binding it to a public-facing interface means anyone on that network can search through you. Run it behind a reverse proxy with a bearer token, or on a trusted LAN only.

## Observability

The live process exposes `GET /health` (liveness, version, cache row count),
`GET /cache/stats` (rows, unexpired rows, hits total, db size, oldest/newest)
and the server-rendered listings at `GET /cache` and `GET /history`.

For trends, every search (HTTP and MCP) writes a row to the `search_log`
SQLite table: query text + hash, cache vs network, result count, latency in
ms, and the client (`http`, `mcp`, `web-ui`). Rows are pruned to
`OXE_SEARCH_LOG_RETENTION_DAYS` (default 30).

Build the dashboard with the `oxe stats` subcommand:

```bash
oxe stats --db ~/.cache/oxe/cache.db --out ./dist/dashboard --days 30
```

- `--db` defaults to `$OXE_CACHE_DIR/cache.db` (or `~/.cache/oxe/cache.db`), `--out` to `./dist/dashboard`, `--days` to `30`.
- Emits a single self-contained `index.html`: six panels (searches per day, cache hit rate, network latency p50/p95, client split, top queries, zero-result queries) as inline SVG. No JavaScript, no external assets.
- Opens the SQLite file read-only (`mode=ro` + `PRAGMA query_only`), so it is safe to run against the live cache while the server is writing (WAL).
- Open the file directly in a browser or serve it from any static host.

## How is this different from X?

| Feature | oxe | `ddgs` direct | MCP-server competitors |
|---|---|---|---|
| Exa-compatible HTTP API | ✅ | ❌ | ❌ |
| MCP server | ✅ | ❌ | ✅ |
| SQLite TTL cache | ✅ | ❌ | ❌ |
| Click-history tool | ✅ | ❌ | ❌ |
| Python library | ✅ (see [Use as a library](#use-as-a-python-library)) | ✅ (low-level) | ❌ |
| Multi-device via LAN | ✅ (run-on-router, see [Multi-device](#multi-device-setups)) | manual | manual |
| Single process | ✅ | n/a | ✅ |
| External API key | ❌ | ❌ | ❌ |
| Web UI | ✅ | ❌ | ❌ |

## Dependencies

Runtime: `ddgs`, `fastapi`, `uvicorn`, `pydantic`, `mcp`. All pulled by `uv tool install oxe` automatically. No system-level dependencies.

Optional host tools (not required): [`portless`](https://portless.sh) for `https://*.localhost/` URLs (recommended for browser + TLS), `oxmgr` / `systemd` / `launchd` for supervision.

## License

[MIT](LICENSE).
