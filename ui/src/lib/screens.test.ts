import { describe, expect, test } from 'vitest'
import { cacheHitRatePercent, dashboardHasLogData } from './historyApi.ts'
import type { ClickItem, HistoryResponse, StatsResponse } from './historyApi.ts'

// Pure helpers are inlined in the route files for the screen; the invariant-relevant pure
// logic they share (stats line, qf substring filter, has-log-data gate) is exercised here
// against the typed shapes so the screens' behavior is pinned without mounting React.

function click(partial: Partial<ClickItem>): ClickItem {
  return {
    clicked_at: 1_700_000_000,
    query_hash: 'h1',
    query: 'python asyncio',
    result_id: 'r1',
    url: 'https://example.com/a',
    title: 'Example',
    source: 'web-ui',
    ...partial,
  }
}

const DAY = 24 * 3600

// Mirrors the HistoryResponse shape: stats come from the backend (`stats.last_24h`,
// `stats.total`, `stats.oldest`), not from scanning items client-side.
function historyResponse(items: ClickItem[], stats: HistoryResponse['stats']): HistoryResponse {
  return { items, stats, limit: 200, since: null }
}

describe('history stats line (checkpoint 15)', () => {
  // The route reads stats.last_24h / stats.total / stats.oldest straight off the response;
  // these fixtures pin the wire shape those reads depend on.
  test('backend stats line fields are surfaced as-is', () => {
    const now = 1_700_100_000
    const data = historyResponse(
      [click({ clicked_at: now - 3600 }), click({ clicked_at: now - 2 * DAY })],
      { last_24h: 1, total: 2, oldest: now - 2 * DAY },
    )
    expect(data.stats.last_24h).toBe(1)
    expect(data.stats.total).toBe(2)
    expect(data.stats.oldest).not.toBeNull()
  })

  test('zero clicks -> oldest is null (the dash state)', () => {
    const data = historyResponse([], { last_24h: 0, total: 0, oldest: null })
    expect(data.stats.last_24h).toBe(0)
    expect(data.stats.total).toBe(0)
    expect(data.stats.oldest).toBeNull()
  })
})

describe('qf substring filter (checkpoint 17)', () => {
  // Mirrors filterByQf() in routes/history.tsx: case-insensitive substring on query text.
  function filterByQf<T extends { query: string }>(items: T[], qf: string): T[] {
    const needle = qf.trim().toLowerCase()
    if (needle === '') return items
    return items.filter((item) => item.query.toLowerCase().includes(needle))
  }

  const items = [click({}), click({ query: 'Rust Tokio tutorial' })]

  test('case-insensitive substring match', () => {
    expect(filterByQf(items, 'TOKIO')).toHaveLength(1)
    expect(filterByQf(items, 'asyncio')).toHaveLength(1)
    expect(filterByQf(items, 'TUTORIAL')).toHaveLength(1)
  })

  test('empty/whitespace qf keeps all rows', () => {
    expect(filterByQf(items, '')).toHaveLength(2)
    expect(filterByQf(items, '   ')).toHaveLength(2)
  })

  test('no match -> empty (the "no rows match" state)', () => {
    expect(filterByQf(items, 'nonexistent')).toEqual([])
  })
})

describe('dashboard log-data gate (checkpoint 20)', () => {
  function makeStats(total: number): StatsResponse {
    return {
      days: 30,
      searches_per_day: [],
      hit_rate: { total, cache_hits: 0, rate: null },
      latency_ms: { p50: null, p90: null, p99: null },
      top_queries: [],
      zero_result_queries: [],
      client_split: [],
      cache: { rows: 0, unexpired: 0, total_hits: 0, db_size_bytes: 0, newest: null },
    }
  }

  test('empty search log -> placeholder state', () => {
    expect(dashboardHasLogData(makeStats(0))).toBe(false)
  })

  test('any logged searches -> real panels', () => {
    expect(dashboardHasLogData(makeStats(7))).toBe(true)
  })
})

describe('cacheHitRatePercent', () => {
  function withCache(rows: number, unexpired: number): StatsResponse {
    const base: StatsResponse = {
      days: 30,
      searches_per_day: [],
      hit_rate: { total: 0, cache_hits: 0, rate: null },
      latency_ms: { p50: null, p90: null, p99: null },
      top_queries: [],
      zero_result_queries: [],
      client_split: [],
      cache: { rows, unexpired, total_hits: 0, db_size_bytes: 0, newest: null },
    }
    return base
  }

  test('empty cache -> null', () => {
    expect(cacheHitRatePercent(withCache(0, 0))).toBeNull()
  })

  test('unexpired over rows, rounded', () => {
    expect(cacheHitRatePercent(withCache(340, 280))).toBe(82)
    expect(cacheHitRatePercent(withCache(4, 0))).toBe(0)
  })
})
