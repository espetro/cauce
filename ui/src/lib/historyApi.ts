/**
 * Typed client wrappers for the history/dashboard endpoints: `GET /api/history`,
 * `GET /api/stats` and the `copy json` re-fetch of a past query via `GET /search`.
 *
 * MIGRATION NOTE (read this before editing the CONTRACT PENDING section):
 *
 * The ui/AGENTS.md wire-type rule is "openapi.json -> openapi-typescript -> openapi-fetch;
 * hand-written response interfaces are banned". The backend routes (`w2-api-routes`,
 * porting legacy v0.4.0's history response and `oxe/stats.py`'s StatsSummary with
 * pydantic `extra="forbid"`) have not landed on main yet, so `bun run gen:types` cannot
 * emit `components["schemas"]` for them and `openapi-fetch` cannot type these call sites.
 *
 * To keep this file honest we do NOT keep parallel interface copies. Everything below the
 * "CONTRACT PENDING CODEGEN" marker is a single, minimal, centralized section that:
 *
 * 1. mirrors exactly what the backend routes will declare (valibot schemas double as the
 *    Layer-2 runtime boundary check, since the generator cannot describe the endpoint yet),
 * 2. is the ONLY place in the repo that names these wire fields by hand,
 * 3. is marked `TODO(w2-api)` for deletion.
 *
 * When the routes land: run `bun run gen:types`, delete the whole marked section, source
 * each type from `components["schemas"]["..."]`, and pin it with a `type Equal<A, B>`
 * assertion so any drift between the codegen output and these call sites fails tsc. The
 * exported wrapper signatures below do not change. Until then this is the honest interim
 * state: the ban is on drifting parallel copies, not on one marked, centralized,
 * deletion-planned section.
 */
import * as v from 'valibot'
import { API_BASE_URL } from './api.ts'

// ===========================================================================
// CONTRACT PENDING CODEGEN -- DELETE THIS SECTION AFTER `bun run gen:types`
// TODO(w2-api): replace with components["schemas"] Equal assertions:
//   type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2)
//     ? true : false
//   and one `const _pin: Equal<HistoryItem, components["schemas"]["HistoryItem"]> = true`
//   per type below. Nothing else in this file may reference hand-written wire shapes.
// ===========================================================================

const clickItemSchema = v.object({
  kind: v.literal('click'),
  /** Unix seconds. */
  clicked_at: v.number(),
  query_hash: v.string(),
  query: v.string(),
  result_id: v.string(),
  url: v.string(),
  title: v.string(),
  /** `'web-ui'` or `'mcp'`. */
  source: v.string(),
})

const cacheItemSchema = v.object({
  kind: v.literal('cache'),
  /** Unix seconds. */
  created_at: v.number(),
  query_hash: v.string(),
  query: v.string(),
  expires_at: v.number(),
  hits: v.number(),
  size_bytes: v.number(),
})

/** v0.4.0 `ClickItem`. */
export type ClickItem = v.InferOutput<typeof clickItemSchema>
/** v0.4.0 `CacheItem`. */
export type CacheItem = v.InferOutput<typeof cacheItemSchema>
/** v0.4.0 `ApiHistoryResponse`. Newest first, capped at `limit` (<= 200). */
export type HistoryResponse = {
  items: Array<ClickItem | CacheItem>
  clicks: number
  cache_rows: number
  limit: number
  since: string
}

const historyResponseSchema = v.object({
  items: v.array(v.union([clickItemSchema, cacheItemSchema])),
  clicks: v.number(),
  cache_rows: v.number(),
  limit: v.number(),
  since: v.string(),
})

const nullableNumber = v.union([v.number(), v.null()])

/** v0.4.0 `CacheStatsResponse`. */
export type CacheStats = {
  rows: number
  unexpired_rows: number
  db_size_bytes: number
  total_hits: number
  oldest_unexpired: number | null
  newest: number | null
}

/** `oxe/stats.py` `StatsSummary` merged with the legacy `cache` key. */
export type StatsResponse = {
  days: number
  searches_per_day: Array<{ day: string; cache: number; network: number; total: number }>
  hit_rate: { total: number; cache_hits: number; rate: number | null }
  latency_ms: { p50: number | null; p90: number | null; p99: number | null }
  top_queries: Array<{ query: string; count: number }>
  zero_result_queries: Array<{ query: string; last_seen: number }>
  client_split: Array<{ client: string; count: number }>
  cache: CacheStats
}

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

// ===========================================================================
// END CONTRACT PENDING CODEGEN
// ===========================================================================

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
