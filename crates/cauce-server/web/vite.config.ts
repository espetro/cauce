// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// SPA build (FX-02): Svelte 5 + Tailwind v4 -> `assets/spa/`, embedded by
// rust-embed and served at `GET /app` (ui feature). This is additive: the
// rolldown pipeline for `assets/app.js` (`web/build.mjs`) is untouched.
//
// `root` is `web/src/spa/` so the emitted `index.html` is the app shell;
// `base` matches the mount point so hashed assets resolve under `/app/`.

import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { sveltePhosphorOptimize } from "phosphor-svelte/vite";
import { defineConfig } from "vite";

export default defineConfig({
  root: fileURLToPath(new URL("./src/spa", import.meta.url)),
  base: "/app/",
  publicDir: false,
  plugins: [
    svelte({
      configFile: fileURLToPath(new URL("../svelte.config.js", import.meta.url)),
    }),
    // Rewrites `phosphor-svelte` barrel imports to per-icon deep imports —
    // without it the build compiles all ~1.6k icon components.
    sveltePhosphorOptimize(),
    tailwindcss(),
  ],
  build: {
    // Relative to `root` (web/src/spa) — outside root, so emptyOutDir must
    // be explicit; only `assets/spa/` is emptied.
    outDir: "../../../assets/spa",
    emptyOutDir: true,
    // The SPA is committed (rust-embed reads it at cargo build time);
    // reproducible output keeps the `mise run web` freshness diff stable.
    manifest: false,
    sourcemap: false,
  },
});
