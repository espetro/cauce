# Frontend replacement assessment — HTMX → SPA

Status: assessment + phased plan, not a commitment. Parent plan:
`2026-09-21-v3-rust-core.md`; companion to `v3/ts-migration.md` (which this would
supersede *at the UI layer only* — the strict-TS toolchain it built carries over).

The proposal under review: Svelte 5 (SvelteKit or Vite SPA) + Specta for Rust→TS
type generation, Bits UI + shadcn-svelte, TanStack Query, svelte/transition;
runner-up SolidJS.

## Verdict (short form)

- **Conditional go for a Svelte 5 Vite SPA**, gated on HTMX search + AI mode
  validated working (gate in §5.0). Embed the build via `rust-embed` — the
  single-binary and `<80 MB` stories are untouched (§3, §6).
- **Reject the Specta swap**: ts-rs is already integrated, gated, and covers the
  type shapes in this codebase. Specta's headline advantage is false against
  ts-rs ≥ 8 (§2.4).
- **Reject SvelteKit**: SSR needs a JS runtime in the request path or a second
  process — both break the single-binary/loopback model for zero SEO payoff (§3).
- **Honest plan B exists**: the pain concentrates in three hand-rolled DOM
  modules; "HTMX + declarative islands" reaches most of the benefit at a fraction
  of the churn (§3.4, §5 alt path).

## 1. Current frontend surface inventory

### 1.1 Routes and pages

Every UI surface is declared in `crates/cauce-server/src/routes.rs` (the
`ROUTES` table, lines 86–188) and mounted in `app.rs::handler_for` (lines
330–402). All pages ride the `ui` cargo feature and the runtime `--headless`
switch (`app.rs:229-248`).

| Page | Template | Interactivity | JSON counterpart (already mounted) |
|---|---|---|---|
| `/` | `page.html` (`Page`, `html/mod.rs:61-104`) | search form, AI-mode pill (`web/src/app.ts`) | — (form GETs `/search`) |
| `/search` | `page.html` + `results.html` | SSE stream via custom htmx `sse` ext (`sse-connect`, `page.html:35-38`), `hx-get` "more" pagination, `hx-post` click beacon, Search Assist trigger/card | `GET /api/search`, `GET /api/search/stream`, `POST /api/click`, `POST /api/answer` (assist) |
| `/answer` | `answer.html` | SSE-over-**POST** via fetch+reader, multi-turn threads, `[n]` citation linking, follow-up form | `POST /api/answer` (`handlers/answer.rs:107-162`) |
| `/history` | `history.html` | `hx-delete` row removal, filters | `GET /api/history`, `DELETE /api/history/{id}` |
| `/dashboard` | `dashboard.html` | read-only; SVG bar charts with geometry precomputed in Rust (`dashboard.rs:28-60`) | `GET /api/stats` |
| `/cache` | `cache.html` + `cache_payload(_error).html` | `hx-delete` single/bulk, `hx-confirm`, payload inspect fragment | `GET/DELETE /api/cache[/{key}]` |
| `/engines` | `engines.html` + `engine_card.html` | per-card reset `hx-post`, inline test query swapping the `search_fragment.html` partial | `GET /api/engines`, `POST /api/engines/{id}/reset|enable|disable` |
| `/audit`, `/trace/{id}` | `audit.html`, `trace.html` | read-only | `GET /api/audit` (`AuditRow`, `store.rs:302`) |
| `/settings` | `settings.html` + `settings_cache.html` | `hx-put` `/api/config` form, `hx-delete` cache buttons | `GET/PUT /api/config` |
| `/archive` | `archive.html` + `archive_markdown(_error).html` | archive search + listing; markdown fetch fragment | `GET /api/archive`, `GET/POST/DELETE /api/pages[/{url}]` |
| `/opensearch.xml`, `/favicon.ico` | generated in `assets.rs` | — | — |

**Every HTML page already has a mounted JSON counterpart.** The pages negotiate
on `Accept`/`HX-Request` inside the same handlers (`handlers/search.rs:40-53`,
`handlers/history.rs:32-46`, `html/mod.rs:107-129`) — a settled "one data path"
input, which is precisely what makes an SPA tractable: the wire contract a SPA
needs mostly already exists and is exercised by tests (`tests/wire_types.rs`,
`tests/sse.rs`, `tests/ui_shell.rs`).

### 1.2 Where the logic lives

- **Templates**: 20 Askama files, ~1,226 lines total; display-only (plain
  `String`/`Display` fields — `Row` is deliberately strings-only,
  `html/mod.rs:50-59`). Rust page modules do all shaping: badges, engine-status
  strings, more-URL building, SVG chart geometry, i18n via `tr()` —
  `html/search.rs`, `html/answer.rs`, `html/history.rs` (485), `html/settings.rs`
  (519), `html/archive.rs` (297), `dashboard.rs` (399), `cache_page.rs` (342),
  `engines_page.rs` (341), `audit_page.rs` (405). ~2,900 LoC of Rust is
  page-shaping code that an SPA replaces with client-side components.
- **Client JS**: `web/src/` is **already strict TypeScript** (ts-migration
  steps 1–3 landed: PRs #215, #218, #219) — ~1,570 LoC src + ~1,545 LoC vitest
  tests, bundled by rolldown to committed `assets/app.js`, inlined `|safe` into
  every page (`assets.rs:43-52`, `page.html:74`). Vendored htmx 2.0.4 +
  htmx-ext-json-enc are bundled deps (`package.json`). The heavy modules are the
  streaming/DOM-building ones: `answer.ts` 447, `search.ts` 326, `assist.ts` 228,
  `sse.ts` 141.
- **i18n**: single `locales/en.yaml` catalog embedded at compile time
  (rust-i18n); JS-visible string bundles serialized as `var S`/`SA`/`AS`
  literals in the page and mirrored to `web/src/i18n/*.json` by
  `gen_i18n` (`i18n.rs:1-22`). Per-request `Accept-Language` is out of scope by
  design.

**Migration scope measurement**: the SPA would replace ~1,226 template lines +
~2,900 LoC of Rust page shaping + ~1,570 LoC of DOM-driving TS, while consuming
the existing `/api/*` surface. The two SSE consumers (`/api/search/stream` GET,
`/api/answer` POST) port nearly as-is — `sse.ts` already implements the
fetch+reader pump a SPA needs (`EventSource` can't POST, `answer.rs:104-106`).

### 1.3 Auth / session surface

None. UI and API are unauthenticated; safety is the `HostGuard` + Origin check
(`middleware.rs:155-227`) plus forced-loopback bind (`config/mod.rs:370-390`,
`[auth]` is a deferred stub — token auth lives in
`v3/later/postgres-and-multi-instance.md`). An SPA inherits this unchanged;
bearer-token plumbing is a deferred wave-6-adjacent concern, not a blocker.

## 2. Proposal verification against the codebase

### 2.1 "Rust handlers already return typed structs" — **mostly true**

The streaming/search/AI contract is already `#[derive(TS)]` and exported:
`tests/wire_types.rs:25-55` regenerates `web/src/types/*.ts` (committed,
freshness-gated by `mise run web` → `git diff --exit-code`, `mise.toml:63-77`).
Exported roots: `SearchResponse`, `StreamMeta`, `ResultsFrame`, `AnswerFrame`,
`AnswerTurn`, `ApiError` + transitive deps. 17 `derive(TS)` sites
(`response.rs`, `ai/answer.rs`, `error.rs`).

**Gaps** (must close before a typed SPA client):
- Serialize-only, no `TS`: `HistoryItem` (`store.rs:352`, `tag="kind"` internal
  tagging), `StatsSnapshot` (`store.rs:704`), `AuditRow` (`store.rs:302`),
  `EngineView` (`engines.rs:52`), `CacheListing` (`cache.rs:37`), `ArchiveRow`
  (`archive.rs:30`), `IndexBody`/`AnswerBody` request shapes
  (`handlers/answer.rs:34-51` is private — needs `pub` + `TS`).
- Ad-hoc `json!` bodies: delete acks `cache.rs:158,205`, `pages.rs:191`,
  `engines.rs:353`, history envelopes `history.rs:111,223-231`, `config.rs:192`
  (PUT echoes the parsed body). These need real structs to be typed on the wire.
- `/api/suggest` returns the OpenSearch `[term, [..]]` tuple — an untypeable
  two-element array shape by design (`suggest.rs:29-52`); model as
  `[string, string[]]` in TS.
- `QueryParams` is strict — unknown keys are 400 (`parse_search_request`,
  `handlers/search.rs:76-110`). The SPA client must emit only documented params.

### 2.2 "MCP/API layer shares types with the SPA contract" — **partially**

MCP tool **inputs** are `Deserialize + JsonSchema` structs in a different schema
domain (`mcp/mod.rs:174-258`: `SearchWebArgs`, `CacheInvalidateArgs`, …) — they
describe tool-call args to the LLM client, not browser payloads; keep them out
of the TS export. MCP tool **outputs** reuse the same wire shapes
(`search_web` returns the canonical `SearchResponse`), so response types are
already shared. Verdict: the `/api/*` JSON surface *is* the SPA contract; MCP
adds nothing the SPA needs.

### 2.3 "SSE endpoints a SPA would consume differently" — **yes, and already handled**

Two endpoints (`routes.rs:115`, `routes.rs:143-149`): `GET /api/search/stream`
(EventSource-friendly; `client=ui` query param exists precisely because
EventSource can't set headers, `handlers/search.rs:122-126`) and
`POST /api/answer` (SSE-over-fetch; `sse.ts`'s `parseSseFrame`/`pumpSse` already
implements the pump). A SPA consumes both identically to today — this is the one
place HTMX genuinely strains (custom `sse` extension in `sse.ts:10-40`, terminal
`meta`/`error` self-close, "new results above" bookkeeping in `search.ts`), and
where a declarative framework pays off.

### 2.4 Specta vs ts-rs for this codebase's type shapes — **keep ts-rs**

The proposal picks Specta. Against this codebase that is a lateral move, not an
upgrade:

- Specta's own "why not ts-rs" (no transitive export) is stale — ts-rs
  `export_all` already emits every dependency (`wire_types.rs:30-43`).
- The shapes exercised are all covered by ts-rs today, with attributes already
  in place: externally tagged enum w/ struct variants (`Source`,
  `response.rs:22-44`), internally tagged (`AnswerFrame` `tag="type"`,
  `ai/answer.rs:149-184`; `HistoryItem` `tag="kind"`), `flatten`
  (`StreamMeta`/`StreamResult`, `response.rs:142-162` via `#[ts(flatten)]`),
  `Option`/`skip_serializing_if`, `deny_unknown_fields` (deser-only, invisible
  to TS output), `#[ts(type = "number")]` on `u64` fields (both tools share the
  >2^53 JS caveat — an audit item, not a differentiator).
- Switching means re-annotating 17+ derive sites (`TS`→`Type`, `#[ts]`→
  `#[specta]`), rewriting the export test + banner post-pass + freshness gate,
  for no wire-visible gain. **Spend that effort on FX-01 coverage instead.**
- If a *client-command* codegen (rspc/tauri-specta style typed RPC) is ever
  wanted, that's a separate decision; nothing in this task needs it.

## 3. Stack verdict for this codebase

Constraints: single binary `cauce serve`, loopback-first, no auth, < 80 MB idle
RSS / ~30 MB binary (`2026-09-21-v3-rust-core.md:46,394,424-426`), human web UI
+ agent API off one router, toolchain already pnpm+tsc+rolldown+vitest.

### 3.1 Svelte 5 SPA via Vite → rust-embed — **recommended (gated)**

- Bundle: Svelte 5 compiles to ~no-runtime; realistic app.js-equivalent is
  ~60–120 KB min+gzip — noise against a 30 MB binary; **idle RSS is
  server-side and unchanged** (static file serving is already how assets ship).
- Single-binary intact: `vite build` → `assets/spa/` → `rust-embed`, same
  pattern as today's `assets/` folder (`assets.rs:20-29`); HTML routes become a
  fallback serving `index.html`. `--headless`/`ui` feature gating unchanged.
- svelte/transition + runes replace the hand-rolled `createElement` churn where
  it hurts most (`answer.ts`, `assist.ts`, the stream renderer in `search.ts`).
- Vite vs staying on rolldown: `vite` 8 + `rolldown` are already devDeps
  (`package.json`); rolldown-vite is the aligned path. No new toolchain *class*.

### 3.2 SvelteKit — **rejected**

SSR is the entire point of SvelteKit and it requires either a Node server in
the request path or a prerendered static export. Option A breaks the
single-binary/loopback model; option B is a worse version of 3.1 (you adopt a
meta-framework's routing/data conventions to use none of them). SEO is
irrelevant on a loopback metasearch UI; `/opensearch.xml` already covers the
only browser-integration surface that matters (`assets.rs:67-110`).

### 3.3 SolidJS — **acceptable runner-up, no decisive edge**

Smaller runtime (~7 KB), JSX may ease the port from hand-rolled DOM code. But
Svelte's compiled syntax is terser for this form/streaming-heavy surface,
`svelte-check` bundles a11y linting, and Bits UI/shadcn-svelte is the more
cohesive component story. Only revisit if Svelte 5 runes hit a wall in
prototyping.

### 3.4 Keep HTMX + islands — **the honest plan B**

The current architecture works and is gated by tests. The *measured* pain is
three modules (~1,000 of 1,570 LoC) doing imperative DOM building. A lighter
path: keep Askama pages, replace the DOM-heavy modules with small Svelte
components mounted as islands (Vite lib mode, same committed-bundle pipeline).
That captures ~70% of the declarative-DOM win at ~20% of the churn, and keeps
no-JS degradation (today's pages render server-side; streams degrade to
`noscript` notices — `page.html:39`). If the phased plan stalls at FX-03, this
is the stable resting point.

## 4. Agent-first claims — sanity check

| Claim | Verdict |
|---|---|
| "Token-efficient for agents" | Direction true, magnitude modest. Svelte is ~30–40% terser than equivalent React *and* than our `createElement` chains — the real win is declarative DOM, not a new axis vs. the existing terse TS. |
| "No useEffect traps" | Mostly true — runes/`$derived` kill the effect-dependency class of bugs. Correction: **SSE still needs lifecycle management** (`onMount`/effect cleanup + `AbortController`); the pump in `sse.ts` ports into a `$effect` or store, it doesn't vanish. |
| "svelte-check shift-left" | True and additive: typed templates + a11y warnings at build. But note the codebase *already* runs strict `tsc` + vitest + happy-dom (`web/AGENTS.md`) — this raises the floor further, it's not a new category. |
| TanStack Query for data | Correct for GET endpoints; **wrong tool for the SSE streams** — one-shot POST streams aren't a query cache. Use `$state` + the existing pump (or a tiny SSE store). |
| Bits UI + shadcn-svelte | Bits UI (headless) is a good fit. **Flag:** shadcn-svelte implies Tailwind, which would stand up a second styling system next to the hand-tuned 1,242-line `style.css` design system — decide "port tokens to Tailwind" or "keep style.css + Bits UI" up front; don't run both. |

## 5. Phased plan

### 5.0 Gate (hard precondition)

Owner validates search **and** AI mode working on the HTMX UI (per the task:
W2-10-style usage validation post-W7; `release-v3.0.md` exit criteria). Do not
branch the frontend before this — the HTMX build is the reference
implementation every phase is diffed against, and v2's lesson (decisions.md,
2026-09-21) is that the framework *wasn't* the cost — ungated cross-cutting
retrofits were.

### 5.1 Phases → proposed issues (EPIC "W8 frontend replacement", effort S/M/L)

| # | Step | Effort | Do | Acceptance |
|---|---|---|---|---|
| FX-01 | Wire-contract completeness | S | `derive(TS)` on `HistoryItem`, `StatsSnapshot`, `AuditRow`, `EngineView`, `CacheListing`, `ArchiveRow`; replace `json!` acks with typed envelopes; `pub`+`TS` on `AnswerBody`; `suggest` tuple documented | `wire_types` exports cover every `/api/*` response; freshness gate green |
| FX-02 | SPA toolchain | S | `svelte` 5 + `vite` (rolldown) into `web/`; `vite build` → `assets/spa/`; extend `[tasks.web]` gate (`svelte-check`, bundle freshness); embed via `rust-embed` | `mise run web` green; `GET /app` serves shell in `ui` builds |
| FX-03 | `/search` parity | M | SPA shell + router (history mode + `index.html` fallback); search page incl. SSE stream, badge/meta line, assist trigger, click beacon, theme toggle, i18n via generated `web/src/i18n/*.json` | golden-path parity vs HTMX page; `stream=1` behavior identical; hidden behind `/app` prefix |
| FX-04 | `/answer` + Assist | M | SSE-over-POST pump ported from `sse.ts`; multi-turn thread, `[n]` citations, confidence/path chips, follow-up form; Assist card as component reused on `/search` | answer e2e parity (replay engine + stub provider, see `testing-cauce-serve` skill) |
| FX-05 | Admin/read pages | L | `/history` `/dashboard` (SVG charts → components) `/cache` `/engines` (incl. inline test) `/audit` `/trace` `/settings` `/archive` | route-by-route parity checklist; htmx fragments deleted per page |
| FX-06 | Teardown | S/M | drop `askama`, `templates/`, `html/*`, `*_page.rs`, htmx deps; `RouteKind::Html` rows → SPA fallback; `ui_shell`/`routes_table` tests updated; header/nav absorbed into shell | `cargo check --no-default-features --features mcp` still green; binary no larger; plan + AGENTS.md updated |

**What stays**: every `/api/*` route, both SSE endpoints, `/mcp`, pipeline,
cache, `Store`, `HostGuard`/Origin guard, `routes.rs` as the single surface
declaration, the i18n catalog + `gen_i18n` (JSON output becomes the SPA's
string source), the strict-TS toolchain and freshness-gate conventions from
`ts-migration.md`.

**What moves**: all page shaping (~2,900 LoC Rust + templates) → Svelte
components; DOM modules → runes-based components; `Page`/`Row` structs → props.

### 5.2 Convention fit

Same as `ts-migration.md`: cross-wave file, one issue per FX step, `For
Refinement` until picked, worktree `~/.worktrees/cauce-fx-<n>` on
`v3/fx-<n>-<slug>`, PR `Closes #<issue>`. Classification labels: FX-01 `infra`,
FX-02 `infra`, FX-03/04/05 `feature`, FX-06 `cleanup`→`feature`.

## 6. Risks

- **Bundle vs budget**: non-issue as analyzed (§3.1) *if* deps stay lean — the
  real risk is dep creep (Tailwind+shadcn+Bits+Query ≈ +5-10 npm deps with their
  own config surface) into a repo whose toolchain is deliberately minimal.
- **Dual-stack interlude**: phases FX-03→FX-05 run two UI stacks in one crate;
  each phase must be landable behind `/app` with zero HTMX regression, or the
  interlude becomes permanent.
- **i18n**: catalog is compile-time server-side; SPA consumes generated JSON
  bundles (`gen_i18n` output) — keeps one source of truth, but per-page
  `var S`-style scoping must become an import-level split.
- **a11y**: today relies on `aria-live`/`aria-busy` hand-wiring; svelte-check
  warnings help, but streaming regions need deliberate porting (the
  `noscript` degradation path is *lost* in a pure SPA — accepted or mitigated
  with a static `<noscript>` shell page).
- **Auth**: none today; if token auth lands (`later/postgres-and-multi-instance.md`)
  the SPA needs `Authorization` header plumbing — trivial, but the
  same-origin assumption in `HostGuard` stays load-bearing.
- **SEO**: none needed (loopback tool); `opensearch.xml` + `favicon.ico` stay
  as generated document routes independent of the SPA.
- **History note**: v2 was a React SPA abandoned partly *because* the SPA
  pushed contract drift into codegen that was never gated
  (`.agents/notes/2026-09-17-velocity-retro.md`). v3 already solved the gating
  (freshness check, strict tsc, wire_types test) — this plan keeps every one of
  those gates and adds svelte-check; the v2 failure mode stays covered.
