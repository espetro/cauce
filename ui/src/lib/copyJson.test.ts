/**
 * Standalone unit test for the copy-json reducer: no renderer, no DOM, per the state ladder's
 * "pure function, unit tested with no renderer" rung-3 doctrine.
 */
import { describe, expect, it } from 'vitest'
import { copyJsonReducer, initialCopyJsonState } from './copyJson.ts'

describe('copyJsonReducer', () => {
  it('starts idle', () => {
    expect(initialCopyJsonState).toEqual({ status: 'idle' })
  })

  it('start -> loading', () => {
    expect(copyJsonReducer(initialCopyJsonState, { type: 'start' })).toEqual({
      status: 'loading',
    })
  })

  it('loading -> success -> copied', () => {
    const loading = copyJsonReducer(initialCopyJsonState, { type: 'start' })
    expect(copyJsonReducer(loading, { type: 'success' })).toEqual({ status: 'copied' })
  })

  it('loading -> failure -> error with message', () => {
    const loading = copyJsonReducer(initialCopyJsonState, { type: 'start' })
    expect(copyJsonReducer(loading, { type: 'failure', message: 'boom' })).toEqual({
      status: 'error',
      message: 'boom',
    })
  })

  it('reset returns to idle from any state', () => {
    const copied = copyJsonReducer(initialCopyJsonState, { type: 'start' })
    expect(copyJsonReducer(copied, { type: 'reset' })).toEqual({ status: 'idle' })
  })
})
