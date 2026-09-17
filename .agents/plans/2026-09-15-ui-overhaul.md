# oxe UI Overhaul — daisyUI compliance, layout, AI reliability, motion

Date: 2026-09-15
Status: approved (research-backed; from research agents R1-R4, this session)
Blocks release re-tag until all blockers land.

## Problem statements (owner)

1. BLOCKER: daisyUI barely used despite being a dep; ui/AGENTS.md mandates
   tokens/components only. Settings dialog = divs; Save doesn't close it.
2. `<Header>` replicated per route instead of layout approach; evaluate
   Astro/Vike vs custom.
3. BLOCKER: AI mode appears "hard-coded" (greeting answers).
4. HARD STOP: nothing is animated; need motion + explicit state machines +
   error boundaries/pages/toasts.

## Decisions

### D1. Layout: custom wrapper, NO Astro/Vike (R2)
Research verdict: custom preact-iso layout = 25/25 fit, Vike 15/25 (11-20KB gz
runtime, no official preact support), Astro 13/25 (MPA model fights our SPA +
FastAPI static serving). Implementation:
- `ui/src/main.tsx`: hoist `<Header/>` above `<Router>` (persistent, one
  useAiAvailable fetch per session instead of per navigation).
- New `ui/src/components/Layout.tsx` (~20 lines) with variants home/app.
- Routes drop their own Header imports. URL contract untouched.

### D2. daisyUI migration (R1 audit)
Ordered:
1. SettingsDialog -> `<dialog class="modal">` + `<form method="dialog">`;
   save path closes dialog (fixes the bug); `?settings=open` deep link still
   works via showModal + close event -> strip param. (M)
2. Errors -> `alert alert-error/success/warning` (4 sites). (S)
3. AboutHint -> `dropdown dropdown-end`. (S)
4. Skeletons for search results + AI answer (daisyUI skeleton + shimmer). (S/M)
5. title= attrs -> daisyUI tooltip (2 sites). (S)
6. Optional: pager arrows -> join. (skip by default)
Keep custom (justified per ui/AGENTS.md): ModelPicker combobox, SuggestionsDropdown,
ResultCard anatomy, search pill.

### D3. AI reliability (R3 root cause)
Root cause: `openrouter/free` alias routes to random tool-less free models; a
greeting answer with confidence 0 got cached for 24h (server caches any done
without error). Pipeline itself is correct.
Fixes:
1. config.toml: model = "{env.OXE_AI_MODEL_ID}" (pinned real model from repo
   .env), api_key/base_url via {env.*} too.
2. oxe/server.py answer caching: require confidence >= 4 to cache.
3. oxe/config.py: .env discovery = cwd, then $OXE_CONFIG_DIR/.env; never
   flatten {env.*} to plaintext on save (skip save or keep interpolation when
   source used it).
4. Wipe poisoned answer cache rows.
5. Also: allow clearing answer cache selectively or via /cache/invalidate.

### D4. Motion + state + errors (R4)
Stack (morphic-style pure CSS, ~2KB total):
- Port morphic's enter/exit `@utility animate-in/animate-out` + single
  enter/exit keyframes into ui/src/index.css; reduced-motion identity override.
- Shimmer skeleton (voy pattern), source-card stagger via animation-delay
  calc, empty-state entrances, toast slide-in.
- `preact-transitioning` (~1KB) only if exit animations needed (toasts,
  dialog close).
- Optional (M, later): View Transitions API for route/theme crossfade with
  fallback.
State:
- AnswerState -> discriminated `status: idle|streaming|done|error|stopped`
  union; useSearch gets explicit status field. No xstate.
Errors/toasts:
- ~20-line class ErrorBoundary + routes/error.tsx page.
- daisyUI toast stack (signal-driven array): settings save error, test
  connection result, cache-refreshed confirm. Search/AI errors stay inline
  alerts (persistent, actionable).

## Implementation waves

- W1 (backend, oxe/): D3 items 2,3,5. Tests.
- W2 (ui layout+dialog): D1 + D2.1-2. Tests, budgets.
- W3 (ui motion+state+errors): D4. Tests, budgets.
- W4 (config+cleanup): D3.1,4; final validator round (read-only).

## Gates
pytest green (93 baseline), bun lint/check/test green, JS <= 40KB gz
(target <= 24), CSS <= 30KB gz, console errors 0, both themes 390+1280.
