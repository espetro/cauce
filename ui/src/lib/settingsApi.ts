/**
 * Typed client wrappers for the future settings endpoints: `GET /settings` and
 * `PUT /settings` (userflow-checkpoints.md checkpoint 19: settings are "persisted
 * backend-side via `PUT /settings`"). The rebuilt backend does not serve these routes yet,
 * so the wire types for exactly these two operations live in the single marked section
 * below -- the same discipline `historyApi.ts` used before `/api/history` landed in the
 * generated spec. TODO(settings-api): once the routes exist in `oxe`'s OpenAPI output, run
 * `bun run gen:types`, delete the CONTRACT PENDING CODEGEN section, and re-point the calls
 * at `paths` from `./types.gen.ts`.
 *
 * The payload shape mirrors `oxe/config.py`'s `AIConfig` dataclass field-for-field
 * (`provider`, `model`, `api_key`, `api_key_env`, `base_url`, `enabled`); the valibot schema
 * below is pinned to that field set by `settingsApi.test.ts` so drift fails a test.
 */
import createClient from 'openapi-fetch'
import * as v from 'valibot'
import { client } from './api.ts'

/** Field names of `oxe/config.py`'s `AIConfig`, in declaration order. */
export const AI_CONFIG_FIELDS = [
  'provider',
  'model',
  'api_key',
  'api_key_env',
  'base_url',
  'enabled',
] as const

/** Runtime mirror of `AIConfig` (Layer 2 parser at the one boundary types cannot reach yet). */
export const aiConfigSchema = v.object({
  provider: v.string(),
  model: v.string(),
  api_key: v.nullable(v.string()),
  api_key_env: v.nullable(v.string()),
  base_url: v.nullable(v.string()),
  enabled: v.boolean(),
})

export type AIConfigPayload = v.InferOutput<typeof aiConfigSchema>

// ---------------------------------------------------------------------------
// CONTRACT PENDING CODEGEN -- TODO(settings-api): delete when /settings lands
// in types.gen.ts. The backend routes (GET/PUT /settings) are future work; the
// shapes here mirror oxe/config.py's AIConfig exactly until then.
// ---------------------------------------------------------------------------

type SettingsPaths = {
  '/settings': {
    get: {
      responses: { 200: { content: { 'application/json': AIConfigPayload } } }
    }
    put: {
      requestBody: { content: { 'application/json': AIConfigPayload } }
      responses: { 200: { content: { 'application/json': AIConfigPayload } } }
    }
  }
}

const settingsClient = client as unknown as ReturnType<
  typeof createClient<SettingsPaths>
>

/** Fetch the current AI config from `GET /settings`. Throws on non-2xx. */
export async function getSettings(): Promise<AIConfigPayload> {
  const { data, error } = await settingsClient.GET('/settings')
  if (error) {
    throw new Error(`GET /settings failed (${JSON.stringify(error)})`)
  }
  return data
}

/** Persist the AI config via `PUT /settings`. Throws on non-2xx. */
export async function putSettings(payload: AIConfigPayload): Promise<AIConfigPayload> {
  const { data, error } = await settingsClient.PUT('/settings', { body: payload })
  if (error) {
    throw new Error(`PUT /settings failed (${JSON.stringify(error)})`)
  }
  return data
}

// ------------------------- end CONTRACT PENDING CODEGEN --------------------
