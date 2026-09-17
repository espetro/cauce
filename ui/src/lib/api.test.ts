import { describe, expect, test, vi } from 'vitest'
import {
  emptySearxResponse,
  ForcedSearchError,
  fixturesEnabled,
  parseFixtureParams,
  search,
} from './api.ts'
import { forcedStateSchema, suggestSchema } from './fixtures.ts'

describe('force=error', () => {
  test('search() rejects with ForcedSearchError, no network call', async () => {
    const params = new URLSearchParams('q=test&force=error')
    await expect(search({ q: 'test' }, params)).rejects.toBeInstanceOf(ForcedSearchError)
  })
})

describe('force=empty', () => {
  test('search() resolves with a well-typed empty SearxResponse, no network call', async () => {
    const params = new URLSearchParams('q=test&force=empty')
    const result = await search({ q: 'test' }, params)

    expect(result).toEqual(emptySearxResponse('test'))
    expect(result.results).toEqual([])
    expect(result.query).toBe('test')
    expect(result.number_of_results).toBe(0)
  })
})

describe('force=ai-off', () => {
  test('parses into the ForcedState type via parseFixtureParams', () => {
    const params = new URLSearchParams('force=ai-off')
    expect(parseFixtureParams(params)).toEqual({ force: 'ai-off', suggest: false })
  })

  test('forcedStateSchema accepts the three recognized values plus absence', () => {
    expect(() =>
      import('valibot').then(({ parse }) => {
        expect(parse(forcedStateSchema, 'error')).toBe('error')
        expect(parse(forcedStateSchema, 'ai-off')).toBe('ai-off')
        expect(parse(forcedStateSchema, 'empty')).toBe('empty')
        expect(parse(forcedStateSchema, undefined)).toBe(null)
      }),
    ).not.toThrow()
  })
})

describe('suggest=1', () => {
  test('parseFixtureParams parses suggest=1 as true', () => {
    const params = new URLSearchParams('q=pyth&suggest=1')
    expect(parseFixtureParams(params)).toEqual({ force: null, suggest: true })
  })

  test('suggest=0 or absent parses as false', () => {
    expect(parseFixtureParams(new URLSearchParams('q=pyth'))).toEqual({
      force: null,
      suggest: false,
    })
    expect(parseFixtureParams(new URLSearchParams('suggest=0'))).toEqual({
      force: null,
      suggest: false,
    })
  })

  test('suggestSchema in isolation', async () => {
    const v = await import('valibot')
    expect(v.parse(suggestSchema, '1')).toBe(true)
    expect(v.parse(suggestSchema, undefined)).toBe(false)
  })
})

describe('dev-gating (production inertness)', () => {
  test('fixturesEnabled() reflects import.meta.env.DEV', () => {
    expect(fixturesEnabled()).toBe(import.meta.env.DEV)
  })

  test('parseFixtureParams ignores force/suggest when fixtures are disabled', async () => {
    vi.resetModules()
    vi.stubEnv('DEV', false)

    const { parseFixtureParams: parseWhenProd } = await import('./fixtures.ts')
    const params = new URLSearchParams('q=test&force=error&suggest=1')

    expect(parseWhenProd(params)).toEqual({ force: null, suggest: false })

    vi.unstubAllEnvs()
    vi.resetModules()
  })

  test('search() ignores force=error when fixtures are disabled and falls through to a real fetch attempt', async () => {
    vi.resetModules()
    vi.stubEnv('DEV', false)

    const { search: searchWhenProd } = await import('./api.ts')
    const params = new URLSearchParams('q=test&force=error')

    // With fixtures disabled, force=error is inert: the call falls through to a real
    // network request. There is no backend in this test environment, so it rejects, but
    // NOT with ForcedSearchError -- proving the fixture short-circuit was skipped.
    await expect(searchWhenProd({ q: 'test' }, params)).rejects.not.toBeInstanceOf(
      ForcedSearchError,
    )

    vi.unstubAllEnvs()
    vi.resetModules()
  })
})
