# Screen: Audit (`/audit`) and Trace (`/trace/<request_id>`)

Two read-only observability pages that ship in one step. `/audit` lists
every audited action (cache deletes, breaker resets, config saves) newest
first with actor and action filters; each row's `request_id` links to
`/trace/<id>`, which renders the same timeline `cauce trace <id>` prints in
the terminal. Contract source: `.agents/plans/v3/wave-2-ui-and-observability.md`
step W2-06 (issue #38) and the wave's "Settled inputs".

## ASCII mockup: audit

```
+------------------------------------------------------------------+
| cauce   search   history   dashboard      engines · cache · audit    |
|                                           settings · v3.0.0 · more  |
+------------------------------------------------------------------+
| Audit                                                            |
|                                                                  |
| [actor          v] [action                v] [filter]  clear     |
| 18 rows                                                          |
|                                                                  |
| when              actor  action          target      request     |
| 2026-09-23 01:12  ui     cache.delete    a1b2c3...   01J8Z4Q7X0.. |
|   > details  {"key":"a1b2c3...","query":"python asyncio"}        |
| 2026-09-23 01:05  ui     engine.reset    ddgs        01J8Z4P2M9.. |
|   > details                                                      |
| 2026-09-23 00:58  api    cache.delete    expired     01J8Z4N1K4.. |
|   > details  {"deleted":12}                                      |
| 2026-09-22 23:40  cli    config.save     search      -            |
|   > details                                                      |
|                                                                  |
| showing the newest 50 · raise `limit` (max 1000) for more          |
|                                                                  |
| request 01J8Z4Q7X0V2H9R6KQ3T5N8M1B                               |
+------------------------------------------------------------------+
```

Empty states:

```
| 0 rows                                                           |
| nothing audited yet. deleting a cache row or resetting a breaker |
| writes the first entry.                                          |
```

```
| [mcp            v] [cache.delete          v] [filter]  clear     |
| 0 rows                                                           |
| no audit rows match actor "mcp" and action "cache.delete".       |
```

## ASCII mockup: trace

```
+------------------------------------------------------------------+
| cauce   search   history   dashboard      engines · cache · audit    |
+------------------------------------------------------------------+
| Trace                                                            |
| 01J8Z4P2M9V2H9R6KQ3T5N8M1B            copy    <- back to audit   |
| search "python asyncio" · 2026-09-23 01:05:12 · 1 412 ms · ok    |
|                                                                  |
|    0 ms  |==                                | admission    3 ms  |
|    3 ms  |=                                 | cache miss   1 ms  |
|    4 ms  |====================              | replay     820 ms  |
|    4 ms  |=================================  | ddgs      1 380 ms |
| 1 390 ms |=                                 | merge (rrf)  9 ms  |
| 1 401 ms |                                  | render      11 ms  |
|                                                                  |
| spans                                                            |
| > replay   820 ms  ok  12 results                                |
| > ddgs   1 380 ms  ok   9 results                                |
| > merge     9 ms                                                 |
|                                                                  |
| request 01J8Z4Q7X0V2H9R6KQ3T5N8M1B                               |
+------------------------------------------------------------------+
```

Trace not found:

```
| Trace                                                            |
| 01J8ZZZZZZZZZZZZZZZZZZZZZZ                                       |
| no trace for this request id. traces are kept for                |
| logs.retention_days days (currently 14).                         |
```

Malformed id (`/trace/not-an-id`) returns 400 with the same page frame and
the line `that is not a request id`.

## Behavior: audit

- Data path: `/audit` and `GET /api/audit` share one handler (content
  negotiation on `Accept`, the page being the HTML arm); filters (`actor`,
  `action`), `since` and `limit` are the API's own params with the API's
  defaults (50 rows, max 1000). A test asserts the HTML rows
  equal the JSON rows for the same request.
- Rows newest first. Columns: when (local `YYYY-MM-DD HH:MM`), actor (`ui`,
  `api`, `mcp`, `cli`), action (mono, dotted name such as `cache.delete`),
  target (key, engine id or config section, truncated with ellipsis),
  request (link).
- Filters are two `<select>` elements populated from the distinct actors
  and actions the store has seen, plus an `any` default; a plain GET form so
  it works without JS. `clear` appears only when a filter is active. The
  count line reads `N rows` and, when filtered, `N rows matching`.
- Details: a `<details>` under each row; the summary reads `details`, the
  body is the stored JSON pretty printed in a monospace block. Rows with no
  details omit the toggle.
- The request column shows the id's 8-character short form (the shared
  `short_id` convention) as the link text, the full id in `title`, and links to `/trace/<full id>`. Rows
  without a request id (CLI actions) show `-`.
- Acceptance (W2-06): a cache delete performed through `/cache` appears
  here with actor `ui` and action `cache.delete`.
- Footer shows the full `request_id` of the page render, copyable.
- All copy comes from `strings.rs` (`AUDIT_*`).

## Behavior: trace

- Same code path as the CLI: the page calls the `cauce trace` timeline
  renderer in `cauce-core` and displays its output; no second parser of the
  JSONL log. The timeline block is preformatted text in the mono face so
  the bar chart columns align; the spans list under it is HTML (one
  `<details>` per span, summary `engine · elapsed · status · N results`,
  body the span's raw fields).
- Header shows the traced id in full with a `copy` button (clipboard API,
  falls back to a selectable `<code>`), the request summary line (kind,
  query, timestamp, total elapsed, outcome) and a `back to audit` link.
- Acceptance (W2-06): the trace of a replay search lists the `replay`
  engine span.
- Missing trace: 404 with the page frame and the retention hint above.
  Malformed id: 400, same frame, no stack trace or raw error text.
- Footer shows the page render's own `request_id` (distinct from the traced
  id), copyable.
- All copy comes from `strings.rs` (`TRACE_*`).

## Reachable states (replay engine)

Populated audit: delete a cache row on `/cache`. Filtered-empty: filter by an
actor that never acted. Trace found: run a replay search, open `/trace/<its
request_id>`. Trace not found: any well-formed ULID never issued. Malformed:
`/trace/x`.

## Responsive

- Audit table: full width with gutters; below 768 px the `target` column
  hides, below 640 px `actor` and `action` share one cell (`ui ·
  cache.delete`) and the table container scrolls horizontally as a last
  resort. Details blocks scroll inside themselves.
- Trace: the preformatted timeline scrolls horizontally inside its block at
  390 px (it is a fixed-width chart); the header, summary line and spans
  list wrap normally. No page-level horizontal scroll.

## Design references

- [`references/tokens.md`](references/tokens.md) - caption size for table
  cells, mono meta for action names and ids, `neutral` role for the
  timeline bars, `success`/`error` for span status.
- [`references/patterns-states.md`](references/patterns-states.md) - empty
  copy with an action, 404/400 shown inline in the frame, never a bare
  error page.
- [`references/patterns-layout-grid.md`](references/patterns-layout-grid.md)
  - table width, gutters, the ~768 px column-hiding breakpoint shared with
  `history.md`.
- `history.md` - the audit table reuses the history table's column
  treatment (truncation, hidden columns, last-resort horizontal scroll).

## Notes

- Audit rows and the JSONL request log are W1-11 settled inputs; the audit
  schema is `ts, actor, action, target, request_id, details`.
- `request_id` values are UUIDv7 (W1-11); the 8-character prefix shown in
  the audit table is display only and never used for lookups.
- Retention for traces follows `logs.retention_days`; audit rows are not
  pruned in v3.0.

## User flow checkpoints

See `userflow-checkpoints.md` #30 (audit, unfiltered), #31 (audit,
filtered), #32 (trace found), #33 (trace not found).
