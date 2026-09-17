import { describe, expect, test } from 'vitest'
import { parseFramePayload, parseSseChunk } from './parseSse.ts'

const stepEvent = 'data: {"type":"step","tool":"web","query":"q","label":"searching"}\n\n'
const deltaEvent = 'data: {"type":"delta","text":"hello"}\n\n'

describe('parseFramePayload', () => {
  test('accepts each frame kind', () => {
    expect(parseFramePayload('{"type":"delta","text":"x"}')).toEqual({ frame: { type: 'delta', text: 'x' } })
    expect(parseFramePayload('{"type":"error","message":"m"}')).toEqual({ frame: { type: 'error', message: 'm' } })
    expect(
      parseFramePayload('{"type":"sources","sources":[{"title":"t","url":"u","favicon":null}]}'),
    ).toEqual({ frame: { type: 'sources', sources: [{ title: 't', url: 'u', favicon: null }] } })
  })

  test('rejects unparseable json and wrong shapes', () => {
    expect(parseFramePayload('{nope')).toEqual({ invalid: 'unparseable json' })
    expect(parseFramePayload('{"type":"nope"}')).toEqual({ invalid: 'not a valid AnswerFrame' })
    expect(parseFramePayload('{"type":"delta"}')).toEqual({ invalid: 'not a valid AnswerFrame' })
    expect(parseFramePayload('42')).toEqual({ invalid: 'not a valid AnswerFrame' })
  })
})

describe('parseSseChunk', () => {
  test('parses multiple events in one chunk', () => {
    const { result, carry } = parseSseChunk(stepEvent + deltaEvent, '')
    expect(result.frames.map((f) => f.type)).toEqual(['step', 'delta'])
    expect(result.errors).toEqual([])
    expect(carry).toBe('')
  })

  test('carries a split event across chunks', () => {
    const first = parseSseChunk(stepEvent + 'data: {"type":"del', '')
    expect(first.result.frames.map((f) => f.type)).toEqual(['step'])
    const second = parseSseChunk('ta","text":"hi"}\n\n', first.carry)
    expect(second.result.frames).toEqual([{ type: 'delta', text: 'hi' }])
    expect(second.carry).toBe('')
  })

  test('collects malformed events as errors without aborting', () => {
    const { result } = parseSseChunk('data: {broken}\n\n' + deltaEvent, '')
    expect(result.errors).toEqual(['unparseable json'])
    expect(result.frames.map((f) => f.type)).toEqual(['delta'])
  })

  test('ignores non-data lines (event:, comments)', () => {
    const { result } = parseSseChunk(': keepalive\n event: message\n' + deltaEvent, '')
    expect(result.frames.map((f) => f.type)).toEqual(['delta'])
    expect(result.errors).toEqual([])
  })
})
