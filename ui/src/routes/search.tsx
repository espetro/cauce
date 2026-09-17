/**
 * The /search results screen (search.md, checkpoints 3-8): search box + results list,
 * meta line with the clickable `cached` badge, copy-link/copy-json share row, continuous
 * scroll with a `more results` fallback button, plus the force=empty / force=error
 * determinism fixtures (dev only, inert in production via `fixturesEnabled()`).
 *
 * State ladder: page 1 comes from the router `loader()` (rung 2, server state); pages 2+
 * live in the rung-3 `searchReducer` FSM; the mode / force / suggest params live in the
 * URL (rung 1) via `validateSearch`. All copy goes through Lingui.
 */
import IconBraces from '~icons/lucide/braces'
import IconCopy from '~icons/lucide/copy'
import IconLoaderCircle from '~icons/lucide/loader-circle'
import IconRotateCcw from '~icons/lucide/rotate-ccw'
import IconSearchX from '~icons/lucide/search-x'
import { Trans } from '@lingui/react/macro'
import { createFileRoute } from '@tanstack/react-router'
import { useReducer } from 'react'
import * as v from 'valibot'
import { SearchBox } from '../components/SearchBox.tsx'
import { search } from '../lib/api.ts'
import { forcedStateSchema, suggestSchema } from '../lib/fixtures.ts'
import { errorMessage, useContinuousScroll } from '../lib/effects/continuousScroll.ts'
import { modeSchema, settingsSchema } from '../lib/routeSearch.ts'
import type { components } from '../lib/types.gen.ts'
import { searchReducer, seedSearchState } from '../lib/searchReducer.ts'

type SearchResult = components['schemas']['SearchResult']

const searchSearchSchema = v.object({
  q: v.optional(v.string(), ''),
  mode: modeSchema,
  force: forcedStateSchema,
  suggest: suggestSchema,
  settings: settingsSchema,
})

export const Route = createFileRoute('/search')({
  validateSearch: searchSearchSchema,
  loaderDeps: ({ search: s }) => ({ q: s.q, force: s.force }),
  loader: async ({ deps }) => {
    if (!deps.q) {
      return null
    }
    const forceParams = deps.force ? `&force=${deps.force}` : ''
    return await search({ q: deps.q }, new URLSearchParams(`q=${encodeURIComponent(deps.q)}${forceParams}`))
  },
  component: SearchComponent,
  pendingComponent: SearchPending,
  errorComponent: SearchErrorScreen,
})

function SearchPending() {
  return (
    <main className="mx-auto max-w-2xl px-4 py-8" aria-busy="true">
      <div className="flex items-center gap-2 text-base-content/60" data-testid="search-loading">
        <IconLoaderCircle className="animate-spin" aria-hidden="true" />
        <p>
          <Trans>Searching</Trans>…
        </p>
      </div>
    </main>
  )
}

function SearchErrorScreen({ error }: { error: unknown }) {
  return (
    <main className="mx-auto max-w-2xl px-4 py-8" data-testid="search-error">
      <p role="alert" className="text-error">
        <Trans>error: search failed:</Trans> {errorMessage(error)}
      </p>
      <div className="mt-3 flex gap-2">
        <a className="btn btn-sm" href="">
          <IconRotateCcw aria-hidden="true" /> <Trans>retry</Trans>
        </a>
      </div>
    </main>
  )
}

function SearchComponent() {
  const { q } = Route.useSearch()
  const loaderData = Route.useLoaderData()
  const [state, dispatch] = useReducer(searchReducer, loaderData, seedSearchState)
  useContinuousScroll({ state, dispatch, query: q })

  const copyLink = () => {
    void navigator.clipboard.writeText(window.location.href)
  }

  return (
    <main className="mx-auto max-w-2xl px-4 py-8">
      <div className="mb-6">
        <SearchBox initialQuery={q} />
      </div>
      {state.status === 'empty' ? <EmptySurface /> : null}
      {state.status === 'error' ? <ErrorSurface message={state.message} /> : null}
      {state.status === 'ready' ? (
        <>
          <div className="flex items-center gap-2 text-sm text-base-content/60">
            <span data-testid="results-meta">
              <Trans comment="results count">{`${state.results.length} results`}</Trans>
            </span>
            <button type="button" className="btn btn-xs" onClick={copyLink}>
              <Trans>cached</Trans>
            </button>
            <span className="flex-1" />
            <button type="button" className="btn btn-ghost btn-xs" onClick={copyLink}>
              <IconCopy aria-hidden="true" /> <Trans>copy link</Trans>
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-xs"
              onClick={() => {
                void navigator.clipboard.writeText(JSON.stringify(state.payload))
              }}
            >
              <IconBraces aria-hidden="true" /> <Trans>copy json</Trans>
            </button>
          </div>
          <div className="mt-4">
            {state.results.map((result, index) => (
              <ResultRow key={`${result.url}-${index}`} result={result} />
            ))}
          </div>
          <ContinuousScrollFooter state={state} dispatch={dispatch} />
        </>
      ) : null}
    </main>
  )
}

function EmptySurface() {
  return (
    <div className="py-8" data-testid="search-empty">
      <p className="flex items-center gap-2">
        <IconSearchX aria-hidden="true" />
        <Trans>no results</Trans>
      </p>
    </div>
  )
}

function ErrorSurface({ message }: { message: string }) {
  return (
    <div className="py-8" data-testid="search-error">
      <p role="alert" className="text-error">
        <Trans>error: search failed:</Trans> {message}
      </p>
      <a className="btn btn-sm mt-3" href="">
        <IconRotateCcw aria-hidden="true" /> <Trans>retry</Trans>
      </a>
    </div>
  )
}

function ContinuousScrollFooter({
  state,
  dispatch,
}: {
  state: { pageno: number; hasMore: boolean; loadingMore: boolean }
  dispatch: (event: { type: 'moreRequested' }) => void
}) {
  if (state.hasMore || state.loadingMore) {
    return (
      <div className="py-4 text-center" data-testid="scroll-sentinel">
        {state.loadingMore ? (
          <p className="flex items-center justify-center gap-2 text-sm text-base-content/60" data-testid="loading-more">
            <IconLoaderCircle className="animate-spin" aria-hidden="true" />
            <Trans>loading more…</Trans>
          </p>
        ) : (
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            data-testid="more-results"
            onClick={() => {
              dispatch({ type: 'moreRequested' })
            }}
          >
            <Trans>more results</Trans>
          </button>
        )}
      </div>
    )
  }
  return (
    <p className="py-4 text-center text-sm text-base-content/60" data-testid="end-of-results">
      <Trans>end of results</Trans>
    </p>
  )
}

function ResultRow({ result }: { result: SearchResult }) {
  const domain = new URL(result.url).hostname
  const favicon = `https://icons.duckduckgo.com/ip3/${domain}.ico`
  return (
    <article className="py-3">
      <p className="flex items-center gap-2 text-sm text-base-content/60">
        <img src={favicon} alt="" width={16} height={16} loading="lazy" className="inline-block" />
        {domain}
      </p>
      <h2 className="text-lg leading-snug">
        <a href={result.url} target="_blank" rel="noreferrer" className="link link-primary">
          {result.title}
        </a>
      </h2>
      {result.content ? <p className="line-clamp-2 text-sm">{result.content}</p> : null}
    </article>
  )
}
