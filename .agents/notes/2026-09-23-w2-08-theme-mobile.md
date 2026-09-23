# 2026-09-23 — W2-08 theme and mobile layout

- Shared shell pattern: `templates/header.html` + `templates/theme_head.html` included by
  every page template. Pages pass `nav_active: &'static str` on their Askama context; the
  header compares it to emit `aria-current="page"` (empty string = no current nav item, e.g.
  `/trace/{id}`). New pages must add the field and the two includes.
- Nav structure is locked by `.agents/docs/screens/README.md`: primary
  `search · history · dashboard`, operator group `engines · cache · audit` (quieter styling,
  collapses into a CSS-only `<details>` "more" menu below 700 px), `settings` rendered
  separately, version label hidden below 640 px. `/cache` is operator, never primary.
- Theme: `:root` light vars, `prefers-color-scheme: dark` media block, plus
  `:root[data-theme="dark"]` override; `:not([data-theme="light"])` on the media query keeps
  a stored light winning over a dark OS. Toggle cycles system -> light -> dark; explicit
  choices persist in `localStorage["cauce-theme"]` (system removes the key). The inline
  `theme_head.html` script applies the stored choice during `<head>` parse — required before
  first paint or a saved dark choice flashes light.
- tracing-core flake worth remembering: callsite `Interest` is a PROCESS-GLOBAL cache. A
  callsite first registered on a thread with no dispatcher caches `Interest::never` and the
  span is silently disabled for every subscriber. `tracing::dispatcher::set_default` in a
  test is thread-local and does NOT protect callsites another parallel test touches first;
  install the test dispatch via `set_global_default` too (audit.rs trace test does).
- DOM-assertion idiom (tests/ui_shell.rs): strip `<script>`/`<style>` bodies before
  landmark/element counting — inlined htmx and CSS selectors contain literal `<body`,
  `aria-current` etc. and produce false positives otherwise. Audit tests asserting "no
  details toggle" must match the row-adjacent `<details><summary>` pair, not bare `<summary>`
  (the header menu always renders one).
- Pre-existing breaks fixed on this branch (main was red): `HistoryFilter.cached` missing in
  `handlers.rs` engine_views query (commit 97950a2). Headless (`--no-default-features
  --features mcp`) check emits a pre-existing `CacheListing` dead-code warning — fields are
  only read by the ui-gated cache page; check still exits 0.
