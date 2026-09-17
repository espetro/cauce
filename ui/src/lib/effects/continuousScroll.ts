/**
 * Continuous-scroll side effects for /search, in the one directory where `useEffect` is
 * legal (ui/AGENTS.md Enforced rule). Everything the effect decides is expressed as reducer
 * events; all fetch knowledge stays in `src/lib/api.ts` (`search()`).
 *
 * pageno deliberately stays out of the URL (checkpoint 4: "pagination is not
 * URL-addressable"). The sentinel div rendered by the route sits near the end of the
 * loaded list; when it intersects (i.e. the user has scrolled near the end) and the
 * machine allows more pages, a `moreRequested` -> fetch -> `moreLoaded`/`moreFailed`
 * sequence drives the rung-3 FSM. The `more results` button remains as the
 * keyboard/no-scroll fallback and dispatches the same event.
 */
import { useEffect, useRef } from 'react'
import { search } from '../api.ts'
import type { SearchEvent, SearchState } from '../searchReducer.ts'

export interface ContinuousScrollProps {
  state: SearchState
  dispatch: (event: SearchEvent) => void
  query: string
}

export function useContinuousScroll({ state, dispatch, query }: ContinuousScrollProps): void {
  const sentinelRef = useRef<HTMLDivElement | null>(null)
  const inFlightRef = useRef(false)

  // A new query resets any in-flight append guard from the previous query.
  useEffect(() => {
    inFlightRef.current = false
  }, [query])

  useEffect(() => {
    if (state.status !== 'ready' || !state.hasMore) {
      return
    }
    const sentinel = sentinelRef.current
    if (!sentinel) {
      return
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting) || inFlightRef.current) {
          return
        }
        inFlightRef.current = true
        dispatch({ type: 'moreRequested' })
        search({ q: query, pageno: state.pageno + 1 })
          .then((response) => {
            dispatch({ type: 'moreLoaded', response })
          })
          .catch((error: unknown) => {
            dispatch({ type: 'moreFailed', message: errorMessage(error) })
          })
          .finally(() => {
            inFlightRef.current = false
          })
      },
      { rootMargin: '480px 0px' },
    )
    observer.observe(sentinel)
    return () => {
      observer.disconnect()
    }
  }, [state, dispatch, query])
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}
