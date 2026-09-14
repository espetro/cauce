# ex-search-proxy

Local web-search proxy and cache for agent harnesses. Exposes an [Exa.ai](https://exa.ai)-compatible HTTP API at `POST /search` plus a StreamableHTTP MCP route at `POST /mcp` with a single tool, `exa_search`. Backed by [DuckDuckGo](https://duckduckgo.com) via the [`ddgs`](https://pypi.org/project/ddgs/) library. Caches responses in SQLite with TTL.

The proxy is intentionally lightweight (~70 MB RSS, single uvicorn process). It does not call Exa.ai itself — it exists so harnesses stop hammering rate-limited upstream search APIs.

## Endpoints

| Route | Method | Purpose |
|---|---|---|
| `/health` | GET | service status, cache size, version, PID |
| `/search` | POST | Exa-compatible search; accepts and ignores `Authorization` / `x-api-key` |
| `/cache/stats` | GET | row counts, db size, total hits |
| `/cache/invalidate` | POST | wipe the cache (admin operation; logs actor) |
| `/mcp/` | POST | StreamableHTTP MCP transport; run `initialize` -> `notifications/initialized` -> `tools/list` / `tools/call` |

## Install

```bash
git clone <repo-url> ex-search-proxy
cd ex-search-proxy

# Option A: install into the same venv as ddgs (smallest footprint, ~70 MB)
uv pip install --python "$(uv tool dir)/ddgs/bin/python" -e .

# Option B: install into a fresh venv (slightly larger, ~120 MB)
uv venv && uv pip install -e .
```

`ddgs` is the search backend; it must be reachable from whichever Python interpreter you launch the proxy with.

## Run

```bash
python -m ex_search_proxy
# or, if installed as a script:
ex-search-proxy
```

The proxy binds to `127.0.0.1:4479`. Override with `EX_SEARCH_PORT`.

## Configuration

| env var | default | meaning |
|---|---|---|
| `EX_SEARCH_PORT` | `4479` | bind port |
| `EX_SEARCH_CACHE_DIR` | `~/.cache/ex-search-proxy` | SQLite directory |
| `EX_SEARCH_TTL_DEFAULT` | `3600` | TTL for non-empty results (s) |
| `EX_SEARCH_TTL_MAX` | `86400` | TTL ceiling (s) |
| `EX_SEARCH_NEGATIVE_TTL` | `300` | TTL for empty results (s) |
| `EX_SEARCH_LOG_LEVEL` | `INFO` | log level |

## MCP tool: `exa_search`

Single tool exposed at `/mcp`. Same Exa-shaped response as `/search`. Args:

- `query` (required)
- `num_results` (1–30, default 10)
- `type` (`auto` | `instant`; `deep*` variants ignored)
- `contents_highlights` (default `true`)
- `contents_text` (default `true`)
- `include_domains`, `exclude_domains` (lists)
- `category` (`news` forces 24h limit; else ignored)

Returns `{requestId, searchType, results, costDollars, _source}`.

## Translation notes

- `query` -> `DDGS().text(query, max_results=…, backend=…)`.
- `includeDomains` / `excludeDomains` -> DDG `site:` / `-site:` syntax.
- `category=news` -> `timelimit=d`.
- Backend preference: `duckduckgo` first, fall back to `auto` on failure.
- Safesearch hardcoded to `moderate`.
- Highlights: simple sentence-rank — first 3 sentences of `body`, score 0.5.
- Exa fields not implemented (silently ignored, warning logged): `startPublishedDate`, `endPublishedDate`, `contents.summary`, `additionalQueries`, `systemPrompt`, `outputSchema`, `stream`.

## Layout

- `cache.py` — stdlib `sqlite3` with WAL, TTL eviction on write, gzipped JSON values.
- `exa_compat.py` — DDGS <-> Exa shape translation, cache-key derivation.
- `search.py` — shared `do_search(cache, req_dict)` helper used by both HTTP and MCP.
- `mcp_server.py` — `MCPServer` definition with one tool; receives cache via `set_cache()`.
- `server.py` — FastAPI app, route handlers, lifespan wiring for the MCP sub-app.
- `__main__.py` — uvicorn entry.

## Memory

Steady-state footprint is ~70 MB RSS (Python + FastAPI + ddgs + mcp libraries). Far under a 200 MB budget. No second process, no separate Python interpreter when installed into the `ddgs` venv.

## Supervision (oxmgr)

```toml
[[apps]]
name = "ex-search-proxy"
command = "ex-search-proxy"   # or: python -m ex_search_proxy
restart_policy = "always"
crash_restart_limit = 5
stop_timeout = 10
health_cmd = "curl -fsS --max-time 4 http://127.0.0.1:4479/health"
health_interval = 30
health_timeout = 5
health_max_failures = 3
```

Apply per-app (never restart the daemon for per-app changes):

```bash
oxmgr apply ~/.config/oxmgr/oxfile.toml --only ex-search-proxy
```

## Public URL (portless)

Expose to harnesses via [`portless`](https://portless.sh):

```bash
portless alias search 4479 --force
```

Then agents reference `https://search.localhost/health`, `https://search.localhost/search`, `https://search.localhost/mcp/`.

## License

MIT.
