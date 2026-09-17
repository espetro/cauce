/**
 * Search-param helper for the settings overlay (checkpoint 18): the updater function passed
 * to `navigate({ search })` that strips `settings` from the url while preserving every other
 * param (e.g. `?q=x&mode=ai` on /search). Kept here so close/save behavior has one home and
 * a cheap unit test.
 */

/** Updater that removes the `settings` key from the current search params. */
export function stripSettings(prev: Record<string, unknown>): Record<string, unknown> {
  const { settings: _settings, ...rest } = prev
  return rest
}

