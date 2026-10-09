/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * History-mode router matching the `GET /app/{*rest}` fallback FX-02
 * mounted: every `/app/*` URL loads this shell, `route` resolves the
 * tail (`/app/search` -> `/search`) into a `routes/` shell, and in-app
 * `<a href="/app/...">` clicks navigate without a reload (the delegated
 * listener lives in `App.svelte`). Links outside `/app` stay normal
 * navigations — they hand off to the HTMX pages.
 */

export interface Route {
  /** Path under the `/app` prefix: `/`, `/search`, ... */
  path: string;
  params: URLSearchParams;
}

function parseLocation(): Route {
  let p = location.pathname;
  p = p.startsWith("/app") ? p.slice(4) || "/" : "/";
  if (p.length > 1 && p.endsWith("/")) p = p.slice(0, -1);
  return { path: p, params: new URLSearchParams(location.search) };
}

export const route: Route = $state(parseLocation());

/** `history.pushState` + swap `route`. `to` is a path or full `/app/...` URL. */
export function navigate(to: string): void {
  const url = new URL(to, location.origin);
  history.pushState({}, "", url.pathname + url.search + url.hash);
  const next = parseLocation();
  route.path = next.path;
  route.params = next.params;
  scrollTo(0, 0);
}

/** `popstate` handler — re-reads `location` into `route`. */
export function onPopState(): void {
  const next = parseLocation();
  route.path = next.path;
  route.params = next.params;
}

/** `/search` -> `/app/search`; `/` -> `/app/`. */
export function appHref(path: string): string {
  return path === "/" ? "/app/" : "/app" + path;
}
