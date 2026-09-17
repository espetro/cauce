import { describe, expect, test } from 'vitest'
import { aiReducer, seedAiState } from './aiReducer.ts'
import type { components } from './types.gen.ts'

type AnswerFrame = components['schemas']['AnswerFrame']
type AnswerSource = components['schemas']['AnswerSource']

const source = (n: number): AnswerSource => ({
  title: `source ${n}`,
  url: `https://example.com/${n}`,
  favicon: null,
})

const step = (label: string): AnswerFrame => ({ type: 'step', tool: 'web', query: 'q', label })
const delta = (text: string): AnswerFrame => ({ type: 'delta', text })
const sources = (n: number): AnswerFrame => ({
  type: 'sources',
  sources: Array.from({ length: n }, (_, i) => source(i + 1)),
})
const done = (overrides: Partial<Extract<AnswerFrame, { type: 'done' }>> = {}): AnswerFrame => ({
  type: 'done',
  answer: 'final answer',
  related_questions: ['related?'],
  confidence: 90,
  model: 'test-model',
  cached: false,
  error: null,
  ...overrides,
})

/** Feed a fresh machine a full frame sequence, the way the SSE effect would. */
function run(frames: AnswerFrame[]) {
  let state = aiReducer({ status: 'idle' }, { type: 'started' })
  for (const frame of frames) {
    state = aiReducer(state, { type: 'frame', frame })
  }
  return state
}

describe('seedAiState', () => {
  test('force=ai-off seeds unavailable (checkpoint 12)', () => {
    expect(seedAiState('ai-off')).toEqual({ status: 'unavailable' })
  })
  test('no force seeds idle', () => {
    expect(seedAiState(null)).toEqual({ status: 'idle' })
  })
})

describe('happy path (checkpoints 9, 10, 11)', () => {
  test('steps accumulate before streaming', () => {
    const state = run([step('searching the web')])
    expect(state).toEqual({ status: 'stepping', steps: ['searching the web'], sources: [] })
  })

  test('deltas concatenate; sources replace', () => {
    const state = run([step('s'), delta('hel'), delta('lo'), sources(2)])
    expect(state).toEqual({
      status: 'streaming',
      text: 'hello',
      steps: ['s'],
      sources: [source(1), source(2)],
    })
  })

  test('done captures answer, sources, related, confidence, cached badge', () => {
    const state = run([step('s'), delta('partial'), sources(2), done({ cached: true, confidence: 77 })])
    expect(state).toEqual({
      status: 'done',
      answer: 'final answer',
      sources: [source(1), source(2)],
      relatedQuestions: ['related?'],
      confidence: 77,
      model: 'test-model',
      cached: true,
      error: null,
    })
  })

  test('done without prior sources yields empty list (checkpoint 14 seed)', () => {
    const state = run([delta('text'), done()])
    expect(state.status).toBe('done')
    if (state.status === 'done') {
      expect(state.sources).toEqual([])
    }
  })
})

describe('failures (checkpoint 13)', () => {
  test('transport failure mid-stream keeps partial text', () => {
    const state = run([delta('partial ans')])
    const failed = aiReducer(state, { type: 'transportFailed', message: 'boom' })
    expect(failed).toEqual({
      status: 'failed',
      message: 'boom',
      partialText: 'partial ans',
      sources: [],
    })
  })

  test('standalone error frame fails with partial text', () => {
    const state = run([delta('text'), sources(1)])
    const failed = aiReducer(state, { type: 'frame', frame: { type: 'error', message: 'bad' } })
    expect(failed).toEqual({ status: 'failed', message: 'bad', partialText: 'text', sources: [source(1)] })
  })

  test('done frame with error field lands in done with error set', () => {
    const state = run([done({ error: 'low confidence', confidence: 20 })])
    expect(state.status).toBe('done')
    if (state.status === 'done') {
      expect(state.error).toBe('low confidence')
    }
  })

  test('transport failure before any frame still fails', () => {
    const failed = aiReducer({ status: 'stepping', steps: [], sources: [] }, {
      type: 'transportFailed',
      message: 'x',
    })
    expect(failed).toEqual({ status: 'failed', message: 'x', partialText: '', sources: [] })
  })
})

describe('terminal-state guards', () => {
  test('frames after done are ignored', () => {
    const finished = run([done({ cached: true })])
    expect(aiReducer(finished, { type: 'frame', frame: delta('late') })).toBe(finished)
    expect(aiReducer(finished, { type: 'frame', frame: sources(1) })).toBe(finished)
    expect(aiReducer(finished, { type: 'frame', frame: done() })).toBe(finished)
  })

  test('frames after unavailable are ignored', () => {
    const off = seedAiState('ai-off')
    expect(aiReducer(off, { type: 'frame', frame: delta('x') })).toBe(off)
    expect(aiReducer(off, { type: 'started' })).toBe(off)
  })

  test('started is idempotent outside idle', () => {
    const streaming = run([delta('a')])
    expect(aiReducer(streaming, { type: 'started' })).toBe(streaming)
  })
})
