import { describe, expect, test, vi } from 'vitest'
import { emptySearxResponse } from './api.ts'
import { loadSearchPage } from './searchLoader.ts'

describe('loadSearchPage', () => {
  test('AI mode never calls classic search, even with a query and a force fixture', async () => {
    const searchFn = vi.fn()
    const result = await loadSearchPage({ q: 'test', mode: 'ai', force: 'error' }, searchFn)
    expect(result).toBeNull()
    expect(searchFn).not.toHaveBeenCalled()
  })

  test('AI mode with the ai-off fixture still loads classic results for the unavailable state', async () => {
    const payload = emptySearxResponse('test')
    const searchFn = vi.fn().mockResolvedValue(payload)
    const result = await loadSearchPage({ q: 'test', mode: 'ai', force: 'ai-off' }, searchFn)
    expect(result).toBe(payload)
    expect(searchFn).toHaveBeenCalledOnce()
  })

  test('empty query returns null without a call', async () => {
    const searchFn = vi.fn()
    expect(await loadSearchPage({ q: '', mode: undefined, force: null }, searchFn)).toBeNull()
    expect(searchFn).not.toHaveBeenCalled()
  })

  test('classic mode calls search with the encoded query and force fixture', async () => {
    const payload = emptySearxResponse('a b')
    const searchFn = vi.fn().mockResolvedValue(payload)
    const result = await loadSearchPage({ q: 'a b', mode: undefined, force: 'empty' }, searchFn)
    expect(result).toBe(payload)
    expect(searchFn).toHaveBeenCalledOnce()
    const [req, params] = searchFn.mock.calls[0] as [{ q: string }, URLSearchParams]
    expect(req).toEqual({ q: 'a b' })
    expect(params.get('q')).toBe('a b')
    expect(params.get('force')).toBe('empty')
  })

  test('a rejected search resolves to a load failure instead of throwing', async () => {
    const searchFn = vi.fn().mockRejectedValue(new Error('boom'))
    expect(await loadSearchPage({ q: 'x', mode: undefined, force: null }, searchFn)).toEqual({ loadError: 'boom' })
  })
})
