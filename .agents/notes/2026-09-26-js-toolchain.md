# 2026-09-26 — inline JS → bundled web/src modules (#207, v3/js-toolchain)

## What landed

All per-page inline `<script>` logic moved to ES modules under `crates/cauce-server/web/src/`
(format, sse, theme, search, answer, clipboard, engines, app), bundled+minified by esbuild
(`web/build.mjs`) into `crates/cauce-server/assets/app.js` — committed, inlined by
`crate::html::app_js()` on every template (same rust-embed delivery as htmx/json-enc/style;
no static-assets route exists). Unit tests: vitest + happy-dom under `web/tests/` (57 tests).
`mise run web` = pnpm install --frozen-lockfile + build + test + bundle-freshness diff;
wired into `validate`.

## Gotchas worth remembering

- **pnpm, not npm**: npm 10.8.3 crashes (`edgesOut`) on vitest 5.0.1's peer tree; pnpm's
  auto-install-peers handles it with no flags. pnpm's global content-addressable store
  (`~/.local/share/pnpm/store`) means worktrees share one copy — node_modules entries are
  hardlinks (nlink=2), so a second worktree costs ~0 extra disk. bun was ruled out: vitest
  needs the Node runtime anyway.
- **pnpm 11 build-script approval lives in `pnpm-workspace.yaml`** (`allowBuilds:
  {esbuild: true}`), NOT `package.json#pnpm.onlyBuiltDependencies` (ignored — install exits
  ERR_PNPM_IGNORED_BUILDS). esbuild's postinstall only validates the optional-dep binary,
  but the nonzero exit would break the gate.
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
