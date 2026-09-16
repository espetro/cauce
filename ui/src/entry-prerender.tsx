// Build-time-only entry (vite build --ssr src/entry-prerender.tsx). Never
// shipped to browsers: scripts/prerender.mjs imports the built module and
// renders each route to static HTML via preact-iso/prerender.
import prerender, { locationStub } from "preact-iso/prerender";
import { App } from "./app";

/** Render the App for `url` to { html, links }. Routes with dynamic content
 * (search results, history) prerender only the shell; data loads client-side
 * on hydration. */
export async function prerenderApp(url: string) {
  locationStub(url);
  return await prerender(<App url={url} />);
}
