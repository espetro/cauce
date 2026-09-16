// Build-time-only entry (vite build --ssr src/entry-prerender.tsx). Never
// shipped to browsers: scripts/prerender.mjs imports the built module and
// renders each route to static HTML via preact-iso/prerender.
import prerender, { locationStub } from "preact-iso/prerender";
import { App } from "./app";
import { openPath } from "./lib/routes";

/** Render the App for `url` to { html, links }. Routes with dynamic content
 * (search results, history) prerender only the shell; data loads client-side
 * on hydration. The router store has no window in SSR, so seed it with the
 * target URL before rendering. */
export async function prerenderApp(url: string) {
  locationStub(url);
  openPath(url);
  return await prerender(<App />);
}
