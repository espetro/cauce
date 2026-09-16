// Post-build SSG: render the two static-host entry points via
// preact-iso/prerender (see .agents/docs/references/preact-iso-layout-prerender.md,
// "Recommended Path: Vite + preact-iso/prerender"). Dependency-free: only
// node: modules + the vite-built SSR entry.
//
// Layout produced:
//   dist/index.html   prerendered / (replaces the vite template)
//   dist/404.html     prerendered 404 (oxe serves it with status 404)
//
// Static-host note: oxe serves dist/index.html for every SPA route and
// dist/404.html (status 404) for anything else, so only these two files are
// prerendered; all other routes hydrate from the root shell.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { prerenderApp } from "../dist-ssr/entry-prerender.js";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "..", "dist");
const template = readFileSync(join(dist, "index.html"), "utf8");

const routes = ["/", "/404"];

for (const route of routes) {
  const { html } = await prerenderApp(route);
  // preact-iso/prerender appends the isodata marker for hydrate(); carry the
  // prerendered URL so the client can detect a mismatched shell.
  const marked = html.replace(
    '<script type="isodata"></script>',
    `<script type="isodata" data-url="${route === "/404" ? "/__404__" : route}"></script>`,
  );
  const out = route === "/" ? join(dist, "index.html") : join(dist, "404.html");
  writeFileSync(out, template.replace(/<div id="app"><\/div>/, `<div id="app">${marked}</div>`));
  console.log(`prerendered ${route} -> ${out.slice(dist.length)}`);
}
