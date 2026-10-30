# W8: Gemini-class UX — threads, conversation shell, minimal chrome

Iteration 9. Priority P1. Parent: `../2026-10-29-gemini-ux.md`. Index: `README.md`.
Previous: `wave-7-ai-ux.md`. Target release: **v0.9.0**.

## Goal

Turn the AI surface into a conversation and strip the chrome down to what a daily driver
needs, without forking the layout into two designs and without touching the theme.

## Settled inputs

- `POST /api/answer` accepts `history: Vec<AnswerTurn>` (W7-04) and the server replays it
  to the provider. `AnswerBody` is `deny_unknown_fields` — new fields must be declared.
- The SPA already owns the live thread in memory and sends the full history
  (`features/answer/thread.svelte.ts:336-343`).
- `done.log_id` is the `answer_log` row id; the SPA's `replaceState` to `/answer/{id}`
  (`thread.svelte.ts:271`) leaves the `/app` mount — this is the follow-up bug.
- `GET /api/answer-log/{id}` exists but is `.admin()`-gated (`routes.rs:207`); the SPA never
  calls it. `answer_log` has no owner column, so per-user isolation needs a new one.
- `answer_log` stores one row per run. There is no thread table.
- The server rejects `history` above 20 turns / 64 KiB with `400 bad_request`
  (`handlers/answer.rs:32-34`); the UI surfaces it as a generic stream failure.
- Frames: `step` · `delta` · `sources` · `done` · `error` (`crates/cauce-core/src/ai/answer.rs`).
- The landing is a **build-time** flag (`VITE_CAUCE_LANDING=1`, `lib/flags.ts:16`); the
  binary's rust-embed build never sets it. It must stay a build-time flag — the Pages
  deploy renders the landing even when the backend is unreachable.
- SPA rules: Svelte 5 runes, Tailwind v4 only (tokens in `app/app.css`), vendored
  `ui/` (bits-ui) + `ai/` (sv-prompt-kit) primitives, phosphor icons, features import
  `ui/` + `ai/` + `lib/` only, no cross-feature imports.
- Type rules: named `interface`s for object types, never inline object-literal types in
  signatures; ts-rs wire types are generated, never hand-edited.

## Exit criteria

1. A follow-up turn survives a reload and a back/forward navigation, still inside the SPA.
2. Sources open in a side panel (desktop) / drawer (mobile) scoped to the selected turn.
3. The composer is at the top on an empty thread and sticky at the bottom once turns exist.
4. The nav shows four items left, a theme toggle and a waffle menu right; everything else
   is inside the menu.
5. The results page is a single column of hairline rows with one meta line.
6. `mise run validate` is green on every step and the golden path stays green.

## Steps

### UX-01 Thread store + API

- Issue #306 · Effort L · Label feature · Team Core+UI · Branch `v3/ux-01-thread-store`
- Depends on: —
- Do: add the `answer_thread` table (migration `0006_answer_threads.sql`, appended — never
  edit a shipped migration) with `id`, `owner`, `title`, `created_at`, `updated_at`,
  `turns_json`; `AnswerThreadRow` in `cauce-core::store` (ts-rs); `Store` trait methods
  `upsert_thread` / `list_threads` / `get_thread` / `delete_thread`; the SQLite
  implementation; the conformance-suite cases. `owner` is `"local"` in local mode and the
  hashed bearer identity in public mode — a public instance never serves another user's
  thread. `POST /api/answer` gains an optional `thread_id`: with it, the completed turn is
  upserted and the id is returned in `done.thread_id`; without it, behaviour is unchanged.
  Handlers: `GET /api/threads`, `GET /api/threads/{id}`, `DELETE /api/threads/{id}`;
  route rows in `routes.rs` with the right `RouteAuth` class (owner-scoped, not
  `.admin()`), plus the parent plan §6 route-table update the conformance test parses.
- Acceptance: a thread upserted via `POST /api/answer` is readable by `GET
  /api/threads/{id}` and lists newest-first; another owner gets `404`; delete removes it;
  the migration applies on an existing v5 database; `cargo test -p cauce-core` and
  `-p cauce-store-sqlite` green; the route-table conformance test green.
- Follow-up: UX-02.

### UX-02 SPA thread hydration + URL fix

- Issue #307 · Effort M · Label bug · Team UI · Branch `v3/ux-02-thread-hydration`
- Depends on: UX-01
- Do: the SPA's answer URL becomes `/app/answer?thread=<id>` — inside the mount, so reload
  and back/forward stay in the app. On mount with a `thread` param, hydrate from
  `GET /api/threads/{id}` and rebuild the transcript; on failure fall back to the
  localStorage copy of the current transcript (Q18). The `replaceState` to
  `/answer/{log_id}` (`thread.svelte.ts:271`) is removed; the server-rendered `/answer/{id}`
  page stays for durable single-answer links. Add the client-side history counter: warn at
  15 turns, and surface the server's `400 bad_request` ("history exceeds 20 turns") as a
  specific message instead of a generic stream failure.
- Acceptance: a follow-up turn survives a page reload with the whole transcript visible;
  back/forward between two threads restores each one; the localStorage fallback renders
  when the server read fails; the 20-turn error is explicit; `web/tests/answer-thread.test.ts`
  extended for hydration + the URL shape.
- Follow-up: UX-03.

### UX-03 Conversation shell — prompt card, sticky composer, unified body

- Issue #308 · Effort L · Label feature · Team UI · Branch `v3/ux-03-conversation-shell`
- Depends on: UX-02
- Do: one shared shell for both modes. The query renders as a top-anchored prompt card; the
  body below is either the result list (search mode) or the answer thread (AI mode) — same
  component tree, two body variants, no layout fork. One composer component whose position
  is driven by state: top-anchored on an empty thread, sticky at the bottom once turns
  exist, with the first question still visible at the top of the thread. Auto-scroll only
  while the user is already at the bottom (the existing rule). No `useEffect`-style
  side-effect patterns — Svelte runes only.
- Acceptance: the empty-thread composer is at the top; after the first turn the composer
  sticks to the bottom and the thread scrolls; search mode and AI mode share the prompt
  card and the composer component; keyboard-only submit works in both;
  `svelte-check` green.
- Follow-up: UX-04, UX-05, UX-06.

### UX-04 Sources panel + mobile drawer

- Issue #309 · Effort M · Label feature · Team UI · Branch `v3/ux-04-sources-panel`
- Depends on: UX-03
- Do: sources leave the inline flow. A right-side panel (desktop) / bottom drawer (mobile,
  drag to dismiss, ≤80vh) lists the selected turn's sources as numbered rows (favicon ·
  domain · title · snippet). The panel scope follows the selected turn; the header reads
  `Sources · turn N/M`. Each answer turn gets a `Sources (N)` chip that opens the panel
  scoped to that turn. The inline `SourceCard` list is removed from the answer body.
  Inline citation chips anchored inside the answer prose are NOT in this step (follow-up).
- Acceptance: opening the panel from a turn chip shows that turn's sources with the right
  `turn N/M`; the panel opens and closes from the keyboard; at 390px it is a bottom drawer
  that does not cover the sticky composer; the answer body no longer contains the inline
  source list.
- Follow-up: UX-06 (shared mobile sheet primitive).

### UX-05 Minimal search results + meta line

- Issue #310 · Effort S · Label cosmetic · Team UI · Branch `v3/ux-05-minimal-results`
- Depends on: UX-03
- Do: the results page becomes a single column of hairline rows (favicon · title · host ·
  2-line snippet), no borders or cards. One muted meta line replaces the cache-details
  expander, the separate stream-status line and the assist card: `12 results · 140 ms ·
  cached`. Assist stays reachable in AI mode. The result count keeps reading
  `engines_used[].result_count` (the merge-collapse count difference stays as it is).
- Acceptance: one meta line renders with the count, the elapsed time and the cache state;
  the cache-details expander, the stream-status line and the assist card are gone from the
  results page; the row count and the meta count agree in the collapsed case.
- Follow-up: UX-08.

### UX-06 Minimal nav — left group, waffle menu, mobile sheet

- Issue #311 · Effort M · Label cosmetic · Team UI · Branch `v3/ux-06-minimal-nav`
- Depends on: UX-03
- Do: the header becomes a minimal bar. Left: logo (home) · Search · Answer · History.
  Right: theme toggle · waffle menu (phosphor `squares-four` — the existing icon library,
  no new dependency). The waffle menu holds Dashboard, Archive, Engines, Cache, Audit,
  Settings and Docs, gated by `capabilities.flags.adminSurface` and `routeVisible` as
  today. At <700px the menu becomes a full-screen sheet. The old `more` dropdown and the
  `.nav-operator` / 700px collapse rules go away.
- Acceptance: the header shows exactly four left items and two right controls at ≥700px;
  the waffle menu lists every gated entry and each one navigates; the operator entries
  are hidden for a non-admin role; at 390px the menu opens as a full-screen sheet and the
  left group still fits.
- Follow-up: UX-08.

### UX-07 Threads in History

- Issue #312 · Effort S · Label feature · Team UI · Branch `v3/ux-07-history-threads`
- Depends on: UX-02
- Do: a `Conversations` section at the top of `/app/history` lists the user's threads
  (title, last activity, turn count) from `GET /api/threads`; each row resumes the thread
  at `/app/answer?thread=<id>`; each row can be deleted (`DELETE /api/threads/{id}`). The
  existing search/click/answer-run feed stays below it.
- Acceptance: the section lists the user's threads newest-first; a row resumes the
  conversation; delete removes it from the list and from the store; a non-owner sees
  nothing.
- Follow-up: UX-09.

### UX-08 Landing parity audit (no landing changes)

- Issue #313 · Effort S · Label documentation · Team UI · Branch `v3/ux-08-landing-audit`
- Depends on: UX-03, UX-06
- Do: audit-only. Produce a short note (in the issue, not the repo) answering: does the
  cauce.fyi landing still render after the nav restructure (it uses the same app shell);
  does the landing's own header/nav need the same waffle treatment to stay consistent, or
  is it deliberately independent; what it takes to preview the landing locally. **No
  landing changes land in this wave** — the user reviews the note first. Make the landing
  locally previewable only if that is a build-flag change with no effect on the shipped
  binary (the flag must stay a build-time flag: the Pages deploy renders the landing even
  when the backend is unreachable).
- Acceptance: the note answers the three questions with file references; the Pages build
  still sets `VITE_CAUCE_LANDING=1` and the binary build still does not; the landing page
  on cauce.fyi is visually unchanged after the nav work.
- Follow-up: a separate landing issue, created from the note.

### UX-09 Release v0.9.0

- Issue #314 · Effort S · Label infra · Team Core · Branch `v3/ux-09-release-0.9.0`
- Depends on: UX-01..UX-08
- Do: bump the workspace version to 0.9.0, tag, update the changelog and the docs
  (`docs/`) for the new thread model and the new UI, write the dated memory note in
  `.agents/MEMORY.md`, and refresh the `testing-cauce-serve` skill with the new thread
  endpoints and the landing-preview recipe.
- Acceptance: `mise run validate` green on the release commit; the release builds from a
  clean checkout; the docs describe the thread model; the skill file mentions the new
  endpoints.
- Follow-up: the landing issue from UX-08.
