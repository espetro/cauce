# TS migration — web frontend to strict TypeScript in 3 steps

Cross-wave plan (between W6 and W7 UI work). Priority P1. Parent:
`../2026-09-21-v3-rust-core.md`. Index: `README.md`. Issues: #212, #213, #214.

## Goal

Move `crates/cauce-server/web/` from untyped `.js` to TypeScript level (b):
every module is `.ts`, `tsc --noEmit` under `strict` is a validation gate, and
the bundler is `rolldown` (esbuild dropped). The committed `assets/app.js`
remains the single inlined script `rust-embed` serves — no runtime surface
change.

Decomposed into three landable steps so each PR stays reviewable and the
committed-bundle freshness gate keeps passing at every step.

## Step 1 — toolchain + leaf modules (issue #212, this PR)

- `typescript@7` devDep (native tsgo compiler; `tsc --noEmit`, LSP-compatible).
- `web/tsconfig.json`: `strict`, `noEmit`, `incremental`, DOM lib, bundler
  module resolution.
- `esbuild` → `rolldown` in `web/build.mjs`; keep `format: "iife"`, `minify`,
  the MPL banner (applied via `output.postBanner` so it survives minification),
  and the `</script` post-build scan.
- `htmx.org` + `htmx-ext-json-enc` become real bundled deps pinned to the
  vendored versions (2.0.4 / 2.0.2); vendored `assets/htmx.min.js` and
  `assets/json-enc.js`, their `|safe` template tags, and the `HTMX_JS` /
  `JSON_ENC_JS` statics are removed. Pins documented in
  `assets/THIRD_PARTY_NOTICES.md`.
- `web/src/globals.d.ts` (window expandos: `htmx`, `S`, `AS`, `Q`,
  `assistContext`, `Element.cauceEventSource`, the `cauce:sse` CustomEvent) and
  `web/src/htmx.d.ts` (the extension/event surface the codebase uses:
  `defineExtension`, `onEvent`, `encodeParameters`, `responseError` detail).
- `pnpm run typecheck` inserted in `[tasks.web]` between install and build.
- Leaf modules only migrate: `format`, `theme`, `clipboard`, `engines`. DOM
  narrowing (`EventTarget.closest`, `HTMLDetailsElement`, htmx
  `responseError` detail) is declared, not cast.
- Entry `app.js` stays `.js` here; it imports the `.ts` leaves via their `.js`
  specifiers (bundler resolution) and assigns `window.htmx` explicitly — the
  ESM htmx build does not self-assign.

Gate: `mise run validate` green including `pnpm run typecheck`;
`assets/app.js` rebuilt by rolldown; pages serve identically (htmx + json-enc
now come from the bundle).

## Step 2 — ts-rs wire types + i18n catalog (issue #213)

- `ts-rs` derives on the API-facing Rust structs; generated `.ts` committed
  under `web/src/generated/` and typechecked by the same `tsc` pass.
- `strings.rs` → `rust-i18n` catalog so templates and the JS-visible string
  maps (`window.S`, `window.AS`, `window.Q`) share one source of truth;
  `globals.d.ts` maps replaced by generated types.
- Server-sent wire shapes (SSE frame payloads, `/api/*` bodies) get generated
  types consumed by the frontend.

## Step 3 — stream modules + tests (issue #214)

- Remaining modules migrate to `.ts`: `assist.js`, `stream.js`,
  `sse.js`, `watch.js`, `search.js`, plus the `app.js` entry — the files that
  consume the generated wire types and the SSE shim most heavily.
- `web/tests/*.test.js` → `.test.ts`; specifiers and Vitest config adjusted.

## Out of scope

- Type-checking Askama templates (Rust side stays the source of truth for
  SSR output).
- Any build-step change beyond esbuild → rolldown (no dev-server, no HMR;
  the committed bundle remains the shipped artifact).
