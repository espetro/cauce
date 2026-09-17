// Typed route store: single source of truth for SPA matching + navigation.
// nanostores/router (725B gz) replaces preact-iso's Router/LocationProvider;
// preact-iso stays only for lazy/hydrate/prerender (Router-independent).
//
// Matching is on pathname; query params always arrive via page.search
// (settings/mode/since/qf/q round-trip there). The 404 page is the undefined
// page: no route matches. /row/{key} is a backend-only path with no SPA route
// (the server 302s it to /search?q=); the router's click interception
// respects defaultPrevented, so links that preventDefault (or stopPropagation
// before it) are never client-side hijacked — see routes/history.tsx.
import { createRouter, getPagePath, openPage, redirectPage } from "@nanostores/router";
import { useStore } from "@nanostores/preact";

const config = {
  home: "/",
  search: "/search",
  history: "/history",
  dashboard: "/dashboard",
} as const;

export const router = createRouter(config);

export type RouteName = keyof typeof config;
export type RoutePage = NonNullable<ReturnType<typeof router.get>>;
export type RouteSearchParams = Record<string, string>;

export function useRoute(): RoutePage | undefined {
  return useStore(router);
}

// Seed the store before the first client render. We cannot rely on the
// router's own onMount seed: it runs inside a useEffect, i.e. AFTER the
// hydration render, so the first render would see an empty store and mount
// the 404 page against the prerendered shell for the real route (direct load
// of /search?q=x would 404-flash). This seed is synchronous at module load,
// before hydrate() in main.tsx, so useRoute() matches the prerendered URL
// from the first render. replace=true is deliberate: parsing the CURRENT
// address means the URL never changes, so history.replaceState rewrites the
// identical entry (no address-bar change, no extra history entry, no
// hydration-mismatch vs the data-url guard in main.tsx). replace=false would
// push a duplicate of the same entry for every full page load.
if (typeof window !== "undefined" && typeof location !== "undefined") {
  router.open(location.pathname + location.search, true);
}

/** Typed programmatic navigation (push). */
const OMITTED = new Set(["p"]);

function clean(search?: RouteSearchParams): RouteSearchParams {
  if (!search) return {};
  const out: RouteSearchParams = {};
  for (const [k, v] of Object.entries(search)) if (!OMITTED.has(k)) out[k] = v;
  return out;
}

export function navigate(name: RouteName, search?: RouteSearchParams) {
  openPage(router, name, {} as never, (clean(search) ?? {}) as never);
}

/** Typed programmatic navigation (replace history entry). */
export function redirect(name: RouteName, search?: RouteSearchParams) {
  redirectPage(router, name, {} as never, (clean(search) ?? {}) as never);
}

/** Build a route URL from its typed name + search params. */
export function routeUrl(name: RouteName, search?: RouteSearchParams): string {
  return getPagePath(router, name, {} as never, (clean(search) ?? {}) as never);
}

/** Open a raw URL through the router (string URLs, e.g. searchUrl() output).
 * `replace` swaps the current history entry instead of pushing. */
export function openPath(path: string, replace = false) {
  router.open(path, replace);
}
