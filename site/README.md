<!-- MPL-2.0 -->

# cauce site

Public landing page for [cauce](https://github.com/espetro/cauce) — Svelte 5 +
Vite, static build, deployed to Cloudflare Pages.

## Develop

```bash
cd site
pnpm install
pnpm dev        # vite dev server
pnpm check      # svelte-check + tsc
pnpm build      # → dist/
```

## Deploy (Cloudflare Pages)

- Build command: `pnpm install && pnpm build`
- Build directory: `site/dist` (set project root to `site/`, or root dir to
  `site` in Pages settings)
- Node/pnpm: Pages picks up `packageManager` from `package.json`.

`VITE_CAUCE_ORIGIN` (build-time env): when set, the hero search box submits to
`<origin>/search` — point it at the public demo instance once one exists. When
unset, the form submits to `/search` on the same origin, which is a real cauce
search if the binary itself serves this page.
