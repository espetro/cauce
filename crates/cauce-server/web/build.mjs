// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Bundles `web/src/app.js` into `assets/app.js` — a single minified IIFE
// the templates inline (`{{ crate::html::app_js()|safe }}`) like the
// vendored htmx/json-enc payloads. `node web/build.mjs --watch` rebuilds
// on change.

import { readFile } from "node:fs/promises";
import * as esbuild from "esbuild";

const options = {
  entryPoints: ["web/src/app.js"],
  bundle: true,
  minify: true,
  format: "iife",
  target: "es2018",
  outfile: "assets/app.js",
  banner: {
    js: "/* This Source Code Form is subject to the terms of the Mozilla Public\n * License, v. 2.0. If a copy of the MPL was not distributed with this\n * file, You can obtain one at https://mozilla.org/MPL/2.0/. */",
  },
};

if (process.argv.includes("--watch")) {
  const ctx = await esbuild.context(options);
  await ctx.watch();
} else {
  await esbuild.build(options);
  // Inlined into a classic <script> block, so a literal "</script" in the
  // output would terminate the tag early — fail the build instead.
  const out = await readFile("assets/app.js", "utf8");
  if (out.includes("</script")) {
    throw new Error("assets/app.js contains '</script' — unsafe to inline");
  }
}
