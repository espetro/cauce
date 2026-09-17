import { Trans } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import * as v from 'valibot'
import { forcedStateSchema, suggestSchema } from '../lib/fixtures.ts'
import { modeSchema, settingsSchema } from '../lib/routeSearch.ts'

/**
 * `/search` is the one route for both Search and AI mode (search.md: "One screen, two modes,
 * one url scheme"). `q` is the canonical shareable query; `mode=ai` switches to AI mode;
 * `force`/`suggest` are the QA/dev fixtures from `src/lib/fixtures.ts` (day-0 gate 6,
 * checkpoints 6/7/8/13/14) wired in here since this is the one route they're designed for;
 * `settings=open` is the settings-dialog overlay param (checkpoint 18), shared with every
 * other route via `routeSearch.ts`.
 *
 * Placeholder only: this foundation task stands up the routing skeleton (route resolves,
 * params validate) -- results rendering, streaming AI, continuous scroll etc. are screen-task
 * work (search.md).
 */
const searchSearchSchema = v.object({
  q: v.optional(v.string(), ''),
  mode: modeSchema,
  force: forcedStateSchema,
  suggest: suggestSchema,
  settings: settingsSchema,
})

export const Route = createFileRoute('/search')({
  validateSearch: searchSearchSchema,
  component: SearchComponent,
})

function SearchComponent() {
  const { q } = Route.useSearch()
  return (
    <main className="mx-auto max-w-2xl px-4 py-8">
      <h1 className="text-xl font-semibold">
        <Trans>Search results</Trans>
      </h1>
      {q ? <p className="text-sm text-base-content/60">{q}</p> : null}
    </main>
  )
}
