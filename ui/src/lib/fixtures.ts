/**
 * QA/dev-only fixture parsing for `force=*` and `suggest=1` URL params.
 *
 * Context: `.agents/docs/screens/userflow-checkpoints.md` checkpoints 6, 7, 8, 13 and 14
 * depend on these params to deterministically reach states (zero results, backend error,
 * suggestions dropdown open) without killing the backend by hand. Day-0 gate 6 in
 * `.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md` requires them to land in the fetch
 * layer before any real screen exists.
 *
 * Production safety: these params MUST be inert outside dev. `fixturesEnabled()` is the
 * single choke point that decides this — it is gated on `import.meta.env.DEV`, Vite's
 * standard dev-vs-prod flag (`false` in any `vite build` output, `true` under `vite dev`).
 * No env var opt-in is offered on top of it: `import.meta.env.DEV` already answers "is this
 * a production build" correctly and adding a second flag would just be another thing that
 * could drift out of sync with it. Every fixture consumer must call `fixturesEnabled()` (or
 * go through `parseFixtureParams`, which already does) rather than reading
 * `import.meta.env.DEV` directly, so there is exactly one place this policy lives.
 *
 * URL parsing lives in exactly one place (this module) per the plan's Layer 2 rule: URL
 * search params are one of the three places that get a valibot schema. `forcedStateSchema`
 * is built standalone here (not yet wired into a route's `validateSearch`) because no search
 * route exists yet (Wave 3 builds it) — a future route's `validateSearch` should import
 * `forcedStateSchema` / `parseFixtureParams` from here rather than re-parsing `force`/
 * `suggest` ad hoc.
 */
import * as v from 'valibot'

/**
 * Recognized values for the `force` QA fixture param.
 *
 * - `'error'` — the search call should behave as if the backend errored (reject/throw).
 * - `'empty'` — the search call should return a canned zero-results `SearxResponse` instead
 *   of hitting the network.
 * - `'ai-off'` — AI itself does not exist until Wave 4; this value is recognized and parsed
 *   today so the URL contract is stable, but nothing consumes it yet. Wave 4 will branch on
 *   it to render the "AI mode is not configured" notice (checkpoint 12) without a real AI
 *   backend.
 * - `null` — no forced state; normal network behavior.
 */
export type ForcedState = 'error' | 'ai-off' | 'empty' | null

const FORCED_STATE_VALUES = ['error', 'ai-off', 'empty'] as const

/** Standalone valibot schema for the `force` param, per Layer 2's "URL search params" rule. */
export const forcedStateSchema = v.pipe(
  v.optional(v.union([v.picklist(FORCED_STATE_VALUES), v.null_(), v.undefined_()])),
  v.transform((value): ForcedState => value ?? null),
)

/** Standalone valibot schema for the `suggest` param (`suggest=1` forces the dropdown open). */
export const suggestSchema = v.pipe(
  v.optional(v.string()),
  v.transform((value) => value === '1'),
)

export interface FixtureParams {
  force: ForcedState
  suggest: boolean
}

/**
 * Whether fixture params should have any effect at all. `false` in production builds
 * (`import.meta.env.DEV === false`), so a leaked `?force=error` in a shared/bookmarked
 * production URL is a no-op rather than a real outage simulation.
 */
export function fixturesEnabled(): boolean {
  return import.meta.env.DEV
}

/**
 * Parse `force` and `suggest` out of a `URLSearchParams`. Returns the inert defaults
 * (`{ force: null, suggest: false }`) outside dev, regardless of what the URL contains — see
 * `fixturesEnabled()`.
 *
 * This is the one place `force`/`suggest` get parsed; every future screen/component should
 * call this (or a route's `validateSearch`, once one wires this schema in) instead of
 * re-parsing `force`/`suggest` itself.
 */
export function parseFixtureParams(search: URLSearchParams): FixtureParams {
  if (!fixturesEnabled()) {
    return { force: null, suggest: false }
  }

  const rawForce = search.get('force')
  const force = v.parse(forcedStateSchema, rawForce ?? undefined)
  const suggest = v.parse(suggestSchema, search.get('suggest') ?? undefined)

  return { force, suggest }
}
