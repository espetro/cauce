# Screen: History (`/history`)

Clicked-result history: every result opened from the web UI (or recorded
via `POST /click` from an MCP agent) lands here, grouped newest first, so
the user and their agents can see what was already read for a query.

## ASCII mockup

```
+------------------------------------------------------------------+
| oxe   search   [history]  cache   health   api           v0.3.x  |
+------------------------------------------------------------------+
| 12 clicks in last 24h - 340 total - 2026-09-01 08:12:33 (oldest) |
|                                                                  |
| filter: [ all time v ]   [query filter______]        clear       |
|                                                                  |
| | 2026-09-14 19:42 | python asyncio | Understanding asyncio    | |
| |                  |               | realpython.com/asyncio... | |
| |                  |               |  [copy json]       web-ui | |
| |----------------------------------------------------------------|
|                    ^ table separator (68 cols)                  |
| | 2026-09-14 18:05 | rust tokio   | Tokio tutorial            | |
| |                  |              | tokio.rs/tokio/tutorial   | |
| |                  |              |  [copy json]          mcp | |
| (newest first, one row per click; query links to the cached      |
|  response row; title+url open the page in a new tab; copy json   |
|  shares that query's Exa-shaped cached payload)                  |
|                                                                  |
+------------------------------------------------------------------+
```

Empty state:

```
+------------------------------------------------------------------+
| oxe   search   [history]  cache   health   api           v0.3.x  |
+------------------------------------------------------------------+
| 0 clicks in last 24h - 0 total - - (oldest)                      |
|                                                                  |
| no clicks yet - open a result from the search page.              |
|                                                                  |
+------------------------------------------------------------------+
```

## Behavior

- Per-row `copy json` affordance: copies the Exa-shaped cached payload
  for that query (same contract as `POST /search`), so a past search
  can be handed to an agent without re-running it. Tiny vanilla JS,
  `navigator.clipboard`; degrades to nothing without JS.
- Otherwise read-only; the other interactive bits are the filter
  controls, the `clear` link (only when a filter is active) and the
  links themselves.
- Rows are recorded on click from the search page (`POST /click` from
  `app.js`) and from MCP agents calling `POST /click` directly, so
  agent exploration shows up next to human browsing.
- Query cell links to the cached response row (`/row/<query_hash>`) so
  the user can see the full result set the click came from.
- Filter select offers 24h / 7d / 30d / all time (`since_hours` query
  param); query text filter narrows by `query_text` substring.
- Stats line above the table: clicks in last 24h, total, oldest
  timestamp (dash when empty).
- Source column distinguishes `web-ui` clicks from `mcp` agent clicks.

## Responsive

- Table columns (timestamp, query, title, url, source) collapse in
  priority order: url truncates first, then title; timestamp and query
  always visible.
- On narrow screens rows can wrap into stacked key-value style; no
  horizontal scroll required below ~360px.

## Notes

- Retention: rows are pruned after `OXE_CLICK_RETENTION_DAYS`
  (default 30). Expect the "oldest" timestamp to roll forward; old rows
  disappearing is retention, not data loss.
- Share-first: `copy json` is the history-screen twin of the search
  page's share row; both emit identical payloads for the same query
  hash.
- `exa_user_history` MCP tool reads this same table so agents can avoid
  re-researching URLs the user already opened.
- Cap of 200 rows returned per view; the filter is the way to reach
  older entries within the retention window.
