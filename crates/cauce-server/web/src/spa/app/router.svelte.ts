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

/* ------------------------------------------------------------------ */
/* FX-07 route requirements (§7.4): every route declares the surface it */
/* needs; `routeVisible` is the one central filter — nav hides and the  */
/* outlet renders a gate notice instead of the page. No inline role    */
/* checks in components.                                               */
/* ------------------------------------------------------------------ */

/** What a route needs from `capabilities.flags`. */
export type RouteRequirement = "admin" | "archiving";

/**
 * `/app` path → required surface. `/admin` and `/settings` are operator
 * surfaces (`adminSurface`); `/archive` exists only while the `archiving`
 * flag is up. Everything else (`/`, `/search`, `/answer`, `/history`,
 * `/dashboard`) is open — public mode swaps their data plane, not their
 * reachability.
 */
export const ROUTE_REQUIRES: Record<string, RouteRequirement> = {
  "/admin": "admin",
  "/settings": "admin",
  "/archive": "archiving",
};

/**
 * The central filter: `false` means the nav link hides and the routed
 * page shows the gate notice. Called by `TopNav` (links) and `App`
 * (outlet) — one function so both can never disagree.
 */
export function routeVisible(path: string, adminSurface: boolean, archiving: boolean): boolean {
  const need = ROUTE_REQUIRES[path];
  if (need === "admin") return adminSurface;
  if (need === "archiving") return archiving;
  return true;
}

