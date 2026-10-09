# Third-party notices

This directory holds the assets embedded into the `cauce` binary at
compile time (`rust-embed`).

`spa/` is the Vite build of `crates/cauce-server/web/src/spa/` (Svelte 5 +
Tailwind v4, `pnpm run build:spa`); its third-party packages resolve
through `package.json`/`pnpm-lock.yaml` like every other build dep.

`favicon.svg` is a first-party icon (MPL-2.0, like the rest of the crate),
not vendored.

FX-06 removed the `app.js` bundle and with it the vendored htmx notices
(`htmx.org`, `htmx-ext-json-enc`).
