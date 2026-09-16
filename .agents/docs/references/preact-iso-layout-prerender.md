`preact-iso` is a minimal, lightweight router and does not include built-in filesystem convention scanning or nested layout wrappers like Next.js or Remix.

You can achieve standard file-based routing and nested `_layout.tsx` handling in Preact using one of two approaches:

---

### Option 1: Vite Plugin (Recommended for `preact-iso`)

If you are using Vite, you can keep `preact-iso` and use **`vite-plugin-pages`** or **`unplugin-vue-router`** (configured for JSX/Preact) to automatically map your `pages/` directory into a route tree that supports layout files.

#### 1. Install `vite-plugin-pages`

```bash
npm i -D vite-plugin-pages

```

#### 2. Configure `vite.config.ts`

```typescript
import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';
import Pages from 'vite-plugin-pages';

export default defineConfig({
  plugins: [
    preact(),
    Pages({
      dirs: 'src/pages',
      extensions: ['tsx', 'jsx', 'ts', 'js'],
      // Exclude layout files from direct URL route matching
      exclude: ['**/_layout.tsx'],
    }),
  ],
});

```

#### 3. Set up Nested Layouts with `preact-iso`

Create an entry in your app that wraps the resolved route component with its matching layout:

```tsx
// src/index.tsx
import { LocationProvider, Router, Route } from 'preact-iso';
import routes from '~pages';

// Eagerly or lazily load layouts using Vite's glob import
const layouts = import.meta.glob('./pages/**/_layout.tsx', { eager: true });

function resolveLayout(path: string) {
  // Finds the nearest _layout.tsx based on the route path
  const match = Object.keys(layouts)
    .sort((a, b) => b.length - a.length)
    .find((layoutPath) => path.startsWith(layoutPath.replace('/_layout.tsx', '')));
  return match ? (layouts[match] as any).default : ({ children }: any) => <>{children}</>;
}

export function App() {
  return (
    <LocationProvider>
      <Router>
        {routes.map(({ path, component: Component }) => {
          const Layout = resolveLayout(path);
          return (
            <Route
              path={path}
              component={() => (
                <Layout>
                  <Component />
                </Layout>
              )}
            />
          );
        })}
      </Router>
    </LocationProvider>
  );
}

```

---

### Option 2: Native Glob Imports (Zero Extra Dependencies)

If you don't want external plugins, use Vite's `import.meta.glob` directly to parse your `src/pages` folder:

```tsx
// src/router.tsx
import { LocationProvider, Router, Route } from 'preact-iso';

const pages = import.meta.glob('./pages/**/[!_]*.tsx');
const layouts = import.meta.glob('./pages/**/_layout.tsx', { eager: true });

const routeTree = Object.entries(pages).map(([pagePath, importer]) => {
  // Convert './pages/blog/[id].tsx' -> '/blog/:id'
  let url = pagePath
    .replace(/^\.\/pages/, '')
    .replace(/\.(tsx|jsx)$/, '')
    .replace(/\/index$/, '') || '/';
  
  url = url.replace(/\[(\w+)\]/g, ':$1');

  // Match closest directory layout
  const matchingLayoutKey = Object.keys(layouts)
    .filter((layoutPath) => pagePath.startsWith(layoutPath.replace('_layout.tsx', '')))
    .sort((a, b) => b.length - a.length)[0];

  const Layout = matchingLayoutKey
    ? (layouts[matchingLayoutKey] as any).default
    : ({ children }: any) => <>{children}</>;

  return {
    path: url,
    Component: (props: any) => {
      const LazyPage = preact-iso.lazy(importer as any);
      return (
        <Layout>
          <LazyPage {...props} />
        </Layout>
      );
    },
  };
});

```

---

### Alternative: Use Fresh

If your project is new and full file-based routing with layouts, zero-bundle overhead, and SSR/Islands is the primary goal, consider **Fresh** (Preact's official full-stack framework).

* Built natively around `routes/_layout.tsx` and nested directory routing.
* Out-of-the-box support for partial hydration and island components.

---

For an **SSG + CSR** architecture (pre-rendering static HTML at build time, then hydrating into a lightweight client-side SPA), your best path depends on your deployment target and DX preferences.

| Path | Best For | Pros | Trade-offs |
| --- | --- | --- | --- |
| **Vite + `preact-iso/prerender` + Glob Routing** | Standard static hosts (GitHub Pages, S3, Netlify, Cloudflare Pages) | Stays 100% in the standard Vite ecosystem; tiny bundle; zero server runtime required | Custom glob-parsing logic for routes and layouts |
| **Astro (with Preact integration)** | Content-heavy SSG with interactive CSR islands | Industry standard for SSG; native file routing; hydrates only when needed | Not a pure SPA; page transitions require Astro View Transitions |
| **Fresh (Deno)** | Edge-first deployment (Deno Deploy) | Native file routing & `_layout.tsx`; zero build-step setup | Coupled to Deno runtime; traditional full static export is less idiomatic than its edge-SSR |

---

### The Recommended Path: Vite + `preact-iso/prerender`

If you are already committed to **Vite + Preact**, you don't need a heavy framework. You can achieve SSG + CSR and file-based layout routing using native Vite tooling:

1. **Routing & Layouts:** Use **Option 2 (Native Glob Imports)** from before. It turns `./pages/**/[!_]*.tsx` and `./pages/**/_layout.tsx` into a dynamic client-side SPA router without extra plugins.
2. **Build-Time Prerendering (SSG):** Use `preact-iso/prerender` inside a post-build script or a minimal Vite plugin to render each route to an `index.html` file.
3. **Hydration (CSR):** When the browser loads the static HTML, `preact-iso` hydrates and takes over client-side navigation without full page reloads.

#### Basic SSG Setup with `preact-iso`:

**1. Export a prerender function in your entry:**

```tsx
// src/index.tsx
import { LocationProvider, Router } from 'preact-iso';
import prerender from 'preact-iso/prerender';
import { routes } from './router';

export function App({ url = '/' }: { url?: string }) {
  return (
    <LocationProvider url={url}>
      <Router>
        {routes.map(({ path, Component }) => (
          <Component key={path} path={path} />
        ))}
      </Router>
    </LocationProvider>
  );
}

// Used at build time for SSG
export async function prerenderSite(data: { url: string }) {
  return await prerender(<App url={data.url} />);
}

```

**2. SSG Build Script (`prerender.js`):**

```javascript
import fs from 'node:fs';
import path from 'node:path';
import { prerenderSite } from './dist/server/index.js';
import { routes } from './src/router-manifest.json'; // or derived from glob

const template = fs.readFileSync('dist/client/index.html', 'utf-8');

for (const route of routes) {
  const { html, head } = await prerenderSite({ url: route });
  const finalHtml = template
    .replace('<!--app-head-->', head.elements.join(''))
    .replace('<!--app-html-->', html);

  const outDir = path.join('dist/client', route);
  fs.mkdirSync(outDir, { recursive: true });
  fs.writeFileSync(path.join(outDir, 'index.html'), finalHtml);
}

```

---

### Key Decision Point

* **Go with Vite + Glob Routing** if you want a **single, unified SPA bundle** that simply starts as pre-baked static HTML.
* **Switch to Astro** if your primary goal is **content performance** (blogs, docs, landing pages) with occasional interactive Preact widgets, as it avoids client-side router overhead altogether.
