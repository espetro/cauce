import { useCallback, useRef, useState } from "preact/hooks";
import { deleteCacheRow, search } from "../../lib/api";
import { search_meta_results } from "../../lib/i18n";
import type { SearchResponse, SearchResult } from "../../lib/schemas";

export type SearchStatus = "idle" | "loading" | "refreshing" | "success" | "empty" | "error";
export type ErrorKind = "rate_limited" | "timeout" | "backend_error";

export interface SearchError {
  message: string;
  kind?: ErrorKind;
}

export interface SearchState {
  /** first-page payload: requestId / cache meta / copy-json source */
  payload: SearchResponse | null;
  /** accumulated results across all loaded pages */
  results: SearchResult[];
  status: SearchStatus;
  /** page-1 load or refresh in flight */
  loading: boolean;
  error: SearchError | null;
  /** last successfully fetched page */
  page: number;
  /** more pages may exist (backend caps at MAX_PAGES) */
  hasNext: boolean;
  /** a next page fetch is in flight */
  loadingMore: boolean;
  /** the last next-page fetch failed; stops infinite loading until retry */
  moreError: boolean;
}

/** Backend hard cap on ?page=N. */
export const MAX_PAGES = 10;
const PAGE_SIZE = 10;

/** Cache-hit when the server served the payload from the SQLite TTL cache. */
export function isCacheHit(payload: SearchResponse | null): boolean {
  return payload?._source === "cache";
}

/** Age (seconds) of a cached payload, from the server-injected `_cached_at`.
 * Returns null when absent or in the future (clock skew). */
export function cachedAgeOf(
  payload: SearchResponse | null,
  now = Date.now() / 1000,
): number | null {
  const at = payload?._cached_at;
  if (typeof at !== "number" || at <= 0) return null;
  const age = Math.floor(now - at);
  return age >= 0 ? age : null;
}

const initial = (): SearchState => ({
  payload: null,
  results: [],
  status: "idle",
  loading: false,
  error: null,
  page: 0,
  hasNext: false,
  loadingMore: false,
  moreError: false,
});

/** Continuous scroll: `run` loads page 1, `loadMore` appends the next page
 * as the user nears the end. `refresh` bypasses the cache by deleting the
 * row first (POST /row/{key}/delete) then re-searching. The `p` URL param
 * is deprecated: deep links still resolve but page state is not restored. */
export function useSearch(): {
  state: SearchState;
  run: (q: string) => void;
  loadMore: (q: string) => void;
  refresh: (q: string) => void;
} {
  const [state, setState] = useState<SearchState>(initial);
  const abortRef = useRef<AbortController | null>(null);
  const reqIdRef = useRef(0);
  const qRef = useRef("");

  const fetchPage = (q: string, p: number, mode: "initial" | "more" | "refresh") => {
    const reqId = ++reqIdRef.current;
    qRef.current = q;
    abortRef.current?.abort();
    const ctl = new AbortController();
    abortRef.current = ctl;
    if (mode !== "more") {
      setState((s) => ({
        ...s,
        status: mode === "refresh" ? "refreshing" : "loading",
        loading: true,
        error: null,
      }));
    } else {
      setState((s) => ({ ...s, loadingMore: true, moreError: false }));
    }
    search({ query: q, numResults: PAGE_SIZE, page: p }, ctl.signal)
      .then((payload: SearchResponse) => {
        if (reqId !== reqIdRef.current) return;
        if (mode === "more") {
          setState((s) => {
            // duplicate-page / stale-race guard: same query only
            if (qRef.current !== q) return s;
            const seen = new Set(s.results.map((r) => r.id || r.url));
            const fresh = payload.results.filter((r) => !seen.has(r.id || r.url));
            return {
              ...s,
              results: [...s.results, ...fresh],
              page: p,
              hasNext: payload.results.length > 0 && p < MAX_PAGES,
              loadingMore: false,
              moreError: false,
            };
          });
        } else {
          setState({
            payload,
            results: payload.results,
            status: nextStatus(payload),
            loading: false,
            error: nextError(payload),
            page: 1,
            hasNext: payload.results.length > 0 && 1 < MAX_PAGES,
            loadingMore: false,
            moreError: false,
          });
        }
      })
      .catch((e: unknown) => {
        if (reqId !== reqIdRef.current) return;
        if ((e as Error)?.name === "AbortError") return;
        const message = (e as Error)?.message ?? "search failed";
        if (mode === "more") {
          // keep the loaded results; page-level `error` stays untouched (the
          // full error block must not replace/overlay accumulated results);
          // stop infinite loading until the inline retry
          setState((s) => ({
            ...s,
            loadingMore: false,
            moreError: true,
          }));
        } else {
          setState((s) => ({
            ...s,
            status: "error",
            loading: false,
            error: { message },
          }));
        }
      });
  };

  const run = (q: string) => {
    setState(initial());
    fetchPage(q, 1, "initial");
  };

  const loadMore = useCallback(
    (q: string) => {
      if (qRef.current !== q) return;
      if (!state.hasNext || state.loadingMore) return;
      // a failed next-page fetch stops auto-loading; retry via loadMore
      fetchPage(q, state.page + 1, "more");
    },
    [state.hasNext, state.loadingMore, state.moreError, state.page],
  );

  const refresh = (q: string) => {
    // Delete the cache entry first so the re-search hits the network;
    // the old hash is unknown client-side beyond `_q_hash`, so delete
    // by the current payload's hash, then fall back to a plain search.
    const key = state.payload?._q_hash;
    if (key) {
      deleteCacheRow(key)
        .catch(() => undefined)
        .then(() => {
          // a newer search may have started while the delete was in
          // flight; don't supersede it with a stale fetch
          if (qRef.current !== q) return;
          fetchPage(q, 1, "refresh");
        });
    } else {
      fetchPage(q, 1, "refresh");
    }
  };

  return { state, run, loadMore, refresh };
}

export function metaLine(payload: SearchResponse | null, total: number): string {
  if (!payload && total === 0) return "";
  return search_meta_results({ n: total });
}

/** Terminal status derived from a successful search response: an empty
 * page carrying `_error` is an error, not a clean empty. */
export function nextStatus(payload: SearchResponse): "success" | "empty" | "error" {
  if (payload.results.length === 0 && payload._error) return "error";
  return payload.results.length === 0 ? "empty" : "success";
}

export function nextError(payload: SearchResponse): SearchError | null {
  if (!payload._error) return null;
  const kind = payload._error_kind;
  return {
    message: payload._error,
    kind:
      kind === "rate_limited" || kind === "timeout" || kind === "backend_error" ? kind : undefined,
  };
}

export type { SearchResult };
