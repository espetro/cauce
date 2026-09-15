import * as v from "valibot";

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

export interface SettingsGet {
  ai: {
    provider: string;
    model: string;
    api_key: string | null; // redacted
    base_url: string | null;
    enabled: boolean;
    api_key_env?: string | null;
  } | null;
}

export interface SettingsPut {
  provider: string;
  model: string;
  api_key?: string | null;
  base_url?: string | null;
  enabled: boolean;
}

export async function getSettings(signal?: AbortSignal): Promise<SettingsGet> {
  const res = await fetch(`/settings`, { signal });
  if (!res.ok) throw new Error(`GET /settings failed: ${res.status}`);
  return (await res.json()) as SettingsGet;
}

export async function putSettings(body: SettingsPut): Promise<void> {
  const res = await fetch(`/settings`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    let detail = `${res.status}`;
    try {
      const j = (await res.json()) as { detail?: string };
      if (j?.detail) detail = j.detail;
    } catch {
      // non-json error
    }
    throw new Error(detail);
  }
}
