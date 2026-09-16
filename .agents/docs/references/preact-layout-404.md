`preact-iso` does not provide an automatic filesystem routing convention out of the box like Next.js. However, you can achieve a Next.js-style `_layout.tsx` pattern in a Vite + Preact setup using Vite's `import.meta.glob` to parse the file tree and wrap matching routes inside layout components.

---

**Step 1: Set up the Directory Structure**

Organize your `src/pages` or `src/routes` directory using `_layout.tsx` files for layout wrappers and standard route files (`index.tsx`, `about.tsx`, etc.):

```text
src/
└── pages/
    ├── _layout.tsx        # Root layout (Header, Footer, etc.)
    ├── index.tsx          # Maps to /
    ├── about.tsx          # Maps to /about
    └── dashboard/
        ├── _layout.tsx    # Nested layout for dashboard
        └── index.tsx      # Maps to /dashboard

```

---

**Step 2: Create Layout Components**

Layouts accept `children` to render nested views:

```tsx
// src/pages/_layout.tsx
import { ComponentChildren } from 'preact';

export default function RootLayout({ children }: { children: ComponentChildren }) {
  return (
    <div class="app-container">
      <nav><a href="/">Home</a> | <a href="/about">About</a> | <a href="/dashboard">Dashboard</a></nav>
      <main>{children}</main>
    </div>
  );
}

```

---

**Step 3: Collect Routes and Nest Layouts Dynamically**

Use `import.meta.glob` to register both page components and layouts. Match each page path to the layouts in its directory chain:

```tsx
// src/routes.tsx
import { lazy, ComponentType } from 'preact-iso';

// 1. Eagerly load layouts so structure is immediately available
const layoutModules = import.meta.glob<{ default: ComponentType<any> }>(
  './pages/**/_layout.tsx',
  { eager: true }
);

// 2. Lazily load page files
const pageModules = import.meta.glob<{ default: ComponentType<any> }>(
  './pages/**/!(_*).tsx'
);

function convertFileToRoute(path: string): string {
  let route = path
    .replace(/^\.\/pages/, '')
    .replace(/\.tsx$/, '')
    .replace(/\/index$/, '');

  // Handle root index
  if (route === '') route = '/';
  // Normalize dynamic segments: [id].tsx -> :id
  return route.replace(/\[([^\]]+)\]/g, ':$1');
}

export function generateRoutes() {
  return Object.keys(pageModules).map((filePath) => {
    const routePath = convertFileToRoute(filePath);
    const PageComponent = lazy(pageModules[filePath]);

    // Find all matching _layout.tsx files along the directory tree
    // Example: for ./pages/dashboard/settings.tsx -> check ./pages/_layout.tsx and ./pages/dashboard/_layout.tsx
    const segments = filePath.split('/').slice(0, -1);
    const applicableLayouts: ComponentType<any>[] = [];

    let currentPath = '';
    for (const segment of segments) {
      currentPath = currentPath ? `${currentPath}/${segment}` : segment;
      const layoutKey = `${currentPath}/_layout.tsx`;
      if (layoutModules[layoutKey]?.default) {
        applicableLayouts.push(layoutModules[layoutKey].default);
      }
    }

    // Wrap the page component inside applicable layouts (inside-out)
    const WrappedRoute = (props: any) => {
      let content = <PageComponent {...props} />;
      for (let i = applicableLayouts.length - 1; i >= 0; i--) {
        const Layout = applicableLayouts[i];
        content = <Layout {...props}>{content}</Layout>;
      }
      return content;
    };

    return {
      path: routePath,
      Component: WrappedRoute,
    };
  });
}

```

---

**Step 4: Mount with `LocationProvider` and `Router**`

Render the dynamically generated routes inside the `Router` component provided by `preact-iso`:

```tsx
// src/app.tsx
import { LocationProvider, Router, Route } from 'preact-iso';
import { generateRoutes } from './routes';

const routes = generateRoutes();

export function App() {
  return (
    <LocationProvider>
      <Router>
        {routes.map(({ path, Component }) => (
          <Route key={path} path={path} component={Component} />
        ))}
      </Router>
    </LocationProvider>
  );
}

```

---

**Key Details to Keep in Mind**

* **Layout Remounting:** Because `preact-iso`'s `<Router>` switches the entire top-level component on route transitions, wrapping each route individually will unmount and remount the layout on navigation between sibling pages unless state preservation is handled at the parent or external state layer.
* **Wildcards & 404s:** To add a fallback 404 page, add a `404.tsx` file in `pages/` and route it with `<Route component="{NotFoundComponent}" default/>` outside the auto-generated loop.

---

# 4xx / 5xx pages

In a Preact application using `preact-iso`, handling 4xx and 5xx errors requires two different mechanisms: **Routing fallback** for client-side 404s/not found routes, and **Error Boundaries** for unhandled 5xx runtime crashes.

---

### 1. Handling 404 (Not Found) Routes

`preact-iso` natively supports a `default` prop on `<Route/>`. Any URL that fails to match your registered route paths will fall through to this component.

Create a fallback component:

```tsx
// src/pages/404.tsx
export function NotFound() {
  return (
    <section style={{ textAlign: 'center', padding: '48px 16px' }}>
      <h1>404 - Page Not Found</h1>
      <p>The page you are looking for does not exist or has moved.</p>
      <a href="/">Return Home</a>
    </section>
  );
}

```

Register it as the last child in your `<Router>`:

```tsx
// src/app.tsx
import { LocationProvider, Router, Route } from 'preact-iso';
import { NotFound } from './pages/404';
import { generateRoutes } from './routes'; // from your dynamic file-based setup

const routes = generateRoutes();

export function App() {
  return (
    <LocationProvider>
      <Router>
        {routes.map(({ path, Component }) => (
          <Route key={path} path={path} component={Component} />
        ))}

        {/* Catch-all 404 route */}
        <Route default component={NotFound} />
      </Router>
    </LocationProvider>
  );
}

```

---

### 2. Handling 500 (Runtime / Application Errors)

Client-side 5xx errors occur when a component throws an uncaught JavaScript error during rendering or lifecycle updates. To catch these without unmounting the entire app, use Preact's `componentDidCatch` lifecycle hook inside an **Error Boundary**.

Create an Error Boundary component:

```tsx
// src/components/ErrorBoundary.tsx
import { Component, ComponentChildren } from 'preact';

interface Props {
  children: ComponentChildren;
  fallback?: (error: Error, reset: () => void) => ComponentChildren;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, errorInfo: any) {
    // Forward error to telemetry/logging (e.g. Sentry)
    console.error('Unhandled UI exception:', error, errorInfo);
  }

  reset = () => {
    this.setState({ error: null });
  };

  render() {
    if (this.state.error) {
      if (this.fallback) {
        return this.props.fallback(this.state.error, this.reset);
      }

      return (
        <section style={{ textAlign: 'center', padding: '48px 16px' }}>
          <h1>500 - Something Went Wrong</h1>
          <p>An unexpected application error occurred.</p>
          <pre style={{ color: '#d32f2f', textAlign: 'left', display: 'inline-block' }}>
            {this.state.error.message}
          </pre>
          <div style={{ marginTop: '16px' }}>
            <button onClick={this.reset}>Try Again</button>
            {' | '}
            <a href="/">Go to Home</a>
          </div>
        </section>
      );
    }

    return this.props.children;
  }
}

```

Wrap the `<Router>` (or individual layout levels) in the `ErrorBoundary`:

```tsx
// src/app.tsx
import { LocationProvider, Router, Route } from 'preact-iso';
import { ErrorBoundary } from './components/ErrorBoundary';
import { NotFound } from './pages/404';

export function App() {
  return (
    <LocationProvider>
      <ErrorBoundary>
        <Router>
          {/* dynamic routes */}
          <Route default component={NotFound} />
        </Router>
      </ErrorBoundary>
    </LocationProvider>
  );
}

```

---

### 3. Handling API-Triggered 4xx/5xx (e.g., Resource Not Found, API Down)

When an API fetch returns a `403`, `404`, or `500` status for dynamic data (e.g., `/user/:id`), you can either throw the response error to trigger the boundary, or conditionally render status-specific views:

```tsx
import { useState, useEffect } from 'preact/hooks';
import { NotFound } from './404';

export function UserProfile({ id }: { id: string }) {
  const [status, setStatus] = useState<number | null>(null);
  const [data, setData] = useState<any>(null);

  useEffect(() => {
    fetch(`/api/users/${id}`)
      .then((res) => {
        if (!res.ok) {
          setStatus(res.status);
          return null;
        }
        return res.json();
      })
      .then((payload) => payload && setData(payload))
      .catch(() => setStatus(500));
  }, [id]);

  if (status === 404) return <NotFound />;
  if (status === 403) return <div>403 - You do not have permission to view this profile.</div>;
  if (status && status >= 500) return <div>500 - User service is currently unavailable.</div>;
  if (!data) return <div>Loading...</div>;

  return <div>Welcome, {data.name}</div>;
}

```

