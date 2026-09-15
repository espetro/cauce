import { useRef, useState } from "preact/hooks";
import type { SearchResponse, SearchResult } from "../../lib/api";
import { deleteCacheRow, search } from "../../lib/api";

export interface SearchState {
  payload: SearchResponse | null;
  loading: boolean;
  error: string | null;
}

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

/** Fetch classic results for a query. SearchBox stays fetch-free;
 * the route calls `run` on query/page changes. `refresh` bypasses the
 * cache by deleting the row first (POST /row/{key}/delete) then
 * re-searching; used by the cached badge click. */
export function useSearch(): {
  state: SearchState;
  run: (q: string, p?: number) => void;
  refresh: (q: string) => void;
} {
  const [state, setState] = useState<SearchState>({ payload: null, loading: false, error: null });
  const abortRef = useRef<AbortController | null>(null);

  const execute = (q: string) => {
    abortRef.current?.abort();
    const ctl = new AbortController();
    abortRef.current = ctl;
    setState((s: SearchState) => ({ ...s, loading: true, error: null }));
    search({ query: q, numResults: PAGE_SIZE }, ctl.signal)
      .then((payload: SearchResponse) => setState({ payload, loading: false, error: null }))
      .catch((e: unknown) => {
        if ((e as Error)?.name === "AbortError") return;
        setState((s: SearchState) => ({
          ...s,
          loading: false,
          error: (e as Error)?.message ?? "search failed",
        }));
      });
  };

  const run = (q: string) => {
    execute(q);
  };

  const refresh = (q: string) => {
    // Delete the cache entry first so the re-search hits the network;
    // the old hash is unknown client-side beyond `_q_hash`, so delete
    // by the current payload's hash, then fall back to a plain search.
    const key = state.payload?._q_hash;
    if (key) {
      deleteCacheRow(key)
        .catch(() => undefined)
        .finally(() => execute(q));
    } else {
      execute(q);
    }
  };

  return { state, run, refresh };
}

export function metaLine(payload: SearchResponse | null): string {
  if (!payload) return "";
  const n = payload.results.length;
  return `${n} result${n === 1 ? "" : "s"}`;
}

export type { SearchResult };
