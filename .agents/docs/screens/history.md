# Screen: History (`/history`)

Clicked-result history: every result opened from the web UI (or recorded
via `POST /click` from an MCP agent) lands here, grouped newest first, so
the user and their agents can see what was already read for a query.

## ASCII mockup

```
+------------------------------------------------------------------+
| cauce   search   [history]  dashboard   (?)  settings  [gh] v0.4.0 |
+------------------------------------------------------------------+
| Click history                                                    |
| 12 clicks in last 24h · 340 total · 2026-09-01 08:12 (oldest)    |
|                                                                  |
| [ all time v ]  [filter by query text...]  [clear]               |
|                                                                  |
| clicked        query            title        url       src       |
| 2026-09-14    python asyncio    Understand~  realpyth~ web-ui  |
|  19:42        (link to /row)                             [copy json] |
| 2026-09-14    rust tokio        Tokio tutor  tokio.rs~ mcp     |
|  18:05                                                  [copy json] |
| (newest first, one row per click; query links to /row/<hash>;    |
|  title+url open the page in a new tab; url column hidden below   |
|  ~768px; copy json shares that query's Exa-shaped payload)       |
+------------------------------------------------------------------+
```

Empty state:

```
+------------------------------------------------------------------+
| cauce   search   [history]  dashboard   (?)  settings  [gh] v0.4.0 |
+------------------------------------------------------------------+
| 0 clicks in last 24h · 0 total · —                               |
|                                                                  |
| no clicks yet — open a result from the search page.              |
+------------------------------------------------------------------+
```

## Behavior

- Per-row `copy json` affordance: re-fetches the query from the search
  endpoint (`Accept: application/json`) and copies the Exa-shaped
  payload to the clipboard (same contract as `POST /search`), so a
  past search can be handed to an agent without re-running it.
- Otherwise read-only; the other interactive bits are the filter
  controls, the `clear` button (only when a filter is active) and the
  links themselves. A loading dots state covers fetches; fetch errors
  render an inline error line.
- Rows are recorded on click from the search page (`POST /click`) and
  from MCP agents calling `POST /click` directly, so agent exploration
  shows up next to human browsing.
- Query cell links to the cached response row (`/row/<query_hash>`) so
  the user can see the full result set the click came from.
- Filter select offers all time / last 24h / last week / last month
  (backed by the `since` hours param on `GET /history`); query text
  filter narrows the fetched rows client-side by query substring.
  Filters are client state only, not url-addressable today (see
  userflow-checkpoints.md for the planned `since`/`qf` params).
- Stats line above the table: clicks in last 24h, total, oldest
  timestamp (dash when empty). Local time format `YYYY-MM-DD HH:MM`.
- Source column distinguishes `web-ui` clicks from `mcp` agent clicks.
- Cap of 200 rows per view; use the filters to reach older entries.

## Responsive

- Table columns (clicked, query, title, url, source): the url column
  hides below ~768px; titles and urls truncate with ellipsis.
- Table container scrolls horizontally as a last resort on very
  narrow screens.

## Queued improvements

Harvested from `references/insights/`; not part of the conformance contract above.

- A quiet "served locally, cached on your machine" line, mirroring DuckDuckGo's
  privacy chrome placement.

## Design references

- [`references/tokens.md`](references/tokens.md) - canonical type scale,
  spacing, color roles behind the table and stats line.
- [`references/patterns-states.md`](references/patterns-states.md) -
  the empty state (`no clicks yet...`) and the loading-dots/inline-error
  states for the table fetch.
- [`references/patterns-layout-grid.md`](references/patterns-layout-grid.md)
  - table container width/gutters, header anchoring, the ~768px
  breakpoint where the url column hides.

## Notes

- Retention: rows are pruned after `CAUCE_CLICK_RETENTION_DAYS`
  (default 30). Expect the "oldest" timestamp to roll forward; old rows
  disappearing is retention, not data loss.
- Share-first: `copy json` is the history-screen twin of the search
  page's share row; both emit Exa-shaped payloads.
- `exa_user_history` MCP tool reads this same table so agents can avoid
  re-researching URLs the user already opened.
