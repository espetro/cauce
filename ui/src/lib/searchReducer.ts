/**
 * Rung-3 FSM for the /search results screen (ui/AGENTS.md state ladder): discriminated-union
 * state, pure reducer, exhaustive `never` check, unit-tested with no renderer.
 *
 * Page 1 comes from the router `loader()` (rung 2, server state) and seeds this machine via
 * `loaded`. Further pages are the continuous-scroll appends (search.md: "pagination is not
 * URL-addressable; continuous scroll owns it"), so pages 2+ live here, not in the URL.
 * The backend caps pagination at 10 pages.
 */
import type { components } from './types.gen.ts'

type SearchResult = components['schemas']['SearchResult']
type SearxResponse = components['schemas']['SearxResponse']

/** Backend caps continuous scroll at 10 pages (search.md, "Continuous scroll"). */
export const MAX_PAGES = 10

export interface ReadyState {
  status: 'ready'
  results: SearchResult[]
  pageno: number
  hasMore: boolean
  /** True between `moreRequested` and its `moreLoaded`/`moreFailed`. */
  loadingMore: boolean
  numberOfResults: number
  suggestions: string[]
  /** Exa-shaped payload captured verbatim for the `copy json` share action. */
  payload: SearxResponse
}

export type SearchState =
  | { status: 'idle' }
  | { status: 'loading' }
  | ReadyState
  | { status: 'empty' }
  | { status: 'error'; message: string }

export type SearchEvent =
  | { type: 'loaded'; response: SearxResponse }
  | { type: 'failed'; message: string }
  | { type: 'moreRequested' }
  | { type: 'moreLoaded'; response: SearxResponse }
  | { type: 'moreFailed'; message: string }

function hasMore(pageno: number, results: SearchResult[]): boolean {
  return results.length > 0 && pageno < MAX_PAGES
}

export function seedSearchState(response: SearxResponse | { loadError: string } | null): SearchState {
  if (!response) {
    return { status: 'idle' }
  }
  if ('loadError' in response) {
    return searchReducer({ status: 'idle' }, { type: 'failed', message: response.loadError })
  }
  return searchReducer({ status: 'idle' }, { type: 'loaded', response })
}

export function searchReducer(state: SearchState, event: SearchEvent): SearchState {
  switch (event.type) {
    case 'loaded': {
      const results = event.response.results ?? []
      if (results.length === 0) {
        return { status: 'empty' }
      }
      return {
        status: 'ready',
        results,
        pageno: 1,
        hasMore: hasMore(1, results),
        loadingMore: false,
        numberOfResults: event.response.number_of_results,
        suggestions: event.response.suggestions ?? [],
        payload: event.response,
      }
    }
    case 'failed':
      return { status: 'error', message: event.message }
    case 'moreRequested':
      return state.status === 'ready' && state.hasMore
        ? { ...state, hasMore: false, loadingMore: true }
        : state
    case 'moreLoaded': {
      if (state.status !== 'ready') {
        return state
      }
      const results = [...state.results, ...(event.response.results ?? [])]
      const pageno = state.pageno + 1
      return { ...state, results, pageno, loadingMore: false, hasMore: hasMore(pageno, results) }
    }
    case 'moreFailed':
      // A failed next-page fetch stops auto-loading with an inline retry (search.md);
      // already-loaded results stay untouched.
      return state.status === 'ready'
        ? { ...state, loadingMore: false, hasMore: state.pageno < MAX_PAGES }
        : state
    default: {
      const exhaustive: never = event
      return exhaustive
    }
  }
}
