/**
 * Rung-3 FSM for the AI answer stream (ui/AGENTS.md state ladder, plan step 21): a pure
 * reducer over the typed `AnswerFrame` union (generated in types.gen.ts, mirrored on the
 * backend in oxe/api/ai_frames.py), fed frame-by-frame from the SSE reader in
 * `src/lib/effects/answerStream.ts`.
 *
 * States: idle -> stepping (tool calls) -> streaming (deltas + sources) -> done, with
 * `failed` (transport error or standalone error frame; partial text kept) and `unavailable`
 * (force=ai-off fixture / AI not configured) reachable from the start.
 */
import type { components } from './types.gen.ts'

type AnswerFrame = components['schemas']['AnswerFrame']
type AnswerSource = components['schemas']['AnswerSource']
type DoneFrame = components['schemas']['DoneFrame']

export type AiState =
  | { status: 'idle' }
  | { status: 'stepping'; steps: string[]; sources: AnswerSource[] }
  | {
      status: 'streaming'
      text: string
      steps: string[]
      sources: AnswerSource[]
    }
  | {
      status: 'done'
      answer: string
      sources: AnswerSource[]
      relatedQuestions: string[]
      confidence: number
      model: string
      cached: boolean
      error: string | null
    }
  | { status: 'failed'; message: string; partialText: string; sources: AnswerSource[] }
  | { status: 'unavailable' }

export type AiEvent =
  | { type: 'started' }
  | { type: 'frame'; frame: AnswerFrame }
  | { type: 'transportFailed'; message: string }

/** Seed for `useReducer`: `force=ai-off` (checkpoint 12) jumps straight to unavailable. */
export function seedAiState(force: 'ai-off' | 'error' | 'empty' | null): AiState {
  return force === 'ai-off' ? { status: 'unavailable' } : { status: 'idle' }
}

export function aiReducer(state: AiState, event: AiEvent): AiState {
  switch (event.type) {
    case 'started':
      return state.status === 'idle' ? { status: 'stepping', steps: [], sources: [] } : state
    case 'transportFailed':
      // Mid-stream transport failure keeps whatever partial answer is on screen
      // (checkpoint 13: "partial text kept, retry / view Search").
      return state.status === 'streaming' || state.status === 'stepping'
        ? {
            status: 'failed',
            message: event.message,
            partialText: state.status === 'streaming' ? state.text : '',
            sources: state.sources,
          }
        : state.status === 'done'
          ? state
          : { status: 'failed', message: event.message, partialText: '', sources: [] }
    case 'frame':
      return applyFrame(state, event.frame)
    default: {
      const exhaustive: never = event
      return exhaustive
    }
  }
}

function applyFrame(state: AiState, frame: AnswerFrame): AiState {
  switch (frame.type) {
    case 'step': {
      if (state.status === 'done' || state.status === 'unavailable') {
        return state
      }
      const steps = state.status === 'stepping' || state.status === 'streaming' ? state.steps : []
      const sources = state.status === 'stepping' || state.status === 'streaming' ? state.sources : []
      return { status: 'stepping', steps: [...steps, frame.label], sources }
    }
    case 'delta': {
      if (state.status === 'done' || state.status === 'unavailable' || state.status === 'failed') {
        return state
      }
      const steps = state.status === 'streaming' || state.status === 'stepping' ? state.steps : []
      const sources = state.status === 'streaming' || state.status === 'stepping' ? state.sources : []
      const text = (state.status === 'streaming' ? state.text : '') + frame.text
      return { status: 'streaming', text, steps, sources }
    }
    case 'sources': {
      if (state.status === 'done' || state.status === 'unavailable' || state.status === 'failed') {
        return state
      }
      const sources = frame.sources ?? []
      if (state.status === 'streaming') {
        return { ...state, sources }
      }
      return { status: 'stepping', steps: state.status === 'stepping' ? state.steps : [], sources }
    }
    case 'done':
      return doneState(frame, state)
    case 'error':
      // Standalone error frame: a failure that aborted the stream before `done`
      // (oxe/api/ai_frames.py). Partial text kept, like a transport failure.
      return {
        status: 'failed',
        message: frame.message,
        partialText: state.status === 'streaming' ? state.text : '',
        sources: state.status === 'stepping' || state.status === 'streaming' ? state.sources : [],
      }
    default: {
      const exhaustive: never = frame
      return exhaustive
    }
  }
}

function doneState(frame: DoneFrame, state: AiState): AiState {
  if (state.status === 'done') {
    return state
  }
  return {
    status: 'done',
    answer: frame.answer,
    sources: state.status === 'stepping' || state.status === 'streaming' ? state.sources : [],
    relatedQuestions: frame.related_questions ?? [],
    confidence: frame.confidence,
    model: frame.model,
    cached: frame.cached,
    error: frame.error,
  }
}
