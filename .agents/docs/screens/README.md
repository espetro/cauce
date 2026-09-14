# Screen specs (oxe web UI + v0.2 dashboard)

ASCII mockups for layout/design validation before implementation. Each
file follows the same format: intro, `## ASCII mockup`, `## Behavior`,
`## Responsive`, `## Notes`.

- [search.md](search.md) - Screen: Search (`/search?q=...` and
  `/?q=...`). Shareable results url (back/forward, `&p=` paging),
  card-less Google-anatomy results (favicon + domain, blue title,
  two-line snippet, collapsed page-text `<details>`), cache
  transparency meta line, copy link / copy json share row, ai-overview
  stub slot (post-v0.2), landing, empty and error states. Same url
  serves HTML to browsers, Exa JSON to `Accept: application/json`
  clients. Grounds the browser search engine integration
  (`https://search.localhost/?q=%s`).
- [history.md](history.md) - Screen: History (`/history`). Clicked
  results from web UI and MCP agents, per-row `copy json` to share a
  past search's payload, filters, 30d default retention.
- [dashboard.md](dashboard.md) - Screen: Dashboard
  (`dist/dashboard/index.html`). Static, no-JS stats page built by
  `oxe stats build`: 6 metrics from `search_log` as a bento of inline
  SVG panels, top-queries table, zero-results list, client split.

Grounded on the v0.2 design memory: `search_log` schema
(ts, query_text, query_hash, source cache|network, backend,
result_count, duration_ms, client mcp|http|web-ui), stdlib-only build,
dark-mode friendly (color-scheme light dark, Geist embedded).
