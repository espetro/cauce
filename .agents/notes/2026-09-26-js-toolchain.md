# 2026-09-26 — inline JS → bundled web/src modules (#207, v3/js-toolchain)

## What landed

All per-page inline `<script>` logic moved to ES modules under `crates/cauce-server/web/src/`
(format, sse, theme, search, answer, clipboard, engines, app), bundled+minified by esbuild
(`web/build.mjs`) into `crates/cauce-server/assets/app.js` — committed, inlined by
`crate::html::app_js()` on every template (same rust-embed delivery as htmx/json-enc/style;
no static-assets route exists). Unit tests: vitest + happy-dom under `web/tests/` (57 tests).
`mise run web` = npm ci + build + test + bundle-freshness diff; wired into `validate`.

## Gotchas worth remembering

- **npm 10.8.3 crashes** (`edgesOut` TypeError) resolving vitest 5.0.1's peer tree under
  default mode. `crates/cauce-server/.npmrc` with `legacy-peer-deps=true` fixes it; all
  required peers (vite) are explicit devDeps so `npm ci` stays deterministic.
- **happy-dom leaks `dataset` between tests** and follows anchor `href="#"` with a network
  call — tests delete dataset keys and use a `clickNoNav()` helper that preventDefaults.
- **Tests that pin inline-JS strings break when JS moves to a bundle**: `answer.rs`/`pages.rs`
  assertions on "/answer?q=", "ai-mode", "/api/pages" now pin markup (element ids,
  `<body data-index-on-click`) instead. Pins that survive: `var Q =`, `var S =`,
  `hx-ext="sse"`, `sse-connect`, `htmx.defineExtension('json-enc'`, the theme_head
  `localStorage.getItem("cauce-theme")` block (still inline by design — must run before CSS).
- **Askama can't interpolate inside `<script>` into data**: per-page config rides `data-*`
  attrs now (body data-index-on-click/data-query-hash, engines data-i18n-*, answer
  data-endpoint/data-accept/data-headers, history data-page/data-copied-label).
- **curl needs `Accept: text/html`** — /history, /engines, /archive content-negotiate to JSON
  on `*/*`.
- **Git author env**: this box exports GIT_AUTHOR_* pointing at the noreply identity;
  `git -c user.email=...` loses to them. Commit with
  `env GIT_AUTHOR_EMAIL=... GIT_COMMITTER_EMAIL=... git commit -s`.
