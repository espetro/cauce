/**
 * Landing screen (landing.md): a quiet, centered hero -- wordmark, one-line tagline, and an
 * oversized pill search input with the Search / AI segmented toggle built into its right
 * end. No cards, no marketing copy. Submit navigates to `/search?q=...` (plus `mode=ai`
 * when AI mode is active; the param stays absent in Search mode to keep urls shareable).
 *
 * State rungs: the typed value is the uncontrolled input (rung 2, read via FormData);
 * `mode=ai` / `settings=open` live in the URL (rung 1) via `validateSearch`. The Search /
 * AI toggle is the spec's `role="radiogroup"` segmented control; selecting AI on landing
 * only arms the mode -- the morphed second row (model picker, reasoning chip) is search.md
 * surface work, not this screen's.
 */
import { Trans } from '@lingui/react/macro'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import * as v from 'valibot'
import { modeSchema, settingsSchema } from '../lib/routeSearch.ts'
import { SearchBox } from '../components/SearchBox.tsx'
import { Shell } from '../components/Shell.tsx'

const landingSearchSchema = v.object({
  mode: modeSchema,
  settings: settingsSchema,
})

export const Route = createFileRoute('/')({
  validateSearch: landingSearchSchema,
  component: HomeComponent,
})

function HomeComponent() {
  const { mode, settings } = Route.useSearch()
  const navigate = useNavigate()

  return (
    <Shell hero>
      <h1 className="text-5xl font-semibold">oxe</h1>
      <p className="mt-3 text-sm text-base-content/60">
        <Trans comment="landing tagline">your local web intel layer</Trans>
      </p>
      <SearchBox
        className="mt-10 max-w-2xl"
        mode={mode}
        autoFocus
        onModeChange={(next) => {
          const keep = settings ? { settings } : {}
          void navigate({ to: '.', search: next ? { mode: next, ...keep } : keep })
        }}
      />
    </Shell>
  )
}
