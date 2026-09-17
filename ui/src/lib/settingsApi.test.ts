import { describe, expect, test } from 'vitest'
import { settingsFormPayload } from '../components/SettingsDialog.tsx'
import { getSettings, putSettings, type SettingsWritePayload } from './settingsApi.ts'
import type { components, paths } from './types.gen.ts'

// Type-level pins: the wrappers must stay bound to the generated wire types, so drift
// between settingsApi.ts and types.gen.ts fails tsc, not just a test.
type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false
type _pinGet = Equal<
  Awaited<ReturnType<typeof getSettings>>,
  paths['/settings']['get']['responses'][200]['content']['application/json']
>
const _pinGet: _pinGet = true
type _pinPutBody = Equal<
  Parameters<typeof putSettings>[0],
  paths['/settings']['put']['requestBody']['content']['application/json']
>
const _pinPutBody: _pinPutBody = true
type _pinPutResponse = Equal<
  Awaited<ReturnType<typeof putSettings>>,
  components['schemas']['SettingsPayload']
>
const _pinPutResponse: _pinPutResponse = true

describe('settingsFormPayload (checkpoint 19 submit path)', () => {
  test('empty optional fields become null, checkbox becomes boolean', () => {
    const form = new FormData()
    form.set('provider', 'anthropic')
    form.set('model', 'claude-sonnet')
    form.set('api_key_env', ' ANTHROPIC_API_KEY ')
    form.set('enabled', 'on')
    expect(settingsFormPayload(form)).toEqual({
      provider: 'anthropic',
      model: 'claude-sonnet',
      api_key: null,
      api_key_env: 'ANTHROPIC_API_KEY',
      base_url: null,
      enabled: true,
      api_key_set: false,
    } satisfies SettingsWritePayload)
  })

  test('unticked checkbox parses as disabled', () => {
    const form = new FormData()
    form.set('provider', 'openai')
    form.set('model', 'gpt-4o-mini')
    expect(settingsFormPayload(form).enabled).toBe(false)
  })

  test('payload is assignable to the PUT body type (provider literal, model required)', () => {
    const form = new FormData()
    form.set('provider', 'mistral')
    form.set('model', 'mistral-small')
    const body: components['schemas']['SettingsWritePayload'] = settingsFormPayload(form)
    expect(body.provider).toBe('mistral')
  })
})
