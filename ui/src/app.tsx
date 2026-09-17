import { ErrorBoundary, lazy } from "preact-iso";
import type { ComponentType } from "preact";
// Root layout wraps every route (statically imported so the root file never
// appears in the lazy page map below).
import RootLayout from "./routes/_layout";
import { useRoute, type RouteName } from "./lib/routes";

// Hand-written route table: ui/src/lib/routes.ts owns matching + typed
// navigation; this map owns the lazy page components. This module is
// environment-agnostic: the browser mounts it from main.tsx, the build
// prerenders it from entry-prerender.tsx. No CSS or browser globals here.
const withLayout = (load: () => Promise<{ default: ComponentType }>): ComponentType =>
  lazy(async () => {
    const { default: Page } = await load();
    return {
      default: (props: object) => (
        <RootLayout>
          <Page {...props} />
        </RootLayout>
      ),
    };
  });

const Pages: Record<RouteName | "notFound", ComponentType> = {
  home: withLayout(() => import("./routes/index")),
  search: withLayout(() => import("./routes/search")),
  history: withLayout(() => import("./routes/history")),
  dashboard: withLayout(() => import("./routes/dashboard")),
  notFound: withLayout(() => import("./routes/404")),
};

// No matching route = 404 (the undefined page in lib/routes.ts).
function Routed() {
  const page = useRoute();
  const Page = page ? Pages[page.route] : Pages.notFound;
  return <Page />;
}

export function App() {
  // preact-iso's lazy is suspense-based: an ErrorBoundary (its childDidSuspend
  // impl) above the lazy routes lets renderToStringAsync await the imports at
  // prerender time; it also catches lazy-load failures client-side.
  return (
    <ErrorBoundary>
      <Routed />
    </ErrorBoundary>
  );
}
