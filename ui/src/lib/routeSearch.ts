/**
 * Shared valibot pieces for route `validateSearch` schemas (Layer 2, "URL search params" --
 * see `.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md`). Kept in one module so the same
 * param means the same thing on every route that carries it, per the plan's "single source
 * per fact" rule for the URL param contract.
 *
 * `force` / `suggest` are NOT here: those already have a canonical home in
 * `src/lib/fixtures.ts` (`forcedStateSchema`, `suggestSchema`) and routes should import them
 * from there directly.
 */
import * as v from 'valibot'

/**
 * `mode=ai` toggles AI mode on `/` and `/search` (checkpoint 2, search.md). Absence keeps the
 * url in Search mode -- per landing.md/search.md, "mode param absent keeps existing urls
 * shareable", so this is deliberately absent-vs-`'ai'`, not a `'search' | 'ai'` picklist with
 * a default, to avoid ever writing `?mode=search` into a url.
 */
export const modeSchema = v.optional(v.picklist(['ai']))

/**
 * `settings=open` opens the settings dialog as an overlay on top of the current route
 * (userflow-checkpoints.md checkpoint 18: "`/?settings=open` (also
 * `/search?q=x&settings=open` etc.)"). Not a route of its own -- every route that renders the
 * global header may carry this param. The dialog's real content is screen-task work; this
 * schema only reserves the param shape.
 */
export const settingsSchema = v.optional(v.picklist(['open']))
