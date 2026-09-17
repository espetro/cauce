import { describe, expect, test } from 'vitest'
import { AI_CONFIG_FIELDS, aiConfigSchema } from './settingsApi.ts'
import { settingsFormPayload } from '../components/SettingsDialog.tsx'

// settingsApi.ts mirrors oxe/config.py's AIConfig dataclass field-for-field. This test is
// the cheap drift gate the CONTRACT PENDING CODEGEN section leans on: if a field is added
// to (or removed from) AIConfig, the schema keys must follow.
describe('settingsApi mirrors AIConfig', () => {
  test('schema keys equal AIConfig field names exactly', () => {
    expect(Object.keys(aiConfigSchema.entries).sort()).toEqual([...AI_CONFIG_FIELDS].sort())
  })

  test('field set is the backend dataclass set', () => {
    expect([...AI_CONFIG_FIELDS]).toEqual([
      'provider',
      'model',
      'api_key',
      'api_key_env',
      'base_url',
      'enabled',
    ])
  })
})

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
    })
  })

  test('unticked checkbox parses as disabled', () => {
    const form = new FormData()
    form.set('provider', 'openai')
    form.set('model', 'gpt-4o-mini')
    expect(settingsFormPayload(form).enabled).toBe(false)
  })
})
