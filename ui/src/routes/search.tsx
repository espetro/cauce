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
import { createFileRoute, Link, useNavigate } from '@tanstack/react-router'
import { useReducer, type ReactNode } from 'react'
import * as v from 'valibot'
import { CitationPopover } from '../components/CitationPopover.tsx'
import { SearchBox } from '../components/SearchBox.tsx'
import { Shell } from '../components/Shell.tsx'
import { aiReducer, seedAiState } from '../lib/aiReducer.ts'
import { loadSearchPage } from '../lib/searchLoader.ts'
import { forcedStateSchema, suggestSchema } from '../lib/fixtures.ts'
import { errorMessage, useContinuousScroll } from '../lib/effects/continuousScroll.ts'
import { useAnswerStream } from '../lib/effects/answerStream.ts'
import { modeSchema, settingsSchema } from '../lib/routeSearch.ts'
import type { components } from '../lib/types.gen.ts'
import { searchReducer, seedSearchState } from '../lib/searchReducer.ts'

type AnswerSource = components['schemas']['AnswerSource']

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
  loaderDeps: ({ search: s }) => ({ q: s.q, mode: s.mode, force: s.force }),
  loader: ({ deps }) => loadSearchPage(deps),
  component: SearchComponent,
  pendingComponent: SearchPending,
  errorComponent: SearchErrorScreen,
})

function SearchPending() {
  return (
    <Shell size="sm" aria-busy="true">
      <div className="flex items-center gap-2 text-base-content/60" data-testid="search-loading">
        <IconLoaderCircle className="animate-spin" aria-hidden="true" />
        <p>
          <Trans>Searching</Trans>…
        </p>
      </div>
    </Shell>
  )
}

function SearchErrorScreen({ error }: { error: unknown }) {
  return (
    <Shell size="sm" data-testid="search-error">
      <p role="alert" className="text-error-strong">
        <Trans>error: search failed:</Trans> {errorMessage(error)}
      </p>
      <div className="mt-3 flex gap-2">
        <a className="btn btn-sm" href="">
          <IconRotateCcw aria-hidden="true" /> <Trans>retry</Trans>
        </a>
      </div>
    </Shell>
  )
}

function useModeChange(q: string) {
  const navigate = useNavigate()
  return (next: 'ai' | undefined, typed: string) => {
    const query = typed || q
    if (query.length > 0) {
      void navigate({ to: '/search', search: next ? { q: query, mode: next } : { q: query } })
    }
  }
}

function SearchComponent() {
  const { q, mode, force } = Route.useSearch()
  const loaderData = Route.useLoaderData()
  const [state, dispatch] = useReducer(searchReducer, loaderData, seedSearchState)
  useContinuousScroll({ state, dispatch, query: q })
  const onModeChange = useModeChange(q)

  // Classic mode only: force=error/empty here are the web-search fixtures
  // (checkpoints 7/8). In AI mode force is the /answer SSE fixture, so the loader
  // must not short-circuit the answer surface (checkpoints 13/14).
  const classicBody = <ClassicBody q={q} state={state} dispatch={dispatch} />

  if (mode === 'ai') {
    return (
      <AiAnswerSurface query={q} force={force ?? null} onModeChange={onModeChange} classicBody={classicBody} />
    )
  }

  return (
    <Shell size="sm">
      <h1 className="sr-only">{q}</h1>
      <div className="mb-6">
        <SearchBox initialQuery={q} onModeChange={onModeChange} />
      </div>
      {classicBody}
    </Shell>
  )
}

function ClassicBody({
  q,
  state,
  dispatch,
}: {
  q: string
  state: ReturnType<typeof seedSearchState>
  dispatch: (event: { type: 'moreRequested' }) => void
}) {
  const copyLink = () => {
    void navigator.clipboard.writeText(window.location.href)
  }

  return (
    <>
      {state.status === 'empty' ? <EmptySurface q={q} /> : null}
      {state.status === 'error' ? <ErrorSurface q={q} message={state.message} /> : null}
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
    </>
  )
}

function AskAiLink({ q }: { q: string }) {
  return (
    <Link className="btn btn-ghost btn-sm" to="/search" search={{ q, mode: 'ai' }}>
      <Trans>ask AI instead</Trans>
    </Link>
  )
}

function EmptySurface({ q }: { q: string }) {
  return (
    <div className="py-8" data-testid="search-empty">
      <p className="flex items-center gap-2">
        <IconSearchX aria-hidden="true" />
        <Trans>no results</Trans>
      </p>
      <div className="mt-3">
        <AskAiLink q={q} />
      </div>
    </div>
  )
}

function ErrorSurface({ q, message }: { q: string; message: string }) {
  return (
    <div className="py-8" data-testid="search-error">
      <p role="alert" className="text-error-strong">
        <Trans>error: search failed:</Trans> {message}
      </p>
      <div className="mt-3 flex gap-2">
        <a className="btn btn-sm" href="">
          <IconRotateCcw aria-hidden="true" /> <Trans>retry</Trans>
        </a>
        <AskAiLink q={q} />
      </div>
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

/**
 * AI answer surface (search.md mockups B/B2, checkpoints 9-14): the answer renders above a
 * horizontal row of cited source cards, with related questions and the confidence/cached
 * meta once done. Streaming state lives in the rung-3 `aiReducer`; the SSE transport is the
 * `useAnswerStream` effect. Force fixtures map to checkpoints 12 (`ai-off`), 13 (`error`)
 * and 14 (`empty`) without a live backend.
 */
function AiAnswerSurface({
  query,
  force,
  onModeChange,
  classicBody,
}: {
  query: string
  force: 'ai-off' | 'error' | 'empty' | null
  onModeChange: (next: 'ai' | undefined, typed: string) => void
  classicBody: ReactNode
}) {
  const [state, dispatch] = useReducer(aiReducer, force, seedAiState)
  useAnswerStream({ query, force, dispatch })

  return (
    <Shell size="sm" data-testid="ai-surface">
      <h1 className="sr-only">{query}</h1>
      <div className="mb-6">
        <SearchBox
          initialQuery={query}
          mode="ai"
          onModeChange={onModeChange}
          aiDisabled={state.status === 'unavailable'}
        />
      </div>
      {state.status === 'unavailable' ? (
        <>
          <AiUnavailableNotice />
          {classicBody}
        </>
      ) : null}
      {state.status === 'stepping' || state.status === 'streaming' ? (
        <AiStreaming text={state.status === 'streaming' ? state.text : ''} steps={state.steps} />
      ) : null}
      {state.status === 'failed' ? (
        <AiFailed query={query} message={state.message} partialText={state.partialText} />
      ) : null}
      {state.status === 'done' ? (
        <AiDone
          query={query}
          answer={state.answer}
          sources={state.sources}
          relatedQuestions={state.relatedQuestions}
          confidence={state.confidence}
          cached={state.cached}
        />
      ) : null}
    </Shell>
  )
}

function AnswerMeta({ cached, confidence }: { cached: boolean; confidence: number }) {
  return (
    <div className="mb-3 flex items-center gap-3 text-sm text-base-content/60" data-testid="ai-meta">
      <span>
        <Trans comment="answer section heading">answer</Trans>
      </span>
      {cached ? (
        <span className="badge badge-sm" data-testid="ai-cached">
          <Trans>from cache</Trans>
        </span>
      ) : null}
      <span data-testid="ai-confidence">
        <Trans comment="answer confidence percentage">confidence:</Trans> {String(confidence)}%
      </span>
    </div>
  )
}

function AiStreaming({ text, steps }: { text: string; steps: string[] }) {
  return (
    <section aria-busy="true">
      {steps.length > 0 ? (
        <p className="mb-2 text-sm text-base-content/60" data-testid="ai-steps">
          {steps.map((label) => (
            <span key={label} className="badge badge-ghost badge-sm mr-1">
              {label}
            </span>
          ))}
        </p>
      ) : null}
      <p className="whitespace-pre-wrap" data-testid="ai-streaming">
        {text}
        <span className="animate-pulse" aria-hidden="true">
          ▌
        </span>
      </p>
      <p className="mt-2 flex items-center gap-2 text-sm text-base-content/60">
        <IconLoaderCircle className="animate-spin" aria-hidden="true" />
        <Trans>streaming…</Trans>
      </p>
    </section>
  )
}

function AiFailed({ query, message, partialText }: { query: string; message: string; partialText: string }) {
  return (
    <section data-testid="ai-failed">
      {partialText ? <p className="mb-3 whitespace-pre-wrap opacity-70">{partialText}</p> : null}
      <p role="alert" className="text-error-strong">
        <Trans>stream interrupted:</Trans> {message}
      </p>
      <div className="mt-3 flex gap-2">
        <a className="btn btn-sm" href="">
          <IconRotateCcw aria-hidden="true" /> <Trans>retry</Trans>
        </a>
        <Link className="btn btn-ghost btn-sm" to="/search" search={{ q: query }} data-testid="ai-view-search">
          <Trans>view Search</Trans>
        </Link>
      </div>
    </section>
  )
}

function AiUnavailableNotice() {
  return (
    <div className="pb-6" data-testid="ai-unavailable">
      <p className="text-base-content/60" role="status">
        <Trans comment="shown when the AI backend is off">
          AI mode is not configured - set a model in settings
        </Trans>
      </p>
    </div>
  )
}

function AiEmptySources({ query }: { query: string }) {
  return (
    <div className="py-6" data-testid="ai-empty-sources">
      <p className="flex items-center gap-2">
        <IconSearchX aria-hidden="true" />
        <Trans>no sources found for this query - try fewer words, or</Trans>
      </p>
      <Link className="btn btn-ghost btn-sm mt-2" to="/search" search={{ q: query }}>
        <Trans>view Search</Trans>
      </Link>
    </div>
  )
}

function SourceCard({ source, index }: { source: AnswerSource; index: number }) {
  const domain = new URL(source.url).hostname
  return (
    <a
      href={source.url}
      target="_blank"
      rel="noreferrer"
      className="card bg-base-200 p-3 min-w-40 shrink-0 hover:shadow-md"
      data-testid="ai-source-card"
    >
      <p className="flex items-center gap-2 text-xs text-base-content/60">
        <span className="badge badge-xs">{String(index + 1)}</span>
        <img
          src={`https://icons.duckduckgo.com/ip3/${domain}.ico`}
          alt=""
          width={16}
          height={16}
          loading="lazy"
        />
        {domain}
      </p>
      <p className="mt-1 line-clamp-2 text-sm">{source.title}</p>
    </a>
  )
}

function AiDone({
  query,
  answer,
  sources,
  relatedQuestions,
  confidence,
  cached,
}: {
  query: string
  answer: string
  sources: AnswerSource[]
  relatedQuestions: string[]
  confidence: number
  cached: boolean
}) {
  return (
    <section data-testid="ai-done">
      <AnswerMeta cached={cached} confidence={confidence} />
      {answer ? (
        <p className="whitespace-pre-wrap leading-relaxed">{renderAnswerWithCitations(answer, sources)}</p>
      ) : null}
      {sources.length === 0 ? <AiEmptySources query={query} /> : null}
      {sources.length > 0 ? (
        <>
          <p className="mt-6 mb-2 text-sm font-semibold text-base-content/60">
            <Trans comment="sources section heading">sources</Trans>
          </p>
          <div className="flex gap-3 overflow-x-auto pb-2">
            {sources.map((source, index) => (
              <SourceCard key={source.url} source={source} index={index} />
            ))}
          </div>
        </>
      ) : null}
      {relatedQuestions.length > 0 ? (
        <>
          <p className="mt-6 mb-2 text-sm font-semibold text-base-content/60">
            <Trans comment="related questions section heading">related</Trans>
          </p>
          <ul className="list-disc pl-5" data-testid="ai-related">
            {relatedQuestions.map((question) => (
              <li key={question}>{question}</li>
            ))}
          </ul>
        </>
      ) : null}
    </section>
  )
}

/**
 * Render the answer text with `[n]` citation markers replaced by CitationPopover triggers
 * (search.md mockup B2: "citation markers [1] [2] are links" to the matching source card).
 */
function renderAnswerWithCitations(answer: string, sources: AnswerSource[]) {
  const parts = answer.split(/(\[\d+\])/g)
  return parts.map((part, i) => {
    const match = /^\[(\d+)\]$/.exec(part)
    if (!match) {
      return <span key={String(i)}>{part}</span>
    }
    const n = Number(match[1])
    const source = sources[n - 1]
    if (!source) {
      return <span key={String(i)}>{part}</span>
    }
    return <CitationPopover key={String(i)} index={n} source={source} />
  })
}
