# Screen: History (`/history`)

Search history the way a browser keeps it: one row per search (from
`search_log`), newest first, with the results the user or an agent clicked
nested under the search that produced them (from `clicks`). Every client
lands here: web UI searches, HTTP API calls and MCP tool calls, so the
owner can see what their agents already looked up. Each row also says
whether that query is still served from cache. Contract source:
`.agents/plans/v3/wave-2-ui-and-observability.md` step W2-02 (issue #34),
the parent plan's DATA section (`search_log` joined with `clicks`) and the
wave's "Settled inputs".

## ASCII mockup

```
+------------------------------------------------------------------+
| cauce   search   [history]   dashboard          settings  v3.0.0 |
+------------------------------------------------------------------+
| History                                                          |
| 41 searches in last 24h · 1 203 total · 9 clicks today           |
|                                                                  |
| [ last 24h v ]  [filter query text...    ]  [ ] cached only  clear|
|                                                                  |
| when   query             source          engines      n    ms  by|
| 01:12  python asyncio    cached · 41m    replay,ddgs  12   18  ui|
|        v 2 clicks                                                |
|          realpython.com   Understanding asyncio in Python    #1  |
|          docs.python.org  asyncio - Asynchronous I/O         #3  |
|        [re-run] [copy json] [payload] [delete]                   |
| 01:05  rust tokio        network · t1    ddgs          9 1412 mcp|
|        > 0 clicks        [re-run] [copy json] [payload] [delete] |
| 00:58  kubernetes ingr~  cached · expired replay        7   22 api|
| 00:40  (click only)      -               -            -    -  mcp|
|          tokio.rs         Tokio tutorial                     #2  |
|                                                                  |
| showing 200 of 1 203 · use the filters to reach older searches   |
|                                                                  |
| request 01J8Z4Q7X0V2H9R6KQ3T5N8M1B                               |
+------------------------------------------------------------------+
```

Empty state (fresh database):

```
| History                                                          |
| 0 searches in last 24h · 0 total · 0 clicks today                |
| nothing searched yet. run a search and it lands here.            |
```

Empty state (filters matched nothing):

```
| [ last 7d v ]  [kubernetes              ]  [x] cached only  clear |
| no searches match "kubernetes" in the last 7 days that are       |
| still cached.                                                    |
```

## Behavior

- Data path: `GET /api/history` with `Accept: text/html` renders the page
  with the API's own params: `since` (hours: `24`, `168`, `720`; absent
  means all), `q` (query substring), `cached=1` (only rows whose query has
  a live cache entry), `limit` (cap 200). A test asserts HTML rows equal the
  JSON rows for the same request. The `cached` param is new in W2-02 and
  is served by the same handler.
- One row per search, newest first. Columns, in order: when (local
  `HH:MM`, with the date on the first row of each day as a group header
  `2026-09-23`), query (links to `/search?q=`, ellipsis truncated), source,
  engines that answered (comma separated ids), n (result count), ms
  (latency), by (client: `ui`, `api`, `mcp`).
- Source column is the cache signal:
  - `cached · <age>` when the row's query still has an unexpired cache
    entry; the age is the entry's age now, not at search time. Links to
    that entry's payload on `/cache` (`/cache?q=<query>` opening the row).
  - `cached · expired` when the entry exists but its TTL passed.
  - `network · t<tier>` when this search fetched from the network (the
    tier that answered), and no entry survives for it.
  The value is computed at render time from `cache_entries`, so the same
  search row changes from `network` to `cached` as later identical searches
  refresh the entry.
- Clicks nest under their search (matched on `query_hash`) as a `<details>`
  whose summary reads `N clicks` (open by default when N > 0): one line per
  click with domain, title (link, opens in a new tab) and the result
  position `#n`. A click whose `query_hash` matches no search row (an MCP
  `record_visit` for a query cauce never ran) renders as its own row with
  query `(click only)` and dashes in the search columns, so agent browsing
  is never lost.
- Row actions, on the row's second line:
  - `re-run`: `GET /search?q=<query>` (normal navigation).
  - `copy json`: copies `GET /api/search?q=<query>` served from cache
    (Exa-shaped payload, same contract as the search page's share row);
    clipboard API with a selectable `<code>` fallback.
  - `payload`: link to the cache entry on `/cache`; shown only when the
    source is `cached`.
  - `delete`: `DELETE /api/history/<id>` with `X-Cauce-Client: ui`,
    native confirm, removes the search row and its nested clicks in place,
    audited as `history.delete` with actor `ui`. It does not touch the
    cache entry.
- Stats line under the heading: `N searches in last 24h · N total · N
  clicks today`. Zeroes render as `0`, never a dash.
- Filters are a plain GET form (works without JS): a `<select>` for
  `since` (`all time`, `last 24h`, `last 7d`, `last 30d`), a text input for
  `q`, a `cached only` checkbox for `cached=1`. `clear` appears only when
  any filter is active. Filters are URL state (`/history?since=24&q=py&cached=1`),
  so a QA agent can deep link every state.
- Cap of 200 rows; when more exist a line under the table says `showing
  200 of N · use the filters to reach older searches`.
- Acceptance (W2-02): after three replay searches the page shows three
  rows with the right sources; `since=24` hides a row backdated in the temp
  DB; a row deleted through the page is gone and an audit row with actor
  `ui` exists.
- Footer shows the full `request_id`, copyable.
- All copy comes from `strings.rs` (`HISTORY_*`).

## Reachable states (replay engine)

Populated: run replay searches from the UI and the API. `cached` vs
`network`: search twice. `cached · expired`: `ttl_s=1` then wait. Clicks:
`POST /api/click` after a search. Click only: `POST /api/click` with an
unseen query. Empty: fresh DB. Filtered empty: `cached=1` after deleting
the cache.

## Responsive

- Full width table with 16-24 px gutters. Below 768 px the `engines` and
  `ms` columns hide; below 640 px each search row becomes a two line card
  (query and source on the first line, `n · by · when` on the second) with
  the actions row under it, and click lines show domain and title only.
  No horizontal scroll at 390 px; titles truncate with ellipsis.

## Design references

- [`references/tokens.md`](references/tokens.md) - caption size for
  table cells, mono meta for engine ids and client, `success` role for the
  `cached` badge and `neutral` for `expired`, spacing `md` between rows.
- [`references/patterns-states.md`](references/patterns-states.md) - one
  sentence empty states with an action; inline errors for a failed delete
  or copy.
- [`references/patterns-layout-grid.md`](references/patterns-layout-grid.md)
  - table width, gutters and the ~768 px column hiding breakpoint.
- [`references/patterns-result-list.md`](references/patterns-result-list.md)
  - nested click lines reuse the domain and title treatment of a result
  row (favicon optional, no snippet).
- `cache.md` - the `cached` badge and `payload` action land on the cache
  inspection page; the two pages share the query text as the join key.

## Notes

- `search_log` is written for every request including cache hits, so a
  query searched five times has five rows; the `cached` column tells them
  apart from the network fetch that filled the cache.
- The v2 spec described this page as "clicked-result history" (one row per
  click). v3 inverts it: searches are the rows, clicks are nested. The
  `exa_user_history` MCP tool reads the same join.
- Retention: `search_log` and `clicks` follow `logs.retention_days`; old
  rows disappearing is retention, not data loss.

## User flow checkpoints

See `userflow-checkpoints.md` #15 (history, unfiltered), #16 (time
filtered), #17 (query text filtered), #21 (cached only), #22 (row with
nested clicks), #23 (row deleted).
