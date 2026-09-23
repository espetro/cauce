# Screen: Settings (`/settings`)

A form view of the TOML config file, saved through `PUT /api/config`.
Every field shows the value from the file as written; `${env:...}`
templates stay templates, and a field an environment variable overrides
is shown read-only with the variable named. Contract source:
`.agents/plans/v3/wave-2-ui-and-observability.md` step W2-07 (issue #39)
and the wave's "Settled inputs". Replaces the v2 settings dialog
(`userflow-checkpoints.md` #18, #19), which was a modal over the search
page; v3 gives settings a route.

## ASCII mockup

```
+------------------------------------------------------------------+
| cauce   search   history   dashboard      engines · cache · audit    |
|                                           settings · v3.0.0 · more  |
+------------------------------------------------------------------+
| Settings                                                         |
| editing ~/.config/cauce/config.toml · changes apply on restart   |
|                                                                  |
| Search                                                           |
|   Deadline (ms)          [ 2500      ]                           |
|   Cache TTL (s)          [ 3600      ]   set by CAUCE_TTL_S      |
|   Hedge threshold (ms)   [           ]   lands in wave 3         |
|                                                                  |
| Engines                                                          |
|   ddgs     exec         [x] enabled   tier [1 v]  proxy [direct ]|
|   replay   replay       [x] enabled   tier [1 v]  proxy [direct ]|
|   wiki     declarative  [ ] enabled   tier [2 v]  proxy [socks5:]|
|                                                                  |
| Admission                                                        |
|   Max queue wait (ms)            [ 500  ]                        |
|   Max concurrent per engine      [ 4    ]                        |
|                                                                  |
| Logging                                                          |
|   JSONL retention (days)         [ 14   ]                        |
|                                                                  |
| Cache                                                            |
|   340 entries · 280 unexpired · 1.2 MB · newest 01:12            |
|   [delete expired]  [delete all]            browse entries ->    |
|                                                                  |
| AI answers                                    lands in wave 4    |
|   Base URL   [ http://localhost:8317/v1              ]           |
|   API key    [ ${env:BIFROST_API_KEY}                ]           |
|              BIFROST_API_KEY is set                              |
|   Model      [ claude-sonnet-4-5          v]                     |
|   [ ] enabled  (the answer pipeline arrives in wave 4)           |
|                                                                  |
|                                        [Save]  saved 01:12       |
|                                                                  |
| request 01J8Z4Q7X0V2H9R6KQ3T5N8M1B                               |
+------------------------------------------------------------------+
```

Validation error (inline, next to the field, form stays filled):

```
|   Deadline (ms)          [ -5        ]                           |
|                          deadline_ms must be 0 or more           |
|                                        [Save]  not saved: 1 error|
```

## Behavior

- Data path: `/settings` and `GET /api/config` share one handler (content
  negotiation on `Accept`), rendering from the same parsed config; `PUT /api/config`
  with the form body (HTMX, `X-Cauce-Client: ui`) writes it, audited as
  `config.save` with actor `ui` and the changed keys in details.
- Sections, in order: Search, Engines, Admission, Logging, Cache, AI
  answers. Each
  is a `<fieldset>` with a `<legend>`; field labels are the human name, the
  TOML key appears as the input `name` (`search.deadline_ms`), so a test can
  find fields by key.
- Templates are never resolved: an `${env:NAME}` value renders verbatim in
  its input and is written back byte for byte on save. Acceptance (W2-07):
  the `${env:BIFROST_API_KEY}` template survives a save round trip
  unchanged; editing `search.deadline_ms`, saving and reloading shows the
  new value.
- Environment overrides: a field whose value is pinned by an env var
  (`CAUCE_*`) renders disabled with the hint `set by CAUCE_TTL_S` beside it
  and is excluded from the PUT body. When `CAUCE_ENGINES` pins the engine
  set, the enabled checkboxes render as text (`enabled` / `disabled`) with
  one hint above the list.
- Engines list: one row per configured engine: id (label weight), kind
  (mono meta), enabled checkbox, tier select (`default`, 1, 2, 3), egress
  proxy text input (placeholder `direct`). Same set as `/engines`; the two
  pages link to each other.
- Cache block: not a form section but a status line plus actions, placed
  here because the TTL that governs it is two sections up. Reads
  `N entries · N unexpired · <db size> · newest HH:MM` from `/api/stats`
  (same numbers as the dashboard cache panel), offers `delete expired` and
  `delete all` (same endpoints, confirms and audit as on `/cache`, page
  section refreshes in place) and a `browse entries` link to `/cache`.
- AI answers section is visibly greyed (reduced opacity, `lands in wave 4`
  in the legend) but its inputs are live so the config can be prepared:
  base URL, API key (template shown verbatim, plus an env status line
  `BIFROST_API_KEY is set` or `is not set` when the value is an env
  template), model picker (text input with a `<datalist>` filled from
  `GET {base_url}/models` using the resolved key; when the list cannot be
  fetched the hint `model list unreachable; type a model name` appears and
  free text still saves), `enabled` checkbox disabled until W4.
- Save: one `Save` button at the bottom. On success the status region
  (`role="status"`, `aria-live="polite"`) reads `saved HH:MM`; on validation
  failure each offending field gets an inline error line under it and the
  status reads `not saved: N errors`; the form keeps the user's values. A
  transport failure (5xx, network) renders `error: could not save (<status>)`
  in the status region, no toast.
- Restart note under the heading: `changes apply on restart`, because the
  running scheduler does not hot-reload in v3.0.
- Without JS the form still renders; saving needs HTMX and a `<noscript>`
  line says so.
- Footer shows the full `request_id`, copyable.
- All copy comes from `strings.rs` (`SETTINGS_*`).

## Reachable states (replay engine)

Default: fresh config. Env-pinned: start with `CAUCE_TTL_S=60`. Engines
pinned: `CAUCE_ENGINES=replay`. Validation error: submit a negative
deadline. Model list unreachable: point `ai.base_url` at a closed port.
No state needs a live AI provider.

## Responsive

- Form column `max-width` ~40rem, left aligned within the page column.
- Labels sit left of inputs on one line at 1280 px; below 640 px each label
  stacks above its input and the engine row wraps to two lines (id and kind
  on the first, controls on the second). No horizontal scroll at 390 px.

## Design references

- [`references/tokens.md`](references/tokens.md) - label size/weight for
  field labels, caption for hints and errors, `error` role for inline
  validation text, `neutral` for disabled env-pinned fields,
  `--radius-field` for inputs.
- [`references/patterns-states.md`](references/patterns-states.md) - inline,
  specific error copy; no vanishing toasts; the status region as the
  single feedback slot.
- [`references/patterns-layout-grid.md`](references/patterns-layout-grid.md)
  - page frame and gutters; the ~640 px label-stacking breakpoint.
- [`references/patterns-typography.md`](references/patterns-typography.md)
  - legends at heading size, weight ceiling 600.

## Notes

- Config schema and `${env:...}` template semantics are W0-11 settled
  inputs; the page must not add keys.
- The cache block is the settings half of the cross-screen cache feature
  described in `cache.md`; `/cache` keeps the list, filter and payload
  inspection.
- The v2 theme fieldset (System / Light / Dark) is not here: W2-08 puts the
  theme toggle in the header and stores it in `localStorage`.
- Secrets are never displayed resolved; if a user types a literal key
  instead of a template, it is written to the file as typed and shown as
  typed (this is the owner's own machine and config file).

## User flow checkpoints

See `userflow-checkpoints.md` #34 (settings, default), #35 (settings,
saved), #36 (settings, validation error).
