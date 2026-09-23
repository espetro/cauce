# Screen: Engines (`/engines`)

One card per configured engine, showing the scheduler's live view of it
(breaker state, latency, reliability) and the three operator actions the
plan allows: reset the breaker, run a test query against that engine only,
enable or disable it. This is the "why did ddgs stop answering" page.
Contract source: `.agents/plans/v3/wave-2-ui-and-observability.md` step W2-05
(issue #37) and the wave's "Settled inputs".

## ASCII mockup

```
+------------------------------------------------------------------+
| cauce   search   history   dashboard      engines · cache · audit    |
|                                           settings · v3.0.0 · more  |
+------------------------------------------------------------------+
| Engines                                                          |
| 3 configured · 2 enabled · 1 breaker open                        |
|                                                                  |
| +----------------------------+  +----------------------------+   |
| | ddgs            exec · t1  |  | replay        replay · t1  |   |
| | [ Open ]  retries in 42s   |  | [ Closed ]                 |   |
| | enabled      yes           |  | enabled      yes           |   |
| | ewma         1 840 ms      |  | ewma         12 ms         |   |
| | last ok      01:02 (9m)    |  | last ok      01:10 (1m)    |   |
| | last error   429 too many  |  | last error   -             |   |
| | p95          2 310 ms      |  | p95          18 ms         |   |
| | reliability  0.62          |  | reliability  1.00          |   |
| | requests     41 today      |  | requests     7 today       |   |
| |                            |  |                            |   |
| | [reset breaker] [disable]  |  | [reset breaker] [disable]  |   |
| | [test query.......] [run]  |  | [test query.......] [run]  |   |
| |                            |  | 1 result · 14 ms           |   |
| |                            |  | > Tokio tutorial  tokio.rs |   |
| +----------------------------+  +----------------------------+   |
| +----------------------------+                                   |
| | wiki       declarative · t2|                                   |
| | [ Closed ]  disabled       |                                   |
| | ...                        |                                   |
| | [reset breaker] [enable]   |                                   |
| +----------------------------+                                   |
|                                                                  |
| request 01J8Z4Q7X0V2H9R6KQ3T5N8M1B                               |
+------------------------------------------------------------------+
```

Breaker chip states (exact labels, one chip per card):

```
[ Closed ]                       success role, no time remaining
[ Open ]  retries in 42s         error role, countdown from breaker until
[ HalfOpen ]  probing            warning role, shown after a reset or when
                                 the cooldown elapsed and one probe is allowed
```

## Behavior

- Data path: `GET /api/engines` with `Accept: text/html` renders the page;
  the JSON body carries the same per-engine fields the cards show. Reset
  and toggle actions return the single re-rendered card (`engine_card`
  fragment) so HTMX swaps it in place without a page reload.
- Card header: engine id in label weight, then `kind · tier N` in mono meta
  (kind is `declarative`, `exec` or `replay`). An engine present in the
  running scheduler but missing from the config file is still shown, with a
  `not in config` note; one in config but not running shows `not running`.
- Breaker chip: `Closed`, `Open` or `HalfOpen`, exactly those words, coloured
  by role (`success`, `error`, `warning`). `Open` carries a `retries in Ns`
  countdown rendered server side from the breaker's `until` timestamp; the
  page does not tick client side, a reload refreshes it.
- Stats list (definition list, label left, value right, one per line):
  enabled, ewma, last ok, last error, p95, reliability, requests today.
  Times are local `HH:MM` plus a relative phrase in parentheses; a value the
  scheduler has not seen yet renders as `-`, never blank or `null`.
- Actions row:
  - `reset breaker`: `POST /api/engines/<id>/reset` with
    `X-Cauce-Client: ui`; the returned card shows `HalfOpen`. Audited with
    action `engine.reset`, actor `ui`. Acceptance (W2-05): with a `blocked`
    replay engine the card shows `Open`, and one reset flips it to
    `HalfOpen`.
  - `enable` / `disable`: `POST /api/engines/<id>/enable` or `/disable`,
    writes `engines.<id>.enabled` to the TOML through the same code path
    `PUT /api/config` uses, audited with action `engine.enable` or
    `engine.disable`. The button label is the action that will happen
    (`disable` on an enabled engine). When `CAUCE_ENGINES` pins the set, the
    button is disabled with the hint `pinned by CAUCE_ENGINES`.
  - test query: an inline form with one text input (default `test`) and a
    `run` button. Submits `GET /api/search?q=<text>&engines=<id>` with
    `Accept: text/html` and renders the returned result list fragment under
    the actions row (same `results.html` partial as the search page, so
    result anatomy matches). A meta line above the fragment shows
    `N results · <elapsed> ms` or the engine's error class (`no results`,
    `blocked`, `timeout`). A second run replaces the first.
- Summary line under the heading: `N configured · N enabled · N breaker
  open` (the last part omitted when zero).
- Footer shows the full `request_id` of the page render, copyable. Each
  swapped-in card fragment carries its own `request_id` as a
  `data-request-id` attribute so a failing reset can be traced.
- All copy comes from `strings.rs` (`ENGINES_*`).

## Reachable states (replay engine)

Configure two replay engines; drive one with the replay fault injection to
`blocked` to reach `Open`, reset it to reach `HalfOpen`, let it answer to
reach `Closed`. `no results`, `timeout` and `error` test-query outcomes
come from the replay fixtures as well. No card state needs live network.

## Responsive

- Card grid `repeat(auto-fit, minmax(20rem, 1fr))` (same rule as the
  dashboard panels): two or three columns at 1280 px, one column at 390 px.
- Inside a card at 390 px: header wraps kind/tier under the id, the chip
  stays on the header line, the stats list keeps label and value on one
  line (values are short), actions wrap to two rows, the test result list
  uses the search page's mobile anatomy. No horizontal scroll.

## Design references

- [`references/tokens.md`](references/tokens.md) - color roles for the
  three breaker states, mono meta for kind/tier, `--radius-box` for cards,
  `resting` elevation only (no hover lift; these are not links).
- [`references/patterns-layout-grid.md`](references/patterns-layout-grid.md)
  - panel grid and the ~700 px single-column breakpoint.
- [`references/patterns-states.md`](references/patterns-states.md) -
  failure lines name the engine and error class; nothing renders on
  success beyond the result list.
- [`references/patterns-result-list.md`](references/patterns-result-list.md)
  - the inline test results reuse the search result anatomy unchanged.
- `dashboard.md` - the dashboard's engine table links to `/engines#engine-<id>`;
  cards carry that id.

## Notes

- Breaker semantics (Closed, Open with cooldown, HalfOpen single probe) are
  the W1-06 scheduler contract; this page only renders them.
- Health numbers (ewma, p95, reliability) come from the in-memory health
  table, so they reset on restart; `requests today` comes from `search_log`.
- Enable/disable edits the config file; the change to the running scheduler
  applies on restart, and the card says so in a hint after a toggle
  (`saved; applies after restart`).

## User flow checkpoints

See `userflow-checkpoints.md` #27 (engines, breaker open), #28 (engines,
after reset), #29 (engines, inline test query).
