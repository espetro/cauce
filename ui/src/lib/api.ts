/** Typed fetch client for the oxe backend.
 * Endpoints: POST /search, POST /click, GET /suggest, GET /ac,
 * GET /api/history, POST /history/delete, GET /api/stats, GET /cache/stats. */

const BASE = "";

import { devLog, devTimed } from "./devlog";

export interface SearchRequest {
  query: string;
  numResults?: number;
  /** 1-based result page (backend support lands separately; sent when > 1). */
  page?: number;
  type?: string;
  includeDomains?: string[];
  excludeDomains?: string[];
  category?: string;
  contents?: { text?: boolean; highlights?: boolean };
}

export interface SearchResult {
  title: string;
  url: string;
  id?: string;
  text?: string;
  highlights?: string[];
  favicon?: string | null;
  publishedDate?: string | null;
  author?: string | null;
  image?: string | null;
}

export interface SearchResponse {
  requestId: string;
  searchType?: string;
  results: SearchResult[];
  costDollars?: { total: number };
  /** server-injected cache transparency fields */
  _source?: "cache" | "network";
  _q_hash?: string;
  _q?: string;
  _backend?: string;
  _duration_ms?: number | null;
  /** epoch seconds when the entry was cached (cache hits) */
  _cached_at?: number;
  /** backend-side failure surfaced in a 200 payload (empty results) */
  _error?: string;
  _error_kind?: "rate_limited" | "timeout" | "backend_error";
}

export interface SearchPageState {
  query: string;
  page: number;
}

export interface ClickPayload {
  query_hash: string;
  result_id: string;
  url: string;
  title: string;
  source?: string;
}

export async function search(req: SearchRequest, signal?: AbortSignal): Promise<SearchResponse> {
  const fetchIt = () =>
    fetch(`${BASE}/search`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify({
        numResults: 10,
        contents: { text: true, highlights: true },
        ...req,
        ...(req.page != null && req.page > 1 ? { page: req.page } : {}),
      }),
      signal,
    });
  const res = await devTimed("search", { q: req.query, page: req.page ?? 1 }, fetchIt);
  if (!res.ok) throw new Error(`search failed: ${res.status}`);
  const out = (await res.json()) as SearchResponse;
  devLog("search.response", {
    q: req.query,
    source: out._source,
    results: out.results?.length ?? 0,
    server_ms: out._duration_ms,
  });
  return out;
}

export async function recordClick(payload: ClickPayload): Promise<void> {
  try {
    const body = JSON.stringify({ source: "web-ui", ...payload });
    if (navigator.sendBeacon) {
      navigator.sendBeacon(`${BASE}/click`, new Blob([body], { type: "application/json" }));
      return;
    }
    await fetch(`${BASE}/click`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body,
      keepalive: true,
    });
  } catch {
    // best effort
  }
}

/** DDG autocomplete via the backend proxy: JSON array of phrases, max 6. */
export async function ddgAc(q: string, signal?: AbortSignal): Promise<string[]> {
  const res = await devTimed("ac", { q }, () =>
    fetch(`${BASE}/ac?q=${encodeURIComponent(q)}`, { signal }),
  );
  if (!res.ok) return [];
  const data = await res.json();
  const out = Array.isArray(data) ? (data as string[]) : [];
  devLog("ac", { q, results: out.length });
  return out;
}

/** OpenSearch suggestions: ["prefix", ["s1", ...], [], []] */
export async function suggest(q: string, signal?: AbortSignal): Promise<string[]> {
  const res = await devTimed("suggest", { q }, () =>
    fetch(`${BASE}/suggest?q=${encodeURIComponent(q)}`, { signal }),
  );
  if (!res.ok) return [];
  const data = await res.json();
  const out = (data?.[1] ?? []) as string[];
  devLog("suggest", { q, results: out.length });
  return out;
}

/** Delete a single cache row by its query hash (POST /row/{key}/delete).
 * Note: the backend route redirects (303) on success; we only check status. */
export async function deleteCacheRow(key: string): Promise<boolean> {
  try {
    const res = await fetch(`${BASE}/row/${encodeURIComponent(key)}/delete`, {
      method: "POST",
      headers: { Accept: "application/json" },
    });
    return res.ok;
  } catch {
    return false;
  }
}

export function cacheStats(signal?: AbortSignal): Promise<CacheStats> {
  return fetchJson<CacheStats>(`${BASE}/cache/stats`, signal);
}

export interface CacheStats {
  rows: number;
  unexpired_rows: number;
  db_size_bytes: number;
  total_hits: number;
  oldest_unexpired: number | null;
  newest: number | null;
}

async function fetchJson<T>(url: string, signal?: AbortSignal): Promise<T> {
  const res = await fetch(url, { signal });
  if (!res.ok) throw new Error(`GET ${url} failed: ${res.status}`);
  return (await res.json()) as T;
}

/** GET /api/history: merged clicks + cache-rows feed, newest first.
 * Params: since (hours or "all"), q (substring), limit (1-200), kind. */
export type HistoryScope = "24" | "168" | "720" | "all";

/** URL-contract since values map to the backend's vocabulary (24h|7d|30d|all). */
const SINCE_TO_BACKEND: Record<HistoryScope, string> = {
  "24": "24h",
  "168": "7d",
  "720": "30d",
  all: "all",
};
export type DeleteScope = "24h" | "7d" | "30d" | "all";

export interface HistoryItem {
  kind: "click" | "cache";
  query_hash: string;
  query: string;
  /** epoch seconds: clicked_at for clicks, created_at for cache rows */
  sort_at: number;
  url?: string;
  title?: string;
  source?: string;
  result_id?: string;
  created_at?: number;
  expires_at?: number;
  hits?: number;
  size_bytes?: number;
}

export interface HistoryResponse {
  items: HistoryItem[];
  clicks: number;
  cache_rows: number;
  limit: number;
  since: string;
}

export async function fetchApiHistory(
  since: HistoryScope,
  qText: string,
  signal?: AbortSignal,
): Promise<HistoryResponse> {
  const p = new URLSearchParams({ since: SINCE_TO_BACKEND[since] });
  if (qText) p.set("q", qText);
  const res = await fetch(`${BASE}/api/history?${p.toString()}`, { signal });
  if (!res.ok) throw new Error(`history failed: ${res.status}`);
  return (await res.json()) as HistoryResponse;
}

/** POST /history/delete: prune click history by scope.
 * Backend replies {"ok": true, "deleted": n} for JSON clients. */
export async function deleteHistory(scope: DeleteScope): Promise<number> {
  const res = await fetch(`${BASE}/history/delete`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify({ scope }),
  });
  if (!res.ok) throw new Error(`delete failed: ${res.status}`);
  return ((await res.json()) as { deleted: number }).deleted;
}

/** GET /api/stats: search-log aggregates for the dashboard plus cache
 * stats. Params: days=1..90 (default 14). */
export interface DaySearches {
  day: string;
  cache: number;
  network: number;
  total: number;
}

export interface ApiStats {
  days: number;
  searches_per_day: DaySearches[];
  hit_rate: { total: number; cache_hits: number; rate: number | null };
  latency_ms: { p50: number | null; p90: number | null; p99: number | null };
  top_queries: { query: string; count: number }[];
  zero_result_queries: { query: string; last_seen: number }[];
  client_split: { client: string; count: number }[];
  cache: CacheStats;
}

export function apiStats(signal?: AbortSignal): Promise<ApiStats> {
  return fetchJson<ApiStats>(`${BASE}/api/stats`, signal);
}
