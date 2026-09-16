<p align="center">
  <img src="assets/logotype.png" alt="oxe" width="320" />
</p>

<h1 align="center">your local web intel layer</h1>

<p align="center">
  <a href="https://pypi.org/project/oxe/"><img src="https://img.shields.io/pypi/v/oxe?color=blue" alt="PyPI" /></a>
  <a href="https://github.com/espetro/oxe"><img src="https://img.shields.io/badge/github-espetro%2Foxe-black" alt="GitHub" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT" /></a>
  <img src="https://img.shields.io/badge/python-3.10+-blue.svg" alt="Python 3.10+" />
  <img src="https://img.shields.io/badge/MCP-compatible-green.svg" alt="MCP" />
</p>

oxe is a local web-search proxy with an Exa.ai-compatible HTTP API and an MCP server, backed by DuckDuckGo. Built so AI agents (Claude Code, Cursor, Hermes, and anything that speaks HTTP or MCP) can search the web without burning third-party API quotas, and so you can search from a browser against the same cache. Single Python process, ~22 MB RSS (~56 MB with the `ai` extra), SQLite TTL cache, no API keys, no telemetry.

**Want your agent to set it up?** Paste this into it:

```text
oxe: local web search for AI agents (Exa-compatible API + MCP, DuckDuckGo-backed, no API quotas).

Install:  uv tool install oxe
Optional AI answers:  uv tool install "oxe[ai]"  (then set provider/model in the web UI settings at http://127.0.0.1:4479)

Prompt for your agent:
"I have oxe running locally at http://127.0.0.1:4479. Use its MCP server at http://127.0.0.1:4479/mcp/ (tools: exa_search, exa_user_history) instead of paid search APIs."
```

**Agents:** this document is the full context. Raw plaintext for cheap loading:
`https://raw.githubusercontent.com/espetro/oxe/refs/heads/main/README.md`

## Install and run

```bash
uv tool install oxe            # core
uv tool install "oxe[ai]"      # with AI answers (aisuite + provider SDKs)

oxe                            # binds to 127.0.0.1:4479
curl http://127.0.0.1:4479/health
```

Open `http://127.0.0.1:4479/` for the web UI (search, AI answers, history, settings). Other install paths (pinned from GitHub, mise, editable from source) and service setup (systemd, launchd) are covered further down.

## Endpoints

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/search` | Exa-compatible search (JSON in, JSON out). `X-Cache: HIT/MISS` response header. Backend failure is signaled with `_error` and `_error_kind` fields instead of a cached empty result. |
| `GET` | `/search?q=X` | HTML results page for browsers, Exa JSON with `Accept: application/json` |
| `GET` | `/` | SPA web UI from the built bundle (see `OXE_UI_DIST`) |
| `GET` | `/dashboard` | SPA dashboard shell (usage metrics, cache health) |
| `GET` | `/api/history` | merged click history view (UI + API) |
| `GET` | `/api/stats` | dashboard metrics: searches, hit rate, latency, clients |
| `GET` | `/suggest?q=X` | OpenSearch suggestions JSON (own query history + completions) |
| `GET` | `/ac?q=X` | DuckDuckGo autocomplete proxy (UI suggestions) |
| `POST` | `/answer` | SSE-streamed AI answer grounded in a live search (requires `oxe[ai]`) |
| `GET` | `/v1/models` | configured provider's model list (OpenAI-compatible; empty when unconfigured) |
| `GET` / `PUT` | `/settings` | read / write the `[ai]` config section (api_key redacted on read, preserved when omitted on write) |
| `POST` | `/settings/test` | test the configured AI provider |
| `GET` | `/health` | liveness + cache stats |
| `GET` | `/cache/stats` | cache row count, hit total, db size |
| `POST` | `/cache/invalidate` | wipe all cached rows |
| `GET` | `/row/{hash}` | redirect to `/search?q=<original query>` |
| `POST` | `/row/{key}/delete` | delete a cache row (204 idempotent for non-HTML clients, 303 for browsers) |
| `GET` | `/history` | click history |
| `POST` | `/history/delete` | delete click history (`scope=24h|7d|30d|all`) |
| `POST` | `/click` | record a click from the web UI (`sendBeacon`) |
| `GET` | `/mcp/` | StreamableHTTP MCP transport |
| `GET` | `/docs` | OpenAPI schema for the typed endpoints |

Errors use a JSON envelope (`{"error": {...}}`); search payload errors additionally carry `_error` / `_error_kind` so agents can distinguish "no results" from "backend down".

### Quick search

```bash
curl -s -X POST http://127.0.0.1:4479/search \
  -H 'Content-Type: application/json' \
  -d '{"query":"python asyncio","numResults":3,"contents":{"text":true,"highlights":true}}' \
  | jq '.results[].title'
```

Browsers can use the same URL: `http://127.0.0.1:4479/search?q=python+asyncio` renders the results page from the same cache. Register it as a browser search engine with `http://127.0.0.1:4479/search?q=%s`.

## MCP

Two tools on the StreamableHTTP transport at `http://127.0.0.1:4479/mcp/`, sharing the same SQLite cache as HTTP:

- **`exa_search(query, num_results=10, type="auto", contents_highlights=true, contents_text=true, include_domains=[], exclude_domains=[], category="")`** — Exa-shaped search response.
- **`exa_user_history(query="", query_hash="", limit=20, since_hours=168)`** — recent URLs the user opened from the web UI for a query. Call it before searching to avoid re-researching what was already explored.

Agents can record their own exploration with `POST /click` (`{query_hash, result_id, url, title, source: "mcp"}`) so it shows up in the user's history.

Handshake smoke test:

```bash
curl -s http://127.0.0.1:4479/mcp/ -X POST \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"cli","version":"0"}}}' | head
```

## Configuration

All env vars are optional:

| Var | Default | Effect |
|---|---|---|
| `OXE_PORT` | `4479` | bind port (always `127.0.0.1`) |
| `OXE_CACHE_DIR` | `~/.cache/oxe` | SQLite directory |
| `OXE_TTL_DEFAULT` | `3600` | TTL for non-empty results (s) |
| `OXE_TTL_MAX` | `86400` | TTL ceiling (s) |
| `OXE_NEGATIVE_TTL` | `300` | TTL for empty results (s) |
| `OXE_CLICK_RETENTION_DAYS` | `30` | how long to keep click history |
| `OXE_SEARCH_LOG_RETENTION_DAYS` | `30` | how long to keep search_log rows (feeds `/api/stats`) |
| `OXE_BACKENDS` | unset | JSON backend spec, see [Backends](#backends). Unset means DuckDuckGo. |
| `OXE_LOG_LEVEL` | `INFO` | log level |
| `OXE_UI_DIST` | unset | built SPA directory. Resolution order: this var, `./ui/dist` (repo checkout), packaged `oxe/ui_dist`. Without any, `/` serves a minimal page explaining how to get the UI; the JSON API and MCP work regardless. |
| `OXE_CONFIG_DIR` | `~/.config/oxe` | directory holding `config.toml` (AI settings) |
| `OXE_DEV` | unset | `1` = structured JSON dev observability events on stdout |

## Web UI

The UI is a Preact SPA (Vite, daisyUI, self-hosted fonts) served from the built bundle. It has a search view with an AI answer mode, a history page backed by `/api/history`, a dashboard backed by `/api/stats`, and a settings dialog that writes the same `config.toml` as `PUT /settings`. AI answers need `oxe[ai]` plus a provider and model, set in the settings dialog or in `$OXE_CONFIG_DIR/config.toml`:

```toml
[ai]
provider = "openai"
model = "gpt-4o-mini"
api_key = "{env.OPENAI_API_KEY}"   # {env.NAME} interpolation; a .env file in the cwd is loaded first
enabled = true
```

AI mode is off unless both `provider` and `model` are set. Providers: `openai`, `anthropic`, `groq`, `mistral`, `ollama`, `huggingface`.

## Backends

DuckDuckGo is the default, but the backend layer is pluggable. oxe can drive any engine the bundled [ddgs](https://pypi.org/project/ddgs/) library supports (bing, brave, google, mojeek, yahoo, yandex, wikipedia, and more), plus third-party backends registered under the `oxe.backends` entry-point group.

```bash
OXE_BACKENDS='"mojeek"' oxe                          # single engine
OXE_BACKENDS='["ddg", "brave"]' oxe                  # fallback chain
OXE_BACKENDS='{"mode": "fanout", "backends": ["ddg", "google"]}' oxe   # concurrent merge + dedupe
```

A string is one engine, a list is a fallback chain, an object is a compositor (`mode: fallback` or `fanout`). The cache key includes the backend name, so engines never collide in one database.

Engine reliability varies by network and time of day; some engines throttle datacenter or VPN IPs hard. Prefer fallback chains (`["ddg", "bing", "mojeek"]`) so a throttled engine falls through to a healthy one.

Python equivalent:

```python
import oxe

backend = oxe.build_from_env()  # or oxe.registry.resolve(["ddg", "brave"])
app = oxe.make_app(backend=backend)
```

Custom backends implement the `SearchBackend` protocol (`oxe.backends`) and register an `oxe.backends` entry point; they are discovered by name automatically.

## Exa to DuckDuckGo translation notes

DuckDuckGo does not support deep-search variants, summaries, system prompts, or output schemas. Pass-through fields are silently ignored with a server log warning. `publishedDate`, `author`, and `image` are always `null` for DDG results. `text` and `highlights` are populated only when `contents.text=true` and `contents.highlights=true` are requested.

<details>
<summary><strong>Architecture</strong></summary>

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
                                  │  Preact SPA (ui/dist)        │
┌────────────┐  browser           │  + SSE /answer streaming     │
│  You, via  │ ─────▶ /  ────────▶│  + sendBeacon /click         │
│  browser   │                    └──────────────────────────────┘
└────────────┘
```

- `oxe/cache.py` — SQLite WAL, gzip values, TTL eviction.
- `oxe/exa_compat.py` — Exa request/response to `ddgs` translation.
- `oxe/search.py` — shared `do_search(cache, req)` used by HTTP and MCP.
- `oxe/server/` — FastAPI app split into APIRouters per resource group.
- `oxe/mcp_server.py` — MCP server with the two tools above.
- `oxe/ai.py` — provider-backed answer generation with a tool loop over search.
- `ui/` workspace (not packaged) — Preact SPA; built bundle served via `OXE_UI_DIST`.

</details>

<details>
<summary><strong>Memory and cost</strong></summary>

- **RSS**: ~22 MB without the `ai` extra, ~56 MB with `oxe[ai]` (aisuite imports provider SDKs lazily). 200 MB ceiling.
- **Disk**: `~/.cache/oxe/cache.db` typically under 10 MB; WAL file under 5 MB.
- **Network**: one DuckDuckGo request per unique cache miss. Cached responses replay instantly.
- **Third-party services**: none. No API keys, no telemetry.

</details>

<details>
<summary><strong>Use as a Python library</strong></summary>

```python
from oxe.cache import TTLCache
from oxe.search import do_search

cache = TTLCache("/tmp/my-cache.db")
hits = do_search(cache, {"query": "python asyncio", "numResults": 5})

# Or build a standalone app with your own cache/backend/observer:
app = oxe.make_app(cache=cache)
```

| Symbol | Where | Use it for |
|---|---|---|
| `TTLCache(db_path)` | `oxe.cache` | Reusable SQLite cache with TTL eviction, click history, stats. Threadsafe (WAL + lock). |
| `do_search(cache, req_dict, ttl=None)` | `oxe.search` | The single canonical search path, shared by HTTP and MCP. |
| `exa_compat.search(req)` | `oxe.exa_compat` | Lower-level Exa to DDGS translation (no cache). |
| `SearchBackend` / `CacheAdapter` / `DDGBackend` | `oxe.backends` | Protocols for plugging in your own backend or cache; `DDGBackend` is the default, `DdgsBackend(engine)` drives any ddgs-supported engine. |
| `make_app(cache=None, backend=None, on_result=None)` | `oxe.server` | Build the FastAPI app with your own cache/backend/observer; wrap in `uvicorn` or mount under another app. |

Configuration is via `OXE_*` env vars (table above).

</details>

<details>
<summary><strong>Run as a service</strong></summary>

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

</details>

<details>
<summary><strong>Multi-device setups</strong></summary>

Do not point multiple devices at the same `cache.db` on a network filesystem. SQLite in WAL mode is explicitly documented as not safe over NFS or SMB:

> "WAL does not work over a network filesystem. This is because WAL requires all processes to share a small amount of memory and processes on separate host machines obviously cannot share memory with each other." — [sqlite.org/wal.html](https://www.sqlite.org/wal.html)

Three patterns that work:

1. **Run one oxe on your router or a home server, every device hits it.** Simplest and recommended: `http://router.lan:4479/search` from everywhere.
2. **Per-device cache with [Litestream](https://litestream.io) replication to S3/R2** if you want a mergeable, disaster-recoverable history.
3. **One central writer, many readers via the HTTP API.** Devices never touch SQLite directly; they POST to the central oxe.

Note that oxe does not require auth today: binding it to a shared interface means anyone on that network can search through you. Run it behind a reverse proxy with a bearer token, or on a trusted LAN only.

</details>

<details>
<summary><strong>Browser-friendly URLs (portless)</strong></summary>

`http://127.0.0.1:4479/` is fine most of the time. A real hostname with TLS is nicer for per-origin browser permissions, bookmarkable history pages, and MCP clients anywhere on your LAN.

With [portless](https://portless.sh) installed, pin a hostname to the port oxe already listens on (one-time; the alias survives restarts):

```bash
portless alias search 4479
curl https://search.localhost/health
```

Then register `https://search.localhost/search?q=%s` as a browser search engine. If you would rather let portless supervise the process, use `OXE_PORT=4479 portless run --name search --app-port 4479 oxe` (flags before the command; oxe reads `OXE_PORT`, not the generic `PORT`). To undo the alias: `portless alias --remove search`.

</details>

<details>
<summary><strong>Observability</strong></summary>

The live process exposes `GET /health` (liveness, version, cache rows), `GET /cache/stats`, `GET /api/history`, and `GET /api/stats` (searches, cache hit rate, latency, client split). With `OXE_DEV=1` it emits structured JSON events on stdout.

Every search (HTTP, MCP, web UI) also writes a row to the `search_log` SQLite table: query text + hash, cache vs network, result count, latency in ms, and the client. Rows are pruned to `OXE_SEARCH_LOG_RETENTION_DAYS` (default 30).

For a shareable static report, build the dashboard:

```bash
oxe stats --db ~/.cache/oxe/cache.db --out ./dist/dashboard --days 30
```

Emits a single self-contained `index.html` (six inline-SVG panels, no JavaScript). Opens SQLite read-only, so it is safe against the live cache.

</details>

<details>
<summary><strong>Machine context for AI agents</strong></summary>

> **Context for AI coding assistants working with or against oxe:**
>
> - **Project role:** local web-search proxy. Exa-compatible `POST /search`, `GET /search?q=` with content negotiation, StreamableHTTP MCP at `/mcp/`.
> - **Key invariants:** the SQLite cache is shared by HTTP and MCP; cache keys include query, page, and backend name. Empty results are cached with a short negative TTL; backend failures are NOT cached and return `_error` / `_error_kind` fields plus a non-2xx-tolerant payload check.
> - **Search errors vs empty results:** `_error_kind` values include `backend_error`. Treat presence of `_error` as retryable; absence with empty `results` as genuinely empty.
> - **Click tracking:** record agent-driven exploration via `POST /click` with `source: "mcp"`; read what the user already explored via the `exa_user_history` MCP tool before searching.
> - **Config:** AI settings live in `$OXE_CONFIG_DIR/config.toml` under `[ai]`; `PUT /settings` is the supported write path (omit `api_key` to keep the stored value). String values support `{env.NAME}` interpolation.
> - **Testing:** `mise run test` (Python), `mise run check` for lint/typecheck gates. Do not write tests that hit live DuckDuckGo.

</details>

## How is this different from X?

| Feature | oxe | `ddgs` direct | MCP-server competitors |
|---|---|---|---|
| Exa-compatible HTTP API | ✅ | ❌ | ❌ |
| MCP server | ✅ | ❌ | ✅ |
| SQLite TTL cache | ✅ | ❌ | ❌ |
| Click-history tool | ✅ | ❌ | ❌ |
| Web UI + AI answers | ✅ | ❌ | ❌ |
| Python library | ✅ | ✅ (low-level) | ❌ |
| Single process, no API keys | ✅ | n/a | ❌ |

## Dependencies

Runtime: `ddgs`, `fastapi`, `uvicorn`, `pydantic`, `mcp`, `aiosql`. All pulled by `uv tool install oxe`. No system-level dependencies. Optional extras: `oxe[ai]` for AI answers, `oxe[fuzzy]` for higher-quality fuzzy matching in history search (falls back to stdlib `difflib` without it).

The logotype is set in [Apfel Grotesk](https://github.com/kkoutnas/typo_apfel_grotezk) (OFL), converted to outlines in [assets/logotype.svg](assets/logotype.svg).

## License

[MIT](LICENSE).
