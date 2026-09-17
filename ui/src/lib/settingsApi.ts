/**
 * Typed client wrappers for `GET /settings` and `PUT /settings`, typed from the
 * generated spec (`paths['/settings']` in `./types.gen.ts`, which mirrors
 * `oxe/api/settings.py`). Layer-1 only: openapi-fetch already types the
 * responses, so there is no runtime parsing or valibot schema here.
 */
import { client } from './api.ts'
import type { components } from './types.gen.ts'

export type SettingsPayload = components['schemas']['SettingsPayload']
export type SettingsWritePayload = components['schemas']['SettingsWritePayload']

/** Fetch the current AI config from `GET /settings`. Throws on non-2xx. */
export async function getSettings(): Promise<SettingsPayload> {
  const { data, error } = await client.GET('/settings')
  if (error) {
    throw new Error(`GET /settings failed (${JSON.stringify(error)})`)
  }
  return data
}

/** Persist the AI config via `PUT /settings`. Throws on non-2xx. */
export async function putSettings(payload: SettingsWritePayload): Promise<SettingsPayload> {
  const { data, error } = await client.PUT('/settings', { body: payload })
  if (error) {
    throw new Error(`PUT /settings failed (${JSON.stringify(error)})`)
  }
  return data
}
