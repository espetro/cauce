# Screen specs (cauce web UI + v0.2 dashboard)

ASCII mockups for layout/design validation before implementation. Each
file follows the same format: intro, `## ASCII mockup`, `## Behavior`,
`## Responsive`, `## Notes`, `## User flow checkpoints`.

- [landing.md](landing.md) - Screen: Landing (`/`, no query). Centered
  hero search input with the traditional / AI mode toggle
  (persisted in localStorage), one-line per-mode hint, autofocus,
  no-JS form fallback, busy submit state. Suggestions dropdown
  (local search_log history always, debounced DDG ac opt-in,
  keyboard nav, aria listbox) shared with the results-header input.
- [search.md](search.md) - Screen: Search (`/search?q=...` and
  `/?q=...`), two modes in one file. Traditional: shareable results
  url (back/forward, `&p=` paging), card-less Google-anatomy results
  (favicon + domain, blue title, two-line snippet, collapsed page-text
  `<details>`), cache transparency meta line, copy link / copy json
  share row. AI (`&mode=ai`): answer-first streaming view with inline
  citation markers, streaming cursor and [stop] state, horizontal
  source cards, related questions, cached-answer replay, toggle back
  to classic. Shared empty/error states, header-input suggestions
  dropdown (same as landing), content negotiation
  (HTML to browsers, Exa JSON to `Accept: application/json`).
  Grounds the browser search engine integration
  (`https://search.localhost/?q=%s`).
- [history.md](history.md) - Screen: History (`/history`). One row per
  search from every client (ui/api/mcp) with clicked results nested under
  it, a `source` column carrying the cache signal (`cached · <age>`),
  filters `since` / `q` / `cached=1`, re-run, copy json, payload link to
  `/cache`, audited delete. Rewritten for v3 (was clicked-result history).
- [dashboard.md](dashboard.md) - Screen: Dashboard
  (`dist/dashboard/index.html`). Static, no-JS stats page built by
  `cauce stats build`: 6 metrics from `search_log` as a bento of inline
  SVG panels, top-queries table, zero-results list, client split.
- [cache.md](cache.md) - Screen: Cache (`/cache`). Inspection and
  maintenance surface for the shared TTL cache: paginated entries, FTS
  filter, payload expand, audited row / expired / all deletes. Not a
  primary nav item; reached from history, dashboard and settings.
- [engines.md](engines.md) - Screen: Engines (`/engines`). One card per
  engine: kind, tier, breaker chip (`Closed` / `Open` + countdown /
  `HalfOpen`), health stats, reset breaker, enable/disable, inline test
  query against that engine only.
- [audit.md](audit.md) - Screens: Audit (`/audit`) and Trace
  (`/trace/<id>`). Audited actions newest first with actor/action filters
  and details; trace renders the `cauce trace` timeline as HTML.
- [settings.md](settings.md) - Screen: Settings (`/settings`). Form view
  of the TOML through `PUT /api/config`: search, engines, admission,
  logging, cache block (stats + bulk deletes), AI answers (greyed until
  W4); templates verbatim, env-pinned fields read-only, inline validation.

Header nav (W2-08): primary `search · history · dashboard`; operator
group `engines · cache · audit`, plus `settings`; the operator group
collapses into a `more` menu below 700 px.

Grounded on the v0.2 design memory: `search_log` schema
(ts, query_text, query_hash, source cache|network, backend,
result_count, duration_ms, client mcp|http|web-ui), stdlib-only build,
dark-mode friendly (color-scheme light dark, Geist embedded).

Stack decision: `.agents/drafts/tech-stack.md` (rev 2) — Preact + Vite
CSR, daisyUI 5 on Tailwind 4, tooling via `mise.toml`.
