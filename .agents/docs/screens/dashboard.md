# Screen: Dashboard (`/dashboard`)

`/dashboard` is a live SPA route sharing the app header (oxe / search /
history / dashboard / (?) / settings / GitHub / version). It fetches
aggregate usage and cache stats from `GET /api/stats` and renders a
responsive panel grid. Panels for data the backend does not yet aggregate
show a muted "no search log data yet" placeholder instead of an empty
chart.

## ASCII mockup

```
+------------------------------------------------------------------+
| oxe   search   history  [dashboard]  (?)  settings  [gh] v0.5.0  |
+------------------------------------------------------------------+
| oxe stats                                                        |
| window: last 30 days                                             |
|                                                                  |
| +---------------------------+  +--------------------------------+|
| | searches per day          |  | cache hit rate                 ||
| | no search log data yet    |  | 78%   (N total hits)           ||
| +---------------------------+  +--------------------------------+|
| +---------------------------+  +--------------------------------+|
| | network latency           |  | client split                   ||
| | no search log data yet    |  | no search log data yet         ||
| +---------------------------+  +--------------------------------+|
| +---------------------------------------------------------------+|
| | cache                                                          ||
| | rows  340                     unexpired  280                   ||
| | db size 1.2 MB                newest  2026-09-14 19:42         ||
| +---------------------------------------------------------------+|
+------------------------------------------------------------------+
```

## Behavior

- Live SPA route: panels fetch `GET /api/stats` via the router `loader()`
  (no `useEffect` fetch) and show the cache hit-rate percentage
  (unexpired/rows), total hits, and a cache table (rows, unexpired, db
  size, newest). Loader errors render in the panel body via the route's
  error boundary.
- Panels for data the backend does not yet aggregate (searches per day,
  network latency, client split) render a flat muted "no search log data
  yet" line instead of an empty chart; the page still renders.
- These log-derived panels become real charts once `/api/stats` grows the
  corresponding fields; no route changes expected, only a widened response
  schema (`components["schemas"]["StatsResponse"]` grows, generated types
  follow).

## Responsive

- Panel grid: `repeat(auto-fit, minmax(20rem, 1fr))` — multi-column on
  desktop, single column stacked below ~700px. Always full width.

## Design references

- [`references/tokens.md`](references/tokens.md) - canonical type scale,
  spacing, color roles behind the panel grid.
- [`references/patterns-layout-grid.md`](references/patterns-layout-grid.md)
  - panel grid (`repeat(auto-fit, minmax(20rem, 1fr))`), header anchoring,
  the ~700px single-column breakpoint.

## Notes

- No params planned: the dashboard window is a build-time constant, not
  URL state.
- The v0.2 static, no-JS `oxe stats build` CLI and its `GET /cache/stats`
  endpoint are gone; `/dashboard` is the only dashboard surface in the
  v0.5.0 app, served like every other route.
