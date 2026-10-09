// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // The svelte plugin compiles `.svelte.ts` runes modules so the SPA
  // feature stores are unit-testable (FX-04). Plain `.ts` is untouched.
  plugins: [
    svelte({
      configFile: fileURLToPath(new URL("./svelte.config.js", import.meta.url)),
    }),
  ],
  test: {
    environment: "happy-dom",
    include: ["web/tests/**/*.test.ts"],
  },
});
