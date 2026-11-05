# SPA UI/UX rewrite wave: docs, settings, filters, landing

Date: 2026-11-04. Parent: the v0.9.0 release wave (UX-09, #305). Scope: the four UI/UX
follow-ups raised after the production deploy, plus one open question (usage counts).

The wave is deliberately split so each piece lands on its own branch and can be reviewed
apart. Two of the five items (filters, landing) are contract changes, not cosmetics: they
touch what the server can express, so they need the plan's settled inputs read first.

## Settled inputs

Everything below was verified against the tree at `main` @ `dea2eae` (tag `v0.9.0`) on
2026-11-04. Do not re-derive it.

### 1. The filter gap is real and it is a contract gap, not a UI gap

`GET /api/search` accepts `time_range` and `safesearch` (`crates/cauce-server/src/handlers/search.rs:76`,
parsed at :83-91) and the cache key includes both (`crates/cauce-core/src/cache.rs:183-194`).
But no declarative engine can express them: `TemplateVars`
(`crates/cauce-engines/src/declarative/spec.rs:596-609`) carries only `q`, `page`,
`page_size`, `lang`, `market`, and `resolve_token` (:652-680) rejects anything else at
compile time. So today `time_range=week` is validated, cache-keyed, logged, and then
ignored by bing and brave. This is issue #122
(`.agents/plans/v3/later/engine-param-forwarding.md`), which called itself an honest gap.
It is no longer honest now that a filter UI would ship: a date-range control that does
nothing is worse than no control.

The upstream engines do support the filters:

| Window | Bing (`filters=ex1:"ezN"`) | Brave (`tf=`) |
| --- | --- | --- |
| Day | `ex1:"ez1"` | `pd` |
| Week | `ex1:"ez2"` | `pw` |
| Month | `ex1:"ez3"` | `pm` |
| Year | no preset (`ez5_<start>_<end>`, epoch days) | `py` |

Safe search: Bing `adlt=strict|moderate|off`; Brave `safesearch=strict|moderate|off`
(cookie is Brave's canonical channel, but a query param is honoured). Region: Bing already
has `mkt` via the spec's market map; Brave has `country=` (not sent today).

The template system has no conditionals and no value maps, so `{time_range}` cannot simply
render `pd|pw|pm|py`. The shape that fits the existing seam is a per-engine optional
parameter map on the spec (`request.params`) where each engine declares which request
fields it consumes and what value each field renders as, so a field that is unset is
omitted rather than rendered empty. That is the amendment the filters item proposes.

### 2. What the UI can offer today without touching the engine seam

- `lang` works end to end (Accept-Language header plus Bing's `mkt` map).
- `engines` pin works (server validates, 400 `unknown_engines`).
- `page` works.
- `time_range` and `safesearch` are API-only and inert.

So the mandatory-for-all-engines subset is **none** of the filters and all of the
per-request scope (`q`, `page`, `lang`, `engines`). Everything else is per-engine
capability. The design that follows from that: the filter row advertises only what the
selected engines can honour, and says so instead of silently dropping it.

### 3. docmd already knows how to do versions

The docs site is docmd (`docmd.config.js`, `deploy/cf-pages/build-docs.sh`, output
`docs-site`). docmd has a first-class versioning engine
(`dist/engine/versioning.js`): `config.versions = { current, all: [{ id, dir, label }] }`,
each version rendered into its own source dir, non-current versions under an id prefix,
with a version selector in the chrome (`window.DOCMD_VERSION_ROOT` is already emitted in
the live page). A missing current-version dir is a hard error; a missing old-version dir
is a soft skip. That matches the "previous version docs do not show the new feature"
requirement exactly: each version gets its own source tree, and the current tree simply
omits what did not exist yet.

The live site is served by a CF Pages project `cauce-docs` Git-connected to
`espetro/cauce` (per the instance runbook). Adding versions means staging version
source trees at build time, the same way `build-docs.sh` already stages
`.agents/docs/` into `docs/developers/`.

### 4. The docs copy is hard-wrapped and carries one version mention

Every `docs/*.md` paragraph is wrapped at ~95 columns (`docs/install.md` lines measure
97 and 77). The one version mention in the copy is `docs/threads.md:3` ("From 0.9.0 a
follow-up question continues..."). `docs/index.md` has none. The rewrite must unwrap the
paragraphs (one sentence per source line, no manual wrapping) and drop the version
sentence, so the previous version's tree simply lacks the threads page.

### 5. The current filter bars wrap, and one control spans two rows

`crates/cauce-server/web/src/spa/app/app.css:547` sets `.filters { display: flex;
flex-wrap: wrap; }` over two selects plus a `flex: 1 1 auto` search input, a checkbox, a
submit button and a clear link (`features/history/HistoryView.svelte:47-89`). The input
flexes, the rest are auto-width, so a control lands on row two while the input stretches
across the first. The requirement is one entry per row with controls mapping 1:1 to cells.

### 6. Settings is a form, not a settings page

`features/settings/SettingsView.svelte` (534 lines) is a formisch `Form` posting dotted
config paths to `PUT /api/config`. It renders every admin field in one flat column with
no grouping, no tabs, and no per-setting help text. The reference
(duckduckgo.com/settings) is the opposite: a tabbed page, one row per setting, label and
help on the left, control floated right, hairline group separators, no per-row save.

cauce's settings are also two different things in one file: browser-local preferences
(BYOK key, base URL) and server-side instance config (admin only). The redesign should
keep that split visible rather than mixing them in one list.

### 7. The landing needs the logotype above the search bar and more air below it

`features/landing/Landing.svelte` renders marketing sections below the omnibox
immediately. The reference (the two DuckDuckGo and Google screenshots) puts the logotype
directly above the search bar, centred, keeps the top nav minimal, moves contextual
actions below the search bar, and leaves a large empty band below it so the static
content is partly out of view (a scroll cue) and a plugin or trending-searches slot can
occupy the space without crowding.

### 8. Usage counts are not answerable from the data cauce stores today

`search_log` (`crates/cauce-store-sqlite/migrations/0001_init.sql:124-136`) has
`ts`, `client`, `query_hash`, `source`, `tier`, `latency_ms`, `result_count`,
`engines_json`, `deadline_hit` (plus `query_raw` and `origin` from later migrations).
There is **no** client IP, no user id, no session id and no bearer-token hash on the row.
`client` is a coarse label (`ui`, `api`, `mcp:<name>`, `cli`) and `origin` is
`user` vs `agent`.

Two consequences:

- On a public instance `search_log` is never written at all: the pipeline runs with
  `.with_history_logging(!cfg.server.public_instance)`
  (`crates/cauce-core/src/pipeline/mod.rs:586`, `crates/cauce-server/src/app.rs:356`),
  so `cauce.fyi` has no rows to count. `GET /api/stats` is admin-scoped and returns 401
  anonymously (verified live).
- Even locally, "how many users" would have to be an estimate from the `client` label
  (`mcp:<name>` names the agent harness, `ui` is a browser). Distinct humans are not
  distinguishable from distinct browsers without an identity column.

The honest answer is therefore an **estimate, not a count**, and it needs a decision
about whether to add an identity column at all. Adding one to a public instance has
privacy implications that are the owner's call, not the implementer's.

## Design decisions

### D1: versioned docs trees, unwrapped copy, no version talk in the copy

- Unwrap every paragraph in `docs/*.md` (and `.agents/docs/*.md`): one sentence per
  line, no hard wrapping. The rendered line count then follows the viewport.
- Remove version references from the copy. `docs/threads.md` drops its "From 0.9.0"
  sentence; the threads page exists only in the 0.9.0-and-later tree.
- Version selector: keep the docs on `docmd` and use its native `versions` config.
  Structure:
  - `docs/` stays the **current** version's source tree (unchanged path, so the existing
    build keeps working).
  - `docs-versions/<id>/` holds archived trees (e.g. `docs-versions/0.8/` with the
    0.8-era copy).
  - `build-docs.sh` stages the current tree plus any archived trees it finds, and
    writes a `docmd.config.js` with `versions: { current: "0.9", all: [...] }`.
  - The selector must be visible; docmd renders it from `versions.position`
    (default `sidebar-top`), so confirm the placement matches the reference.
- The archived tree for 0.8 must be produced from the v0.8.1 tag, not reconstructed by
  hand: `git show v0.8.1:docs/install.md` etc. That is the only way the "previous version
  does not show the new feature" rule is actually true.
- Tone: the copy passes `tono check` (the local voice linter, `../tono`). The em dash
  and the "not just X, but Y" constructions are exactly what it flags.

### D2: settings as a tabbed page, one row per setting, no per-row save

- Four tabs, matching what cauce actually has: **General** (browser-local: theme,
  language, results-per-page, default engines), **Privacy** (safe search, region),
  **Search defaults** (default engines, TTL, page size), **Instance** (server config,
  admin only), plus **AI** (BYOK key and base URL, the browser-local key section that
  exists today).
- Row anatomy follows the reference: label on line one, one-line help on line two, the
  control aligned right on wide viewports and stacked below on narrow ones.
- Toggles are a `role="switch"` button with a visible On/Off word, not a checkbox whose
  accessible name is "On".
- Selects are styled native-like selects with a chevron, matching the existing
  `ui/select.svelte` token treatment so the DS stays intact.
- Tabs use the vendored bits-ui `ui/tabs.svelte` (already in the repo), with
  `role="tablist"` semantics, not DDG's anchor links.
- No per-row save: browser-local settings apply immediately; instance config keeps the
  existing explicit save (it is a server `PUT` with a restart contract, so pretending it
  is instant would lie).

### D3: filters as a shared primitive, one row, honest capability

- One `FilterBar` primitive used by both search and history, built on the DS primitives.
- Layout: a grid, not a wrapping flex row. Every control is one cell, `min-width: 0`,
  labels truncate with an ellipsis, and the only stretching control is the free-text
  input, placed last. Below ~700px it collapses to one column (the reference's stacked
  pattern). No entry spans two rows.
- Multiselect is a first-class need, so the primitive needs a multi-select variant
  (`ui/multi-select.svelte`), which does not exist today: `ui/select.svelte` is
  `type="single"`. Engine pick is the multiselect case (the server already takes a
  comma-separated `engines` pin).
- Search page: the filter row sits **below** the search bar (contextual actions), and the
  top nav keeps only global actions (the reference's split). The search filter row
  carries: engine (multiselect), language, safe search, date range.
- History page: since-window, origin, cached-only, and the free-text query, in the same
  primitive, so the two surfaces read as one system.
- Capability honesty: the filter row only offers controls the selected engines can
  honour. Until the engine seam grows (item 2 above), date range and safe search are
  either wired to the engine seam in the same wave or marked unavailable in the UI.
  They must not be shipped as no-ops.

### D4: landing with the logotype above the bar and air below it

- Logotype (mark + wordmark) centred directly above the search bar, on the landing only.
  The top nav keeps the mark on the left and the global actions on the right.
- Below the search bar: a reserved band. On the landing it holds contextual affordances
  (trending searches for the instance, or a plugin slot) and stays empty when there is
  nothing to show.
- The static marketing content starts below that band, so the first screen shows the
  bar, the logotype and empty space, and the content is partly out of view (the scroll
  cue the reference gets).
- On the search page the same band position carries the contextual filter row instead of
  marketing content.

### D5: usage counts as an assessment, not a feature

- Report what is knowable today: distinct `client` labels and their request counts
  (`by_client` in `StatsSnapshot`, already computed by
  `crates/cauce-store-sqlite/src/store/stats.rs:55-71`), and the `origin` split
  (human vs agent). That is the honest ceiling with today's columns.
- Report what a real count would need: an identity column (hashed client id or IP
  prefix) on the request path, plus an explicit privacy decision for public instances,
  plus counting outside the search path (the page view, the health check, the MCP
  handshake are all requests too).
- Recommend: do not add identity columns to the public instance. Expose the aggregate
  counters (`by_client`, `origin`) instead, and label them as activity counts, not users.
- This item ships as a document, not code.

## Waves

| Item | Branch | Scope | Depends on |
| --- | --- | --- | --- |
| D1 docs | `v3/docs-versions` | docs copy + version trees + build | none |
| D2 settings | `v3/settings-redesign` | SPA settings page | none |
| D3 filters | `v3/filter-primitives` | `FilterBar` + multi-select + engine seam | D3a |
| D3a engine seam | `v3/engine-param-forwarding` | spec `request.params`, bing/brave filters | none |
| D4 landing | `v3/landing-rewrite` | landing + nav split | D2 (shared nav) |
| D5 assessment | none (document) | `.agents/notes/` + issue comment | none |

D3a is the risky item and lands first on its own: it changes the declarative engine
contract, and everything in D3 depends on it.

## Verification

- `mise run validate` green on every branch (the pre-push hook runs it).
- `tono check` on every changed docs file, with the score at or above the project
  threshold.
- The docs site builds locally (`bash deploy/cf-pages/build-docs.sh`) and the version
  selector switches between trees; the 0.8 tree renders without the threads page.
- Playwright walkthrough of `/app`, `/app/settings`, `/app/search?q=`, `/app/history` at
  1440 and 375, light and dark, with zero console errors and no layout collapse.
- The filter bar at 1440 and 375 shows every control on one row (or one column when
  stacked), never two rows for one entry.
- A date-range filter on the search page demonstrably changes the upstream request (a
  canary log shows `filters=ex1:"ez2"` for bing and `tf=pw` for brave), not just the
  cache key.

## Risks

- The engine-seam change touches the declarative spec's compile-time validation, so a
  bad spec must fail at load, not at search time. The existing `validate_template`
  probe is where that belongs.
- docmd's versioning expects real source directories per version. A tag-derived 0.8
  tree is a build-time staging step; if the staging fails, the build must fail loudly
  rather than silently shipping a one-version site.
- The landing rewrite touches `TopNav`, which every surface shares. The nav split
  (global vs contextual) must not break the admin gating that `routeVisible` and
  `capabilities.flags` currently provide.
