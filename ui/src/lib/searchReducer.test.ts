import { describe, expect, test } from 'vitest'
import {
  MAX_PAGES,
  searchReducer,
  seedSearchState,
} from './searchReducer.ts'
import { emptySearxResponse } from './api.ts'
import type { components } from './types.gen.ts'

type SearxResponse = components['schemas']['SearxResponse']

function responseWith(count: number, query = 'test'): SearxResponse {
  const base = emptySearxResponse(query)
  return {
    ...base,
    number_of_results: count,
    suggestions: ['rel 1'],
    results: Array.from({ length: count }, (_, i) => ({
      category: 'general',
      content: `snippet ${i}`,
      engine: 'ddg',
      score: i,
      title: `result ${i}`,
      url: `https://example.com/${i}`,
    })),
  }
}

describe('seedSearchState', () => {
  test('a loader failure seeds the error state', () => {
    expect(seedSearchState({ loadError: 'boom' })).toEqual({ status: 'error', message: 'boom' })
  })

  test('null loader data seeds idle', () => {
    expect(seedSearchState(null)).toEqual({ status: 'idle' })
  })

  test('empty response seeds empty status', () => {
    expect(seedSearchState(emptySearxResponse('q')).status).toBe('empty')
  })

  test('page-1 response seeds ready with hasMore', () => {
    const state = seedSearchState(responseWith(3))
    expect(state).toMatchObject({ status: 'ready', pageno: 1, hasMore: true, numberOfResults: 3 })
  })
})

describe('loaded', () => {
  test('zero results -> empty', () => {
    const next = searchReducer({ status: 'loading' }, { type: 'loaded', response: emptySearxResponse('q') })
    expect(next.status).toBe('empty')
  })

  test('non-empty -> ready with meta and captured payload', () => {
    const response = responseWith(5)
    const next = searchReducer({ status: 'loading' }, { type: 'loaded', response })
    expect(next).toMatchObject({
      status: 'ready',
      pageno: 1,
      hasMore: true,
      loadingMore: false,
      numberOfResults: 5,
      suggestions: ['rel 1'],
    })
    if (next.status === 'ready') {
      expect(next.payload).toBe(response)
    }
  })
})

describe('failed', () => {
  test('carries the message into the error state', () => {
    const next = searchReducer({ status: 'loading' }, { type: 'failed', message: '502' })
    expect(next).toEqual({ status: 'error', message: '502' })
  })
})

describe('continuous scroll appends (checkpoint 4)', () => {
  test('moreRequested sets loadingMore and clears hasMore while fetching', () => {
    const ready = seedSearchState(responseWith(3))
    if (ready.status !== 'ready') throw new Error('expected ready')
    const next = searchReducer(ready, { type: 'moreRequested' })
    expect(next).toMatchObject({ status: 'ready', loadingMore: true, hasMore: false, pageno: 1 })
  })

  test('moreLoaded appends and advances pageno', () => {
    const ready = seedSearchState(responseWith(3))
    if (ready.status !== 'ready') throw new Error('expected ready')
    const requesting = searchReducer(ready, { type: 'moreRequested' })
    const next = searchReducer(requesting, { type: 'moreLoaded', response: responseWith(2, 'test') })
    expect(next).toMatchObject({ status: 'ready', pageno: 2, loadingMore: false })
    if (next.status === 'ready') {
      expect(next.results).toHaveLength(5)
    }
  })

  test('moreFailed keeps results and re-enables retry via hasMore', () => {
    const ready = seedSearchState(responseWith(3))
    if (ready.status !== 'ready') throw new Error('expected ready')
    const requesting = searchReducer(ready, { type: 'moreRequested' })
    const next = searchReducer(requesting, { type: 'moreFailed', message: 'boom' })
    expect(next).toMatchObject({ status: 'ready', hasMore: true, loadingMore: false })
    if (next.status === 'ready') {
      expect(next.results).toHaveLength(3)
    }
  })

  test(`hasMore goes false at the ${MAX_PAGES}-page cap -> end of results`, () => {
    let state = seedSearchState(responseWith(2))
    for (let pageno = 1; pageno < MAX_PAGES; pageno += 1) {
      if (state.status !== 'ready') throw new Error('expected ready')
      state = searchReducer(searchReducer(state, { type: 'moreRequested' }), {
        type: 'moreLoaded',
        response: responseWith(1),
      })
    }
    expect(state).toMatchObject({ status: 'ready', pageno: MAX_PAGES, hasMore: false })
  })

  test('moreRequested is ignored when hasMore is false', () => {
    let state = seedSearchState(responseWith(2))
    for (let pageno = 1; pageno < MAX_PAGES; pageno += 1) {
      if (state.status !== 'ready') throw new Error('expected ready')
      state = searchReducer(searchReducer(state, { type: 'moreRequested' }), {
        type: 'moreLoaded',
        response: responseWith(1),
      })
    }
    expect(searchReducer(state, { type: 'moreRequested' })).toBe(state)
  })
})
