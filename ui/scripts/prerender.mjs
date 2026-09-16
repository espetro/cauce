// Post-build SSG: render each known route to dist/<route>/index.html via
// preact-iso/prerender (see .agents/docs/references/preact-iso-layout-prerender.md,
// "Recommended Path: Vite + preact-iso/prerender"). Dependency-free: only
// node: modules + the vite-built SSR entry.
//
// Layout produced (mirror of the src/routes glob):
//   dist/index.html           prerendered / (replaces the vite template)
//   dist/search/index.html    shell only (results load client-side; ?q= deep
//                             links hydrate normally — prerender assumes no q)
//   dist/history/index.html   shell
//   dist/dashboard/index.html shell
//   dist/404.html             prerendered 404
//   dist/200.html             fallback copy of the SPA template for hosts
//                             wanting to run their own 404 fallback
//
// Static-host note: oxe's backend serves dist/index.html for EVERY SPA route
// (see oxe/server/system.py / search.py / cache_admin.py), so only the
// prerendered / is actually served by oxe itself; the per-route files exist
// for static hosting and stay correct (hydration self-checks the URL).
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { prerenderApp } from "../dist-ssr/entry-prerender.js";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "..", "dist");
const template = readFileSync(join(dist, "index.html"), "utf8");

const routes = ["/", "/search", "/history", "/dashboard", "/404"];

for (const route of routes) {
  const { html } = await prerenderApp(route);
  // preact-iso/prerender appends the isodata marker for hydrate(); carry the
  // prerendered URL so the client can detect a mismatched shell.
  const marked = html.replace(
    '<script type="isodata"></script>',
    `<script type="isodata" data-url="${route === "/404" ? "/__404__" : route}"></script>`,
  );
  const out =
    route === "/"
      ? join(dist, "index.html")
      : route === "/404"
        ? join(dist, "404.html")
        : join(dist, route, "index.html");
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, template.replace(/<div id="app"><\/div>/, `<div id="app">${marked}</div>`));
  console.log(`prerendered ${route} -> ${out.slice(dist.length)}`);
}

// SPA fallback copy of the raw (unprerendered) template for hosts that
// want an explicit 200/404 fallback document.
writeFileSync(join(dist, "200.html"), template);
