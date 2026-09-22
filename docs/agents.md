# Agent wiring

oxe serves MCP over streamable HTTP at `https://search.localhost/mcp`
(the portless alias for `127.0.0.1:4479`; see [install.md](install.md)).
Four tools:

| Tool | Purpose |
|---|---|
| `search_web` | Canonical search: `query`, `page?`, `engines?`, `lang?`, `ttl_s?` → oxe `SearchResponse` with `meta.request_id` and `meta.source` |
| `exa_search` | Frozen Exa wire shape (`query`, `num_results?`, `type?`, `source?`, `exclude_domains?`, `category?`) for existing Exa-compatible wiring |
| `cache_status` | Cache stats snapshot plus `request_id` |
| `cache_invalidate` | Invalidate by `key`, `expired`, or `all` (exactly one selector; audited) |

While iterating on harnesses, pin `engines=["wikipedia"]` (keyless,
gentle rate limits; needs a `[[engines]]` config entry since it ships
`enabled: false`) or `engines=["replay"]` (deterministic, offline).

## Claude Code

Per-session, one-shot:

```bash
claude --mcp-config '{"mcpServers":{"search":{"type":"http","url":"https://search.localhost/mcp"}}}' \
       --strict-mcp-config -p \
       --allowedTools "mcp__search__search_web,mcp__search__exa_search" \
       "Search for X with search_web and report the first 3 results."
```

Or persistently in `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "search": { "type": "http", "url": "https://search.localhost/mcp" }
  }
}
```

## Hermes

```bash
yes | hermes mcp add search --url https://search.localhost/mcp --auth header
hermes mcp list   # confirm 'search' enabled
```

Then in interactive `hermes chat`, the tools are available under their
standard names (Hermes prefixes `mcp__search__` automatically). Note:
hermes `-z` (one-shot) mode does not load MCP tools — use `chat`.

## maki

Edit `~/.config/maki/mcp.toml`:

```toml
[mcp.search]
url = "https://search.localhost/mcp"
enabled = true
```

maki's `-p` (one-shot) mode only loads OAuth-capable servers from that
file and the proxy exposes a plain streamable-HTTP transport, so the
tools may not register in `-p`; run maki interactively.

## Generic HTTP clients (curl, scripts, other agents)

The endpoint is streamable HTTP: every POST needs
`Accept: application/json, text/event-stream`, and calls after
`initialize` carry the `mcp-session-id` response header. Responses are
SSE frames (`data: {jsonrpc}` lines).

```bash
MCP=https://search.localhost/mcp

# 1. initialize; capture the session id
SID=$(curl -sS -D - -o /dev/null "$MCP" \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}' \
  | awk 'tolower($1) == "mcp-session-id:" {print $2}' | tr -d '\r')

# 2. mark the session initialized
curl -sS -o /dev/null "$MCP" \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -H "mcp-session-id: $SID" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}'

# 3. call a tool
curl -sS "$MCP" \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -H "mcp-session-id: $SID" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search_web","arguments":{"query":"rust async tokio","engines":["wikipedia"]}}}'
```

For plain HTTP without MCP, `GET /api/search?q=...` returns the same
`SearchResponse` JSON `search_web` wraps.
