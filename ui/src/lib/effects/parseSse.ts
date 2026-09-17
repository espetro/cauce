/**
 * SSE frame parser for `POST /answer` (Layer 2, plan "the SSE hole"): the transport is
 * text/event-stream so OpenAPI cannot reach the payloads and a small parser is the one
 * sanctioned runtime-validation boundary. Pure: text in, typed frames out, no fetch, no
 * React -- unit-tested directly in `parseSse.test.ts`.
 *
 * The stream shape (oxe/api/ai.py, FastAPI StreamingResponse): each event is
 * `data: {json}\n\n` with no `event:` field, so each `data:` line carries one
 * `AnswerFrameEvent` whose `data` is one `AnswerFrame`. Malformed lines are skipped
 * (returned as `sse-errors`) rather than aborting the parse of the rest of the chunk.
 */
import type { components } from '../types.gen.ts'

type AnswerFrame = components['schemas']['AnswerFrame']

export interface SseResult {
  frames: AnswerFrame[]
  errors: string[]
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function isFrame(value: unknown): value is AnswerFrame {
  if (!isObject(value)) {
    return false
  }
  switch (value['type']) {
    case 'step':
      return typeof value['tool'] === 'string' && typeof value['query'] === 'string' && typeof value['label'] === 'string'
    case 'delta':
      return typeof value['text'] === 'string'
    case 'sources':
      return Array.isArray(value['sources']) && value['sources'].every(isAnswerSource)
    case 'done':
      return (
        typeof value['answer'] === 'string' &&
        typeof value['confidence'] === 'number' &&
        typeof value['model'] === 'string' &&
        typeof value['cached'] === 'boolean'
      )
    case 'error':
      return typeof value['message'] === 'string'
    default:
      return false
  }
}

function isAnswerSource(value: unknown): value is components['schemas']['AnswerSource'] {
  return isObject(value) && typeof value['title'] === 'string' && typeof value['url'] === 'string'
}

/**
 * Parse one SSE `data:` line's JSON payload into a typed `AnswerFrame`, or `null` with a
 * reason when the payload is not a valid frame. Exported for direct unit testing.
 */
export function parseFramePayload(payload: string): { frame: AnswerFrame } | { invalid: string } {
  let parsed: unknown
  try {
    parsed = JSON.parse(payload)
  } catch {
    return { invalid: 'unparseable json' }
  }
  return isFrame(parsed) ? { frame: parsed } : { invalid: 'not a valid AnswerFrame' }
}

/**
 * Parse a chunk of SSE text (possibly containing several `data: ...` events, possibly
 * splitting an event across chunk boundaries via `carry`) into frames. Returns the frames
 * and any leftover partial event text to carry into the next chunk.
 */
export function parseSseChunk(text: string, carry: string): { result: SseResult; carry: string } {
  const frames: AnswerFrame[] = []
  const errors: string[] = []
  let buffer = carry + text
  // Events are delimited by a blank line. If the chunk ends mid-event, keep the tail.
  const events = buffer.split('\n\n')
  const tail = events.pop() ?? ''
  buffer = ''

  for (const event of events) {
    for (const line of event.split('\n')) {
      if (!line.startsWith('data:')) {
        continue
      }
      const payload = line.slice(5).trim()
      if (payload.length === 0) {
        continue
      }
      const parsed = parseFramePayload(payload)
      if ('frame' in parsed) {
        frames.push(parsed.frame)
      } else {
        errors.push(parsed.invalid)
      }
    }
  }
  return { result: { frames, errors }, carry: tail }
}
