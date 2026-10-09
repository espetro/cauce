# 2026-10-08 — FX-03 /search parity page

Landed the SPA shell + `/app/search` behind `/app` (variant E chrome).

## svelte-check `--tsgo` root cause (gate was a silent no-op)

- svelte-check emits virtual `++*.svelte.ts` files under
  `<workspace>/.svelte-check/svelte/` mirroring each source's
  **workspace-relative** path, then generates an overlay tsconfig whose
  `rootDirs` pairs the **tsconfig's directory** with that mirror root.
- The pairing only works when the tsconfig sits at the workspace root —
  the dir svelte-check was launched from (the crate root here).
- With `web/src/spa/tsconfig.json` nested three levels deep, every
  relative import inside `.svelte` files failed to resolve AND the
  generated `exclude` silently dropped all real `.ts` inputs — the gate
  passed while checking nothing.
- Fix: `tsconfig.spa.json` at the crate root; `web/src/spa/tsconfig.json`
  is now an alias shim for editors. `.svelte-check/` is gitignored.

## Contract notes worth keeping

- `GET /api/search/stream` accepts exactly (q,page,lang,time_range,
  safesearch,engines,client). `stream` is a page-route flag, NOT an API
  param — `apiParams()` must whitelist before building the EventSource
  URL or the server 400s inside the stream.
- `/search` 400 order: `required("q")` → `stream=1 flag` → allowlist →
  typed params → `validate_pin`. SSR validates pin before rendering, so
  the SPA must `await loadCapabilities()` (deduped) before pin-checking —
  EventSource cannot surface a 400 as a page error.
- Theme cycle is system→light→dark; the pre-paint inline script in
  `index.html` reads `localStorage["cauce-theme"]` to avoid the flash.
- `/app` bare path: `pathname.slice(4)` yields `""` — normalize to `/`.

## Import-specifier convention in this tree

Relative TS imports carry `.js`; `.svelte.ts` state modules import as
`.svelte.js`; `.svelte` components as `.svelte`; `.json` stays `.json`.
Naive sed fixes corrupt this — use a small script.

## Parity deltas vs HTMX (golden path)

- None behavioral. AI mode is a segmented control in the omnibox tool
  row rather than the SSR `mode-pill` button (same `ask` submit).
- Assist section mounts eagerly with an inert trigger until `meta`
  arms context — identical to SSR's `disabled` semantics.
- SPA needs JS for first paint (inherent); `/app` is additive.
