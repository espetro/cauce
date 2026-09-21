/**
 * The one new `useEffect` for wave 4 (ui/AGENTS.md Enforced rule: `useEffect` lives only in
 * `src/lib/effects/`): wires the `POST /answer` SSE transport to the rung-3 `aiReducer`.
 *
 * All decisions are expressed as reducer events (`started`, `frame`, `transportFailed`);
 * frames arrive as text, so parsing is delegated to the pure `parseSseChunk` (parseSse.ts,
 * Layer 2's sanctioned SSE boundary). The AbortController cancels the stream on cleanup,
 * and an aborted fetch resolves instead of rejecting so a stale stream never dispatches.
 */
import { useEffect } from 'react'
import { client } from '../api.ts'
import type { AiEvent } from '../aiReducer.ts'
import { parseSseChunk } from './parseSse.ts'

export interface AnswerStreamProps {
  query: string
  force: 'ai-off' | 'error' | 'empty' | null
  dispatch: (event: AiEvent) => void
}

export function useAnswerStream({ query, force, dispatch }: AnswerStreamProps): void {
  useEffect(() => {
    const controller = new AbortController()
    let cancelled = false

    const run = async (): Promise<void> => {
      dispatch({ type: 'started' })
      const response = await client.POST('/answer', {
        body: { query, mode: 'answer', force },
        headers: { Accept: 'text/event-stream' },
        signal: controller.signal,
        parseAs: 'stream',
      })
      if (!response.response.ok || !response.response.body) {
        throw new Error(`answer request failed: HTTP ${String(response.response.status)}`)
      }
      const reader = response.response.body.getReader()
      const decoder = new TextDecoder()
      let carry = ''
      for (;;) {
        const { done, value } = await reader.read()
        if (done || cancelled) {
          return
        }
        const { result, carry: next } = parseSseChunk(decoder.decode(value, { stream: true }), carry)
        carry = next
        for (const frame of result.frames) {
          dispatch({ type: 'frame', frame })
        }
      }
    }

    run().catch((error: unknown) => {
      if (cancelled || (error instanceof DOMException && error.name === 'AbortError')) {
        return
      }
      dispatch({
        type: 'transportFailed',
        message: error instanceof Error ? error.message : String(error),
      })
    })

    return () => {
      cancelled = true
      controller.abort()
    }
  }, [query, force, dispatch])
}

