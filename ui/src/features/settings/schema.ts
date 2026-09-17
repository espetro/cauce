import * as v from "valibot";
import { request } from "../../lib/api";
import { SettingsGetSchema, SettingsPutSchema, type SettingsGet } from "../../lib/schemas";

/** [ai] config fields per oxe config.toml schema. */
export const SettingsSchema = v.object({
  provider: v.picklist(["openai", "anthropic", "groq", "mistral"] as const, "pick a provider"),
  model: v.pipe(v.string(), v.trim(), v.nonEmpty("model is required")),
  api_key: v.optional(v.pipe(v.string(), v.trim())),
  base_url: v.optional(v.pipe(v.string(), v.trim(), v.url("must be a valid url"))),
  enabled: v.boolean(),
});

export type SettingsValues = v.InferInput<typeof SettingsSchema>;

export const PROVIDERS = ["openai", "anthropic", "groq", "mistral"] as const;

export type { SettingsGet };

export interface SettingsPut {
  provider: string;
  model: string;
  api_key?: string | null;
  base_url?: string | null;
  enabled: boolean;
}

export async function getSettings(signal?: AbortSignal): Promise<SettingsGet> {
  return request("/settings", SettingsGetSchema, { signal });
}

export async function putSettings(body: SettingsPut): Promise<void> {
  // backend expects the [ai] section nested under an "ai" key
  await request("/settings", SettingsPutSchema, {
    method: "PUT",
    body: { ai: body },
  });
}
