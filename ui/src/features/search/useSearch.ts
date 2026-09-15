import { useRef, useState } from "preact/hooks";
import type { SearchResponse, SearchResult } from "../../lib/api";
import { search } from "../../lib/api";

export interface SearchState {
  payload: SearchResponse | null;
  loading: boolean;
  error: string | null;
}

const PAGE_SIZE = 10;

/** Fetch classic results for a query. SearchBox stays fetch-free;
 * the route calls `run` on query/page changes. */
export function useSearch(): { state: SearchState; run: (q: string, p: number) => void } {
  const [state, setState] = useState<SearchState>({ payload: null, loading: false, error: null });
  const abortRef = useRef<AbortController | null>(null);

  const run = (q: string) => {
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

  return { state, run };
}
export function metaLine(
  payload: SearchResponse | null,
  ageS: number | null,
  ttlLeftS: number | null,
): string {
  if (!payload) return "";
  const n = payload.results.length;
  const bits = [`${n} result${n === 1 ? "" : "s"}`];
  if (payload._source) bits.push(`from ${payload._source}`);
  if (ageS) bits.push(`${ageS < 60 ? "just now" : fmtDur(ageS)} old`);
  if (ttlLeftS != null) bits.push(`ttl ${ttlLeftS <= 0 ? "expired" : fmtDur(ttlLeftS) + " left"}`);
  return bits.join(" - ");
}

function fmtDur(s: number): string {
  if (s < 60) return `${Math.floor(s)}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

export type { SearchResult };
