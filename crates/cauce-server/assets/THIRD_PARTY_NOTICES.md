# Third-party notices

This directory holds the assets embedded into the `cauce` binary at
compile time (`rust-embed`). `app.js` is a bundle built from
`crates/cauce-server/web/` (rolldown, `pnpm run build`); it includes two
third-party packages resolved through `package.json`/`pnpm-lock.yaml`:

- **htmx** (`htmx.org`), pinned to **2.0.4** — the version previously
  vendored here as `htmx.min.js`. License: BSD-2-Clause.
  Source: https://github.com/bigskysoftware/htmx
- **htmx `json-enc` extension** (`htmx-ext-json-enc`), pinned to
  **2.0.2** — the version previously vendored here as `json-enc.js`.
  License: 0BSD (BSD Zero Clause).
  Source: https://github.com/bigskysoftware/htmx-extensions
  (`src/json-enc/json-enc.js`)

The pins are deliberate: both must move together, and json-enc ≥2.0.0 is
the htmx-2-compatible line (the htmx-1 build warned on every page load).

`favicon.svg` is a first-party icon (MPL-2.0, like the rest of the crate),
not vendored.
