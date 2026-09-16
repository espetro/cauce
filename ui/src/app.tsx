import { LocationProvider, Route, Router, lazy, ErrorBoundary } from "preact-iso";
// Root layout is statically imported (the eager layouts glob must exclude it:
// './routes/**' matches the root file too and vite emits an
// INEFFECTIVE_DYNAMIC_IMPORT warning for the eager entry). Segment layouts,
// if added later, are glob-resolved below.
import RootLayout from "./routes/_layout";

// File-based routing: ui/src/routes/* maps to URLs (index.tsx -> "/", 404.tsx
// -> default fallback). [param] files become :param routes; every page is
// wrapped by routes/_layout.tsx; _-prefixed files never become routes. This module is environment-agnostic:
// the browser mounts it from main.tsx, the build prerenders it from
// entry-prerender.tsx. No CSS or browser globals here.
// Non-layout routes only; '*.tsx' would also match _layout.tsx, which is
// statically imported above and would trip vite's INEFFECTIVE_DYNAMIC_IMPORT.
const pages = import.meta.glob<{ default: any }>([
  "./routes/*.tsx",
  "./routes/*/*.tsx",
  "./routes/*/*/*.tsx",
  "!./routes/_layout.tsx",
]);
const isPage = (file: string) => !file.split("/").pop()!.startsWith("_");

function routePath(file: string): string {
  return (
    file
      .replace("./routes", "")
      .replace(/\.tsx$/, "")
      .replace(/\/index$/, "")
      // NOTE: there is no /row SPA route (backend-only path). preact-iso
      // intercepts ALL same-origin anchors, so backend-only paths like /row
      // must never be used as plain SPA links — see routes/history.tsx.
      // './routes/row/[key].tsx' -> '/row/:key'
      .replace(/\[(\w+)\]/g, ":$1") || "/"
  );
}

/** Layout resolution: the statically imported root _layout.tsx applies to
 * every page. If segment layouts (routes/<seg>/_layout.tsx) are ever added,
 * restore an eager glob here (excluding the root file) with
 * longest-prefix-match resolution (glob "routes/<seg>/_layout.tsx").
 */
function layoutFor(_pagePath: string) {
  return RootLayout;
}

const pageRoutes = Object.entries(pages)
  .filter(([file]) => isPage(file))
  .map(([file, load]) => {
    const path = routePath(file);
    const Layout = layoutFor(file);
    const Component = lazy(async () => {
      const Page = (await load()).default;
      const Wrapped = (props: object) => (
        <Layout>
          <Page {...props} />
        </Layout>
      );
      return { default: Wrapped };
    });
    // preact-iso's lazy is suspense-based: an ErrorBoundary (its childDidSuspend
    // impl) above the lazy route lets renderToStringAsync await the import at
    // prerender time; it also catches lazy-load failures client-side.
    // 404.tsx falls out of the glob as the Router's default (fallback) route,
    // wrapped by the layout machinery like any other page.
    return { path, Component, isDefault: path === "/404" };
  });

export function App({ url }: { url?: string }) {
  const regular = pageRoutes.filter((r) => !r.isDefault);
  const fallback = pageRoutes.filter((r) => r.isDefault);
  const wrap = (Component: unknown) => {
    const C = Component as any;
    return (props: object) => (
      <ErrorBoundary>
        <C {...props} />
      </ErrorBoundary>
    );
  };
  return (
    <LocationProvider {...(url ? { url } : {})}>
      <Router>
        {regular.map(({ path, Component }) => (
          <Route key={path} path={path} component={wrap(Component)} />
        ))}
        {fallback.map(({ Component }) => (
          <Route key="404" default component={wrap(Component)} />
        ))}
      </Router>
    </LocationProvider>
  );
}
