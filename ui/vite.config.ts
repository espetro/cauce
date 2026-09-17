import { defineConfig, type UserConfig } from "vite";
import preact from "@preact/preset-vite";
import tailwindcss from "@tailwindcss/vite";
import Icons from "unplugin-icons/vite";
import { paraglideVitePlugin } from "@inlang/paraglide-js";
import pkg from "./package.json" with { type: "json" };

// Replace at build time: app version shown in the header.
const defines = { __APP_VERSION__: JSON.stringify(pkg.version) };
const iconResolver = Icons({ compiler: "jsx", jsx: "preact" });

// Document navigations must fall through to the SPA (index.html);
// only fetch/XHR traffic is proxied to the oxe backend.
const isDocument = (req: { headers: Record<string, string | string[] | undefined> }) =>
  (req.headers["sec-fetch-dest"] ?? "") === "document";

const paraglide = paraglideVitePlugin({
  project: "./project.inlang",
  outdir: "./src/paraglide",
  // EN-only app: static locale + baseLocale strategy let the bundler
  // tree-shake all locale-detection machinery from the paraglide runtime
  // (keeps us inside the 40KB gz JS budget).
  strategy: ["baseLocale"],
  experimentalStaticLocale: '"en"',
  isServer: "typeof window === 'undefined'",
  disableAsyncLocalStorage: true,
});

const spaConfig: UserConfig = {
  define: defines,
  plugins: [
    preact(),
    tailwindcss(),
    iconResolver,
    paraglide,
    // EN-only: swap the paraglide runtime for a minimal shim (see
    // src/lib/paraglide-runtime-shim.js). The prerender build keeps the
    // real runtime so server-side flows stay intact.
    {
      name: "oxe-paraglide-runtime-shim",
      enforce: "pre",
      resolveId(source, importer) {
        if (
          importer &&
          importer.includes("src/paraglide") &&
          (source === "../runtime.js" || source === "./runtime.js")
        ) {
          return new URL("./src/lib/paraglide-runtime-shim.js", import.meta.url).pathname;
        }
        return null;
      },
    },
  ],
  server: {
    proxy: {
      "/search": {
        target: "http://127.0.0.1:4480",
        // POST /search from the app is JSON; a GET document hit should
        // render the SPA instead of the backend's HTML page.
        bypass: (req) => {
          if (isDocument(req)) return "/index.html";
        },
      },
      "/suggest": "http://127.0.0.1:4480",
      "/ac": "http://127.0.0.1:4480",
      "/history": {
        target: "http://127.0.0.1:4480",
        // direct SPA navigation to /history serves index.html instead
        bypass: (req) => {
          if (isDocument(req)) return "/index.html";
        },
      },
      "/click": "http://127.0.0.1:4480",
      "/v1/models": "http://127.0.0.1:4480",
      "/answer": "http://127.0.0.1:4480",
      "/settings": "http://127.0.0.1:4480",
      "/cache/stats": "http://127.0.0.1:4480",
      "/row": "http://127.0.0.1:4480",
      "/mcp": "http://127.0.0.1:4480",
    },
  },
};

// bundle virtua into the SSR prerender bundle (it imports 'react', which only
// resolves via the compat alias inside the bundle, not from node)
const environmentsSsrNoExternal: object = {
  environments: { ssr: { build: { noExternal: [/virtua/] } } },
};

// SSG step (post-build): an SSR-only bundle of the prerender entry, consumed
// by scripts/prerender.mjs. Emitted into dist-ssr (outside dist) so it stays
// out of the served bundle and the size budget.
const prerenderConfig: UserConfig = {
  define: defines,
  plugins: [preact(), iconResolver, paraglide],
  resolve: {
    // same alias the client build gets from @preact/preset-vite, needed for
    // react-facing deps (e.g. virtua) resolved at SSR runtime
    alias: { react: "preact/compat", "react/": "preact/compat/" },
  },
  build: {
    ssr: "src/entry-prerender.tsx",
    outDir: "dist-ssr",
    emptyOutDir: true,
    ...(environmentsSsrNoExternal as object),
    rollupOptions: {
      // bundle react-facing deps (virtua) so the compat alias applies, and
      // externalize the preact ecosystem so prerender()'s
      // preact-render-to-string shares the same preact instance as the app
      // (options hook wiring for suspense/async rendering).
      external: [/^preact/, /^@preact/],
      output: { entryFileNames: "entry-prerender.js", inlineDynamicImports: true },
    },
  },
};

export default defineConfig(({ mode }) => (mode === "prerender" ? prerenderConfig : spaConfig));
