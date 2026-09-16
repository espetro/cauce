import { render } from "preact";
import { LocationProvider, Route, Router, lazy } from "preact-iso";
import { initTheme } from "./lib/theme";
import "./index.css";

// File-based routing: ui/src/routes/* maps to URLs (index.tsx -> "/", 404.tsx
// -> default fallback). Each route is wrapped by routes/_layout.tsx (found
// via the layout glob below); _-prefixed files never become routes.
const pages = import.meta.glob<{ default: any }>("./routes/!(_)*.tsx");
const layouts = import.meta.glob<{ default: any }>("./routes/**/_layout.tsx", {
  eager: true,
});

const RootLayout = layouts["./routes/_layout.tsx"].default;

function routePath(file: string): string {
  return (
    file
      .replace("./routes", "")
      .replace(/\.tsx$/, "")
      .replace(/\/index$/, "") || "/"
  );
}

const pageRoutes = Object.entries(pages).map(([file, load]) => ({
  path: routePath(file),
  Component: lazy(async () => {
    const Page = (await load()).default;
    const Wrapped = (props: object) => (
      <RootLayout>
        <Page {...props} />
      </RootLayout>
    );
    return { default: Wrapped };
  }),
}));

const NotFound = lazy(async () => {
  const Page = (await import("./routes/404")).default;
  const Wrapped = (props: object) => (
    <RootLayout>
      <Page {...props} />
    </RootLayout>
  );
  return { default: Wrapped };
});

export function App() {
  return (
    <LocationProvider>
      <Router>
        {pageRoutes.map(({ path, Component }) => (
          <Route key={path} path={path} component={Component} />
        ))}
        <Route default component={NotFound} />
      </Router>
    </LocationProvider>
  );
}

render(<App />, document.getElementById("app")!);
initTheme();
