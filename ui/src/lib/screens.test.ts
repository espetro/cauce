import { describe, expect, test } from 'vitest'
import { dashboardHasLogData } from './historyApi.ts'
import type { CacheItem, ClickItem, HistoryResponse, StatsResponse } from './historyApi.ts'

// Pure helpers are inlined in the route files for the screen; the invariant-relevant pure
// logic they share (stats line, qf substring filter, has-log-data gate) is exercised here
// against the typed shapes so the screens' behavior is pinned without mounting React.

function click(partial: Partial<ClickItem>): ClickItem {
  return {
    kind: 'click',
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

function cacheRow(partial: Partial<CacheItem>): CacheItem {
  return {
    kind: 'cache',
    created_at: 1_700_000_000,
    query_hash: 'h2',
    query: 'rust tokio',
    expires_at: 1_700_086_400,
    hits: 2,
    size_bytes: 1024,
    ...partial,
  }
}

const DAY = 24 * 3600

// Mirrors statsLine() in routes/history.tsx: keep in sync (route-local because it formats
// via Lingui-sensitive display strings; the numbers are what the checkpoints assert).
function historyStats(data: HistoryResponse, nowSeconds: number) {
  const clicks = data.items.filter((item) => item.kind === 'click')
  return {
    clicks24h: clicks.filter((item) => item.kind === 'click' && item.clicked_at >= nowSeconds - DAY)
      .length,
    total: clicks.length,
  }
}

describe('history stats line (checkpoint 15)', () => {
  test('counts clicks in last 24h and total, ignoring cache rows', () => {
    const now = 1_700_100_000
    const data: HistoryResponse = {
      items: [
        click({ clicked_at: now - 3600 }),
        click({ clicked_at: now - 2 * DAY, query: 'old query' }),
        click({ clicked_at: now - 3600, source: 'mcp' }),
        cacheRow({}),
      ],
      clicks: 3,
      cache_rows: 1,
      limit: 200,
      since: 'all',
    }
    expect(historyStats(data, now)).toEqual({ clicks24h: 2, total: 3 })
  })

  test('zero clicks -> the empty-state predicate (dash oldest)', () => {
    const data: HistoryResponse = {
      items: [cacheRow({})],
      clicks: 0,
      cache_rows: 1,
      limit: 200,
      since: 'all',
    }
    expect(historyStats(data, 1_700_100_000)).toEqual({ clicks24h: 0, total: 0 })
  })
})

describe('qf substring filter (checkpoint 17)', () => {
  // Mirrors filterByQf() in routes/history.tsx: case-insensitive substring on query text.
  function filterByQf<T extends { query: string }>(items: T[], qf: string): T[] {
    const needle = qf.trim().toLowerCase()
    if (needle === '') return items
    return items.filter((item) => item.query.toLowerCase().includes(needle))
  }

  const items = [click({}), click({ query: 'Rust Tokio tutorial' }), cacheRow({})]

  test('case-insensitive substring match', () => {
    expect(filterByQf(items, 'TOKIO')).toHaveLength(2)
    expect(filterByQf(items, 'asyncio')).toHaveLength(1)
    expect(filterByQf(items, 'TUTORIAL')).toHaveLength(1)
  })

  test('empty/whitespace qf keeps all rows', () => {
    expect(filterByQf(items, '')).toHaveLength(3)
    expect(filterByQf(items, '   ')).toHaveLength(3)
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
      cache: {
        rows: 0,
        unexpired_rows: 0,
        db_size_bytes: 0,
        total_hits: 0,
        oldest_unexpired: null,
        newest: null,
      },
    }
  }

  test('empty search log -> placeholder state', () => {
    expect(dashboardHasLogData(makeStats(0))).toBe(false)
  })

  test('any logged searches -> real panels', () => {
    expect(dashboardHasLogData(makeStats(7))).toBe(true)
  })
})
