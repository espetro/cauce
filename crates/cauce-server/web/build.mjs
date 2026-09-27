// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Bundles `web/src/app.js` into `assets/app.js` — a single minified IIFE
// the templates inline (`{{ crate::html::app_js()|safe }}`). `node
// web/build.mjs --watch` rebuilds on change.

import { readFile } from "node:fs/promises";
import { build, watch } from "rolldown";

const options = {
  input: "web/src/app.js",
  transform: { target: "es2018" },
  output: {
    file: "assets/app.js",
    format: "iife",
    minify: true,
    // postBanner is prepended after minification, so the MPL header
    // survives (a plain `banner` would be fed through the minifier).
    postBanner:
      "/* This Source Code Form is subject to the terms of the Mozilla Public\n * License, v. 2.0. If a copy of the MPL was not distributed with this\n * file, You can obtain one at https://mozilla.org/MPL/2.0/. */\n",
  },
};

if (process.argv.includes("--watch")) {
  watch(options);
} else {
  await build(options);
  // Inlined into a classic <script> block, so a literal "</script" in the
  // output would terminate the tag early — fail the build instead.
  const out = await readFile("assets/app.js", "utf8");
  if (out.includes("</script")) {
    throw new Error("assets/app.js contains '</script' — unsafe to inline");
  }
}
