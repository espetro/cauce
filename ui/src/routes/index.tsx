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
import IconSparkles from '~icons/lucide/sparkles'
import IconSearch from '~icons/lucide/search'
import { Trans, useLingui } from '@lingui/react/macro'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import * as v from 'valibot'
import { modeSchema, settingsSchema } from '../lib/routeSearch.ts'

const landingSearchSchema = v.object({
  mode: modeSchema,
  settings: settingsSchema,
})

export const Route = createFileRoute('/')({
  validateSearch: landingSearchSchema,
  component: HomeComponent,
})

function HomeComponent() {
  const { t } = useLingui()
  const { mode, settings } = Route.useSearch()
  const navigate = useNavigate()
  const aiMode = mode === 'ai'

  const submit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const q = String(new FormData(event.currentTarget).get('q') ?? '').trim()
    if (q.length === 0) {
      return
    }
    void navigate({ to: '/search', search: aiMode ? { q, mode: 'ai' } : { q } })
  }

  return (
    <main className="flex min-h-[calc(100vh-4rem)] flex-col items-center justify-center px-4">
      <h1 className="text-5xl font-semibold">oxe</h1>
      <p className="mt-3 text-sm text-base-content/60">
        <Trans comment="landing tagline">your local web intel layer</Trans>
      </p>
      <form
        onSubmit={submit}
        className="mt-10 flex h-14 w-full max-w-2xl items-center gap-2 rounded-full border border-base-300 bg-base-100 pl-6 pr-2 shadow-sm focus-within:border-primary transition-colors"
      >
        <input
          name="q"
          type="search"
          autoFocus
          aria-label={t`Search query`}
          placeholder={aiMode ? t`Ask anything privately` : t`Search privately`}
          className="flex-1 bg-transparent outline-none placeholder:text-base-content/50"
        />
        <div className="join rounded-full bg-base-200 p-1" role="radiogroup" aria-label={t`Mode`}>
          <button
            type="button"
            role="radio"
            aria-checked={!aiMode}
            className={`btn btn-xs join-item rounded-full ${!aiMode ? 'btn-neutral' : 'btn-ghost'}`}
            onClick={() => {
              void navigate({ to: '.', search: settings ? { settings } : {} })
            }}
          >
            <IconSearch aria-hidden="true" />
            <Trans>Search</Trans>
          </button>
          <button
            type="button"
            role="radio"
            aria-checked={aiMode}
            className={`btn btn-xs join-item rounded-full ${aiMode ? 'btn-neutral' : 'btn-ghost'}`}
            onClick={() => {
              void navigate({ to: '.', search: settings ? { mode: 'ai', settings } : { mode: 'ai' } })
            }}
          >
            <IconSparkles aria-hidden="true" />
            <Trans>AI</Trans>
          </button>
        </div>
        <button type="submit" className="btn btn-neutral btn-circle" aria-label={t`Search`}>
          <IconSearch aria-hidden="true" />
        </button>
      </form>
    </main>
  )
}
