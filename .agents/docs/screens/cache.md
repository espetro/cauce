# Screen: Cache (`/cache`)

The shared TTL cache made visible: every `cache_entries` row the pipeline
wrote for the web UI, the HTTP API and MCP agents, newest first, with the
stored payload one click away and audited delete controls. This is the page
the owner opens when a query "looks stale" or when an agent asks why it got
a cached answer. Contract source: `.agents/plans/v3/wave-2-ui-and-observability.md`
step W2-04 (issue #36) and the wave's "Settled inputs".

Cache is a cross-screen feature; this route is its inspection and
maintenance surface, not its home. The daily signals live elsewhere: the
search page's `cached · <age>` badge, history's `source` column and
`cached only` filter (`history.md`), the dashboard's hit-rate and cache
panels, and the settings cache block (`settings.md`). `/cache` sits in the
header's operator group (`engines · cache · audit`), not in the primary
nav, and is reached from history's `payload` links, the dashboard cache
panel and settings.

## ASCII mockup

```
+------------------------------------------------------------------+
| cauce   search   history   dashboard      engines · cache · audit    |
|                                           settings · v3.0.0 · more  |
+------------------------------------------------------------------+
| Cache                                                            |
|                                                                  |
| [filter cached queries...            ] [filter]  clear           |
| 34 entries                                                       |
|                          [delete expired]  [delete all]          |
|                                                                  |
| > python asyncio                                        [delete] |
|   2026-09-23 01:10 · expires in 41m · 3 hits · replay,ddgs · 12 KB|
| > rust tokio                                            [delete] |
|   2026-09-22 23:58 · expired 2h ago · 1 hit · ddgs · 9 KB        |
| v kubernetes ingress                                    [delete] |
|   2026-09-22 22:31 · expires in 12m · 7 hits · replay · 18 KB    |
|   +------------------------------------------------------------+ |
|   | {                                                          | |
|   |   "query": "kubernetes ingress",                           | |
|   |   "results": [ ... pretty JSON, monospace, scrolls ... ]   | |
|   +------------------------------------------------------------+ |
|                                                                  |
| <- newer                                              older ->   |
|                                                                  |
| request 01J8Z4Q7X0V2H9R6KQ3T5N8M1B                               |
+------------------------------------------------------------------+
```

Empty state (no rows at all):

```
| Cache                                                            |
| [filter cached queries...            ] [filter]                  |
| 0 entries                                                        |
| no cached queries yet. run a search and it lands here.           |
| request 01J8Z...                                                 |
```

Empty state (filter matched nothing):

```
| [kubernetes                          ] [filter]  clear           |
| 0 entries                                                        |
| nothing cached matches "kubernetes".                             |
```

## Behavior

- Data path: `/cache` and `GET /api/cache` share one handler (content negotiation on
  `Accept`); the page renders what the API returns for the same query
  string (`q`, `limit`, `offset`). There is no second data path; a test asserts HTML
  rows equal the JSON rows for the same request.
- One row per cache entry, newest `created` first. Row line shows in this
  order: query text, created (local `YYYY-MM-DD HH:MM`), expiry as a relative
  phrase (`expires in 41m` or `expired 2h ago`), hit count, engine ids that
  answered (comma separated), stored size. Expired rows keep their place in
  the list but render muted (`base-content` at reduced opacity), never
  hidden: the owner must be able to see what "delete expired" will remove.
- The row is a `<details>`; opening it fetches `GET /api/cache/<key>` once
  (HTMX `toggle once`) and renders the stored payload as pretty JSON in a
  monospace block. A `loading...` line occupies the block until the fetch
  returns; a fetch error renders one inline `error: could not load payload
  (<status>)` line inside the block, no toast.
- Filter: the search box narrows rows server side (`q` param, same FTS the
  API exposes: it matches query text, titles and snippets, so the placeholder
  says `filter cached queries` and the count line says `matching entries`
  when a filter is active). `clear` appears only while a filter is active
  and returns to `/cache`. History's `payload` link arrives as
  `/cache?q=<query>#<key>`; the matching row renders open.
- Pagination: `offset=<n>` window into the newest-first list, 50 rows per page, `newer`
  / `older` links shown only when a neighbour page exists. Filtered lists are
  capped at one page and say so under the count (`showing the newest 50
  matches`).
- Deletes: three controls, all issued with `X-Cauce-Client: ui` so the audit
  row carries actor `ui`:
  - per-row `delete`: `DELETE /api/cache/<key>`, native confirm, row removed
    in place (HTMX `outerHTML` delete), count line decrements.
  - `delete expired`: `DELETE /api/cache?expired=true`, confirm text names
    what goes (`Delete all expired entries?`), page reloads.
  - `delete all`: `DELETE /api/cache?all=true`, styled with the `error`
    color role, confirm text `Delete every cached entry? Searches will hit
    the network until the cache refills.`, page reloads.
  Acceptance (W2-04): after a delete the row is gone, an `audit` row with
  actor `ui` exists, and the next identical search reports `Network`.
- Footer shows the full `request_id` of the render as a `<code>` element
  the user can select and copy; never abbreviated in visible text.
- No JavaScript beyond vendored HTMX; the filter form and the pager work
  with JS disabled. Deletes need HTMX and say so in a `<noscript>` hint.
- All copy comes from `strings.rs` (`CACHE_*`).

## Reachable states (replay engine)

Every state above is reachable with the `replay` engine and no fixture-only
markup: empty (fresh DB), populated (run a replay search), expired (replay
search with `ttl_s=1` then wait), filtered-empty (filter for a nonsense
word), payload error (delete the row from another tab, then expand it).

## Responsive

- Content column matches the history table width (full width with 16-24 px
  gutters below 768 px, `max-width` ~72rem above). No horizontal scroll at
  390 px: the meta line under the query wraps onto two lines, the row
  `delete` button drops under the meta line, the pretty JSON block scrolls
  inside itself (`overflow-x: auto`) rather than widening the page.
- `delete expired` / `delete all` stack vertically below 640 px.

## Design references

- [`references/tokens.md`](references/tokens.md) - type scale (caption/meta
  for the row meta line, mono meta for engines and size), `error` color role
  for `delete all`, spacing rhythm between rows (`md`).
- [`references/patterns-states.md`](references/patterns-states.md) - inline
  error lines instead of toasts, one-sentence empty state with an action.
- [`references/patterns-layout-grid.md`](references/patterns-layout-grid.md)
  - sticky header, full-bleed canvas, table-like list width and gutters.
- [`references/patterns-typography.md`](references/patterns-typography.md)
  - query text as the only emphasized text per row (label weight 500), all
  meta at caption size.

## Notes

- The `cache_entries` schema and FTS index are W1-11 settled inputs; this
  page adds no columns.
- Confirm dialogs are the browser's native `confirm()` via `hx-confirm`; a
  styled modal is a W2-08 or later concern, not part of this contract.
- `/cache/stats` from v0.2 is gone; the dashboard's cache panel reads
  `/api/stats` and links here.

## User flow checkpoints

See `userflow-checkpoints.md` #24 (cache, unfiltered), #25 (cache,
filtered, row open), #26 (cache, row deleted).
