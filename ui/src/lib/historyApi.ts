/**
 * Typed client wrappers for the history/dashboard endpoints: `GET /api/history`,
 * `GET /api/stats` and the `copy json` re-fetch of a past query via `GET /search`.
 *
 * Wire-type status: `GET /api/history` and `GET /api/stats` do not yet exist on the rebuilt
 * backend (oxe/app.py serves `/health` + `/search` only in wave 2; the feature routers are a
 * later wave), so `types.gen.ts` has no `paths` entry for them and `openapi-fetch` cannot
 * type those call sites yet. To avoid hand-drawn `any`-shaped responses, the shapes below
 * are pinned structurally to what the backend WILL publish, copied from the legacy
 * `v0.4.0` implementation these specs describe:
 *
 * - `/api/history` -> `oxe/server/schemas.py` `ApiHistoryResponse` / `ClickItem`
 *   (items[], clicks, cache_rows, limit, since; sort newest-first server-side).
 * - `/api/stats`   -> `oxe/stats.py` `build_json()` merged with `TTLCache.stats()`:
 *   {days, searches_per_day[], hit_rate{total,cache_hits,rate}, latency_ms{p50,p90,p99},
 *    top_queries[], zero_result_queries[], client_split[], cache{rows,unexpired_rows,
 *    db_size_bytes,total_hits,oldest_unexpired,newest}}.
 *
 * When the backend routers land, `bun run gen:types` grows the `paths` entries and these
 * interfaces are deleted in favor of `components["schemas"][...]` — the migration is
 * mechanical because nothing outside this module names a field that is not on these types.
 *
 * Until then, each wrapper also passes the fields through `v.parse` against a valibot schema
 * at the boundary (Layer 2 runtime validation: this is a boundary types cannot reach, since
 * the generator cannot describe the endpoint yet), so a shape drift fails loudly instead of
 * leaking `undefined` into the screens.
 */
import * as v from 'valibot'
import { API_BASE_URL } from './api.ts'

// ---------------------------------------------------------------------------
// /api/history
// ---------------------------------------------------------------------------

/** A recorded click (web-ui or mcp). Mirrors legacy `ClickItem`. */
export interface ClickItem {
  kind: 'click'
  /** Unix seconds. */
  clicked_at: number
  query_hash: string
  query: string
  result_id: string
  url: string
  title: string
  /** `'web-ui'` or `'mcp'`. */
  source: string
}

/** A cached search row. Mirrors legacy `CacheItem`. */
export interface CacheItem {
  kind: 'cache'
  /** Unix seconds. */
  created_at: number
  query_hash: string
  query: string
  expires_at: number
  hits: number
  size_bytes: number
}

export type HistoryItem = ClickItem | CacheItem

/** Mirrors legacy `ApiHistoryResponse`. Newest first, capped at `limit` (<= 200). */
export interface HistoryResponse {
  items: HistoryItem[]
  clicks: number
  cache_rows: number
  limit: number
  since: string
}

const clickItemSchema = v.object({
  kind: v.literal('click'),
  clicked_at: v.number(),
  query_hash: v.string(),
  query: v.string(),
  result_id: v.string(),
  url: v.string(),
  title: v.string(),
  source: v.string(),
})

const cacheItemSchema = v.object({
  kind: v.literal('cache'),
  created_at: v.number(),
  query_hash: v.string(),
  query: v.string(),
  expires_at: v.number(),
  hits: v.number(),
  size_bytes: v.number(),
})

const historyResponseSchema = v.object({
  items: v.array(v.union([clickItemSchema, cacheItemSchema])),
  clicks: v.number(),
  cache_rows: v.number(),
  limit: v.number(),
  since: v.string(),
})

/** Query params for `GET /api/history`. */
export interface HistoryQuery {
  /** `'24h' | '7d' | '30d' | 'all'` — server-side time filter. */
  since: '24h' | '7d' | '30d' | 'all'
  /** Server-side query-text substring filter. */
  q?: string
  /** 1..200, default 50. */
  limit?: number
}

/**
 * Fetch the merged click+cache history view (newest first) from `GET /api/history`.
 * Throws on non-2xx or shape drift (see module docstring for the wire-type status).
 */
export async function history(query: HistoryQuery): Promise<HistoryResponse> {
  const params = new URLSearchParams({ since: query.since })
  if (query.q !== undefined) params.set('q', query.q)
  if (query.limit !== undefined) params.set('limit', String(query.limit))
  const data = await fetchJson(`${API_BASE_URL}/api/history?${params}`, '/api/history')
  return v.parse(historyResponseSchema, data) as HistoryResponse
}

/** One `fetch` + JSON parse + status check, shared by both wrappers. */
async function fetchJson(url: string, label: string): Promise<unknown> {
  const response = await fetch(url, { headers: { Accept: 'application/json' } })
  if (!response.ok) {
    throw new Error(`${label} failed (${response.status})`)
  }
  return response.json() as unknown
}

// ---------------------------------------------------------------------------
// /api/stats
// ---------------------------------------------------------------------------

/** Cache table stats. Mirrors legacy `CacheStatsResponse`. */
export interface CacheStats {
  rows: number
  unexpired_rows: number
  db_size_bytes: number
  total_hits: number
  oldest_unexpired: number | null
  newest: number | null
}

/** Dashboard aggregates. Mirrors legacy `build_json()` + `cache` key. */
export interface StatsResponse {
  days: number
  searches_per_day: Array<{ day: string; cache: number; network: number; total: number }>
  hit_rate: { total: number; cache_hits: number; rate: number | null }
  latency_ms: { p50: number | null; p90: number | null; p99: number | null }
  top_queries: Array<{ query: string; count: number }>
  zero_result_queries: Array<{ query: string; last_seen: number }>
  client_split: Array<{ client: string; count: number }>
  cache: CacheStats
}

const nullableNumber = v.union([v.number(), v.null()])

const statsResponseSchema = v.object({
  days: v.number(),
  searches_per_day: v.array(
    v.object({ day: v.string(), cache: v.number(), network: v.number(), total: v.number() }),
  ),
  hit_rate: v.object({ total: v.number(), cache_hits: v.number(), rate: nullableNumber }),
  latency_ms: v.object({ p50: nullableNumber, p90: nullableNumber, p99: nullableNumber }),
  top_queries: v.array(v.object({ query: v.string(), count: v.number() })),
  zero_result_queries: v.array(v.object({ query: v.string(), last_seen: v.number() })),
  client_split: v.array(v.object({ client: v.string(), count: v.number() })),
  cache: v.object({
    rows: v.number(),
    unexpired_rows: v.number(),
    db_size_bytes: v.number(),
    total_hits: v.number(),
    oldest_unexpired: nullableNumber,
    newest: nullableNumber,
  }),
})

/**
 * Fetch the dashboard aggregates from `GET /api/stats`. The backend does not aggregate the
 * search-log-derived panels yet (dashboard.md), so screens must treat missing/log-less
 * deployments as the normal case, not an error — see `dashboardHasLogData()`.
 */
export async function stats(days = 30): Promise<StatsResponse> {
  const params = new URLSearchParams({ days: String(days) })
  const data = await fetchJson(`${API_BASE_URL}/api/stats?${params}`, '/api/stats')
  return v.parse(statsResponseSchema, data) as StatsResponse
}

/**
 * Whether the search-log-derived dashboard panels have real data to show (dashboard.md:
 * log-less deployments render the muted "no search log data yet" placeholder instead).
 * A hit_rate.total of 0 across the window means the search_log table is empty or absent.
 */
export function dashboardHasLogData(stats: StatsResponse): boolean {
  return stats.hit_rate.total > 0
}
