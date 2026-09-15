# Tech stack decision: oxe web UI frontend

Date: 2026-09-15 (rev 2, supersedes the vanilla-JS verdict). Scope: frontend/UI stack for the redesigned web UI against the phase 1 fixed design requirements (landing with TRADITIONAL|AI toggle + suggestions dropdown, search A/B/B2/C screens, history, static dashboard, chunked AI streaming, URL/content-negotiation contract intact).

Reversal notes from the owner (binding): (1) a component REUSE story is required, vanilla forces re-implementing dropdown, source cards, toggle, citations per screen; (2) mise.toml pins distinct python/bun toolsets and mise tasks run the workflow; (3) no-JS degradation is dropped as a criterion, replaced by small client payload and fast hydration; (4) Bulma and daisyUI are evaluated as no-JS CSS layers, including pairing with Preact.

## Verdict

Adopt **Preact + Vite (CSR, static build)** with **daisyUI 5 on Tailwind CSS 4** as the styling layer, served as static files from the unchanged stdlib Python server. Preact is the honest middle point in this field: at ~4.5KB min+gz for the core runtime (preactjs.com, homepage claim) it is an order of magnitude smaller than a hydrated SvelteKit/SolidStart app shell, it gives us real reusable components (`<SearchBox>`, `<SourceCard>`, `<Citation>`, `<ModeToggle>`) with the largest ecosystem of the small-runtime options, and its Vite integration is the most boring and best-documented path. Solid and Svelte 5 have better reactivity throughput on paper but their advantages are irrelevant at this component count, and Svelte 5's runes/compiler model plus Lit's web-component + shadow-DOM styling story both add conceptual surface a single Python maintainer does not need; Preact's value is precisely that a React-knowledgeable contributor can be productive instantly. daisyUI wins the CSS layer over Bulma because it gives ready-made no-JS class components (dropdown, toggle, badge) plus a theming system built on CSS custom properties that maps directly onto our `color-scheme: light dark` dark mode requirement, ships only the classes actually used (zero-JS by design, CSS output purged by Tailwind, tens of KB raw but only what we reference lands in dist), and the `@plugin`/Vite wiring is first-party documented; Bulma is lighter conceptually but its dark mode is a newer/inferior fit, its class vocabulary is heavier per component, and the Google-anatomy card-less aesthetic (near-black `#0d0817` canvas, hairline borders, one saturated accent) fights Bulma's opinionated component chrome more than it fights a custom daisyUI theme. Hand-rolled custom-property CSS stays in the running as the styling escape hatch for the result anatomy itself, layered on top of daisyUI for structural components. The backend, routes, and Exa JSON content negotiation do not move: Vite builds to `oxe/static/dist/`, the Python process serves it, no runtime node, no server RSS change.

| Option | Server RSS impact | Client payload (min+gz) | Node/build added | Component reuse | Streaming ergonomics | Dark mode / theming | Content negotiation intact | Maintainability (1 person, Python) |
|---|---|---|---|---|---|---|---|---|
| 1. Preact + Vite CSR + daisyUI (chosen) | none (static assets) | ~4-5KB runtime + app + ~10-20KB purged CSS | dev-time only (bun) | excellent, real components | excellent (fetch reader → state → DOM) | excellent (CSS vars, `color-scheme: light dark`) | yes, serve dist/, routes untouched | good: one JS ecosystem, pinned via mise |
| 2. Preact + Vite + Bulma | none | same JS + ~30KB+ CSS (modular import trims it) | dev-time only | good components, weak theming | excellent | fair: dark mode newer, more manual overrides | yes | good, but two styling mental models (Bulma classes + custom overrides) |
| 3. Solid + Vite | none | ~7-8KB runtime + app | dev-time only | excellent | excellent (fine-grained signals) | framework-agnostic, own theming | yes | good, smaller ecosystem/community than Preact |
| 4. Svelte 5 + Vite (no Kit) | none | ~5-10KB compiled runtime + app | dev-time only | excellent | good (stores/runes) | framework-agnostic | yes | runes are a young, churning API; compiler opinionation |
| 5. Lit + Vite | none | ~5-7KB runtime + app | dev-time only | good (web components) | good | shadow DOM styling fights global theme tokens | yes | web-components ceremony (custom elements, shadow DOM) for a 1-page app |
| 6. Status quo: SSR + vanilla JS | none | ~5-10KB | none | poor: every element re-implemented per screen | excellent | manual | yes | excellent ecosystem-wise, worst code-duplication-wise (rejected reason) |

## Component inventory

The reuse argument is the whole point of this revision. Each spec'd UI element maps to one component with multiple mount points across the screen specs:

- `<SearchBox query onSubmit>` — the oversized landing input (landing.md states 1-5) and the results-header input (search.md mockups A/B/B2). Same combobox behavior in both places: 300ms debounced `/suggest` fetch, AbortController in-flight cancellation, full ARIA combobox keyboard map (down/up/enter/tab-fill/escape), `aria-activedescendant` management. Without a framework this is the behavior we would paste twice and drift.
- `<SuggestionsDropdown groups onSelect>` — the grouped listbox (your history cap-3 / web suggestions cap-4, group labels skipped in keyboard nav). Consumed by every `<SearchBox>`; the `oxe-ac` opt-in toggle at its bottom edge is part of the component.
- `<ModeToggle mode onChange>` — the TRADITIONAL|AI segmented control with caps/filled active convention, `role="radiogroup"`, `localStorage` (`oxe-mode`) persistence. Used on landing and in every search-screen header; `[view classic]` / `[ask AI]` are the same component rendered as a link affordance.
- `<MetaLine count|age, source, age, ttl>` — the cache-transparency line ("12 results - from cache - 3h old - ttl 21h left"). Appears in traditional results, AI completed view (B2), and future history/dashboard surfaces. One component, three call sites today.
- `<ShareRow>` — copy link / copy json buttons (traditional view).
- `<ResultItem>` — Google-anatomy traditional result: favicon slot with lazy `icons.duckduckgo.com/ip3` load and absent-on-failure, domain line, `#1a0dab`/`#8ab4f8` title, two-line snippet, `<details>` cached-text preview. Rendered N times per traditional page.
- `<SourceCard index result>` — compact horizontal AI tile: favicon, domain, truncated title, `[n]` badge, hover lift, click → new tab + `POST /click`. Used in the streaming row (B) and completed row (B2); also the scroll-target of every citation click.
- `<Citation n target>` — the inline superscript `[n]` link that scrolls to and highlights its `<SourceCard>`. Rendered inside streamed answer text via a marker-to-component reconciliation pass as chunks arrive.
- `<AnswerStream chunks onAbort state>` — the streaming answer surface: fetch + `response.body.getReader()`, blinking `▌` cursor at stream head, `[stop]` abort keeping partial text, mid-stream failure state with retry / switch-to-classic (mockup C). The single most stateful component; hand-managing its transitions (streaming → completed → cached replay → error) is exactly the reactivity vanilla lacks.
- `<RelatedQuestions questions onSelect>` — the B2 related-question list; clicking pushes a new `?q=...&mode=ai` entry via `history.pushState`.
- `<Nav brand active>` — header with active-link bracket convention, shared by landing, search, history, dashboard.
- `<Pager page total>` — traditional results pagination.

Count: 12 components, of which 8 appear on more than one screen. That is the duplication the vanilla verdict forced and this stack eliminates.

## CSS layer

Three candidates against our fixed aesthetic (card-less Google anatomy, near-black warm dark canvas `#0d0817`-ish, off-white `#fafafa` light, one saturated accent, Geist self-hosted, `color-scheme: light dark`):

- **daisyUI 5 + Tailwind CSS 4 (chosen).** daisyUI is now a Tailwind plugin (`@plugin "daisyui"` in CSS, daisyui.com/docs/install/; Tailwind-first docs, Vite install guide is first-party). No JS ships from daisyUI or Tailwind; components are class vocabulary, so it is a true no-JS UI layer under Preact's hydration. Theming is CSS custom properties end to end (daisyui.com/docs/themes/): one custom theme for our canvas/accent tokens, `--prefersdark` wiring for `color-scheme: light dark`, and dark mode needs no class juggling. Output is purged to only used classes (typically 10-20KB gz for an app of this size; verify at implementation with the actual build). Risk noted honestly: daisyUI's component look is its own; for the result anatomy we override with Tailwind utilities + a few custom properties rather than fighting it. The choice over Bulma is driven by theming ergonomics and the Tailwind utility layer being the better tool for the Google-anatomy card-less detail work.
- **Bulma.** Framework-only CSS, no JS (bulma.io). Real strengths: Flexbox-based, Sass-modular (import only used pieces to trim the ~200KB+ raw minified build complained about in jgthms/bulma#1560/#1844), CSS-variables-based theming and dark mode are recent additions (bulma.io homepage: "CSS Variables, Dark Mode, Light Mode, Dart Sass"). But its dark mode story is younger than daisyUI's (which has shipped theme switching as a core feature across five major versions), its components carry more opinionated chrome that must be overridden for the near-black canvas / hairline-border look, and the Sass-modular path pulls in a Dart Sass build step that daisyUI+Tailwind 4 (CSS-first config) avoids. Pairing with Preact works fine (classes are classes); the loss is purely styling control per hour invested.
- **Hand-rolled custom-properties CSS.** Cheapest payload, full control, but it is the "re-implement everything" failure mode again at the CSS level: dropdown panel, segmented toggle, badge, hover-lift card are all hand-written and drift across screens. Rejected for the same reason vanilla JS was rejected, kept as the override layer on top of daisyUI for the result anatomy.

## Analysis

### Preact + Vite (chosen)

- Payload: Preact's core runtime is ~4.5KB min+gz (preactjs.com). App code, Geist woff2 subsets, and purged CSS dominate after that; total first-load JS target stays under ~25KB gz. Hydration is trivially fast at this component count (no suspense boundaries, no data-fetching framework; the app fetches from the same Python server it is served by).
- Streaming: chunked HTTP maps directly to `fetch` + `getReader()` inside `<AnswerStream>`; signal/state updates append tokens; abort is an `AbortController`. The B → B2 transition (cursor off, citations reconciled, related questions render) is state, not DOM surgery.
- Content negotiation: untouched. The Python server keeps `/search` HTML-vs-Exa-JSON negotiation and serves the Vite build as static files; the CSR app calls the same routes client-side. No router duplication of the URL contract: `pushState` mirrors `?q=&p=&mode=` exactly as spec'd in search.md.
- Dark mode + Geist: daisyUI theme tokens + `color-scheme: light dark`; Geist stays self-hosted woff2 via `@font-face` in the Vite asset pipeline (hashed, cached).
- Ecosystem: React API surface means every combobox/streaming pattern online is portable; `preact/compat` is the escape hatch if a React-only lib is ever wanted.

### Solid

Genuinely fine-grained reactivity and a ~7-8KB runtime (solidjs.com); JSX-based like Preact. Its compiler model and smaller community are the cost; the performance edge is unmeasurable here. No differentiator over Preact for 12 components. Rejected on ecosystem/maintainer-familiarity grounds.

### Svelte 5

Compiled output is small and ergonomics are lovely (svelte.dev), but Svelte 5's runes are a young, still-churning API (the 3→4→5 transition history is itself a maintenance warning for a single maintainer), and without SvelteKit we hand-roll routing/layout that Kit would then tempt us toward (which reintroduces the adapter-static SPA tradeoffs the rev-1 doc correctly rejected). Rejected.

### Lit

Web components would be a natural fit if the UI were a cross-site widget library; for a single-page app they add shadow-DOM styling boundaries that fight the global daisyUI/Tailwind theme tokens (lit.dev). Global CSS theming + shadow DOM = extra wiring for zero user-visible benefit here. Rejected.

### CSS decision detail

See the CSS layer section; the pairing note: Preact + daisyUI is a well-trodden combo (daisyui.com's framework install guides include Vite first-party), Bulma + Preact is also trivial but pushes all dark-mode and de-chroming work onto us. daisyUI's CSS-variable theming is also what makes a future accent tweak a one-line change instead of a Sass rebuild.

## Risks

- **Tailwind/daisyUI learning curve for a Python-first maintainer.** Mitigation: mise-pinned versions, the daisyUI theme system means most visual decisions live in one `@theme` block, not scattered utilities.
- **CSR means the search screen is JS-only by default.** no-JS degradation is no longer a criterion (owner decision), but agents/curl still hit the same URLs and get the Exa JSON via content negotiation, and the HTML served is a shell plus metadata; keep `Accept: application/json` as the documented agent path.
- **Purge/size drift.** Tailwind only emits used classes, but a careless wildcard can bloat CSS. Mitigation: size budget check in the build task (fail CI over ~30KB gz CSS + ~40KB gz JS). Concretely: a small bun script (`scripts/size-budget.ts`) runs after `vite build` in the `check` mise task; it reads Vite's build output, gzips each emitted `dist/assets/*.css` and `dist/assets/*.js` (`Bun.gzipSync`), sums per type, and exits nonzero over budget (30KB gz CSS / 40KB gz JS). Standalone equivalent for any asset: `gzip -c file | wc -c`.
- **Streaming edge cases** (IME composition, Enter races, partial-citation reconciliation mid-stream) now live in framework state. Mitigation: the state machine stays explicit in `<AnswerStream>`; Playwright e2e under `tests/e2e/` per repo convention.
- **daisyUI major-version churn** (v5 was a breaking release). Mitigation: exact pin in package.json, upgrade is opt-in and reviewable.
- **Two toolchains** (uv/Python + bun/JS). Mitigated, not eliminated, by mise: one file pins both, one command surface runs both.

## Tooling & tasks

`mise.toml` at repo root pins both toolchains reproducibly and defines the workflow as tasks (no README-runbooks):

```toml
[tools]
python = "3.12"        # matches the server runtime; uv manages packages
bun = "1.2"            # frontend build toolchain, dev-time only
uv = "latest"

[tasks.dev]
description = "Python server + Vite dev server with proxy"
run = "bun run dev"     # vite.config proxies /search,/suggest,/click,/mcp to 127.0.0.1:4479

[tasks.build]
description = "Static frontend build into oxe/static/dist/"
run = "bun run build"   # vite build; output committed? no, built in CI/package step

[tasks.serve]
description = "Production: stdlib python server serving built assets"
run = "uv run oxe"

[tasks.check]
description = "Typecheck + build + size budget"
run = "bun run check"
```

Key properties: `bun` exists only at build time; the installed/running oxe (`uv tool install oxe`) never needs node or bun; Python and bun versions are pinned per-repo so any machine reproduces the build. Tasks use `mise run dev|build|serve`.

## Migration path

1. Add `mise.toml`, `package.json`, `vite.config.ts`, `src/` at repo root; `bun install` once.
2. Build the component inventory above in `src/components/`, starting with the shared chrome (`<Nav>`, `<ModeToggle>`, `<SearchBox>`+`<SuggestionsDropdown>`) since they unblock both screens.
3. Vite outputs to `oxe/static/dist/` (hashed assets); the Python server mounts it read-only and keeps all routes. `ASSET_VERSION` cache-busting becomes content-hash based via Vite, which is strictly better.
4. Port screens in order: landing (toggle + combobox) → traditional results (`<ResultItem>`, `<MetaLine>`, `<ShareRow>`, `<Pager>`) → AI streaming (`<AnswerStream>`, `<SourceCard>`, `<Citation>`, `<RelatedQuestions>`) → history/dashboard as static-ish pages.
5. Server changes are minimal: serve `dist/index.html` shell for HTML `Accept`, keep everything else byte-identical. Add `GET /suggest` per the backend section (only new endpoint).
6. Keep the rev-1 doc's suggestion design intact; the vanilla `app.js` is deleted at the end of step 4.

## Suggestions backend

Unchanged from rev 1, restated for completeness:

- `GET /suggest?q=...` returns OpenSearch Suggestions JSON (`[q, [s1, s2, ...]]`, `application/x-suggestions+json`); doubles as a browser search-engine format via `opensearch.xml`.
- Ranking: SQLite `LIKE` prefix match over `search_log`, recency-then-frequency, capped 3-10.
- Client: 300ms debounce + AbortController, inside `<SearchBox>`/`<SuggestionsDropdown>`; local history group always on, DDG ac group opt-in via `oxe-ac` in `localStorage` (privacy default: keystrokes never leave the machine before submit).

## Cross-reference

landing.md and search.md predate this revision: where they reference hand-written `app.js` or a no-JS fallback, those requirements are superseded by the component build described here (no-JS degradation was dropped as a criterion; screen behavior is now owned by the component inventory above).

## Sources

First-party docs:

- https://preactjs.com/ — Preact "~4.5KB" core runtime claim (min+gz), React-compatible API
- https://vitejs.dev/guide/build/ — Vite production build: minification, chunking, hashed assets, browser targets
- https://solidjs.com/ — Solid fine-grained reactivity and size positioning
- https://svelte.dev/ — Svelte 5 compiler model and runes docs
- https://lit.dev/ — Lit web components, shadow DOM styling model
- https://bulma.io/ — Bulma: CSS-only framework, CSS variables, Sass modular, recent dark mode support
- https://bulma.io (issues) — jgthms/bulma#1560, #1844: minified build size discussions
- https://daisyui.com/docs/install/ — daisyUI 5 as a Tailwind plugin (`@plugin "daisyui"`), Vite install guides
- https://daisyui.com/docs/themes/ — daisyUI CSS-variable theming, 35 built-in themes, `--prefersdark`
- https://daisyui.com/docs/v5/ — daisyUI 5 release notes (Tailwind 4 alignment)
