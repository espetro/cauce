import { useEffect, useRef, useState } from "preact/hooks";
import * as v from "valibot";
import { listModels } from "../../lib/ai";
import { bumpModels, ModelPicker } from "../../components/ModelPicker";
import { getTheme, setTheme, THEMES, type ThemeChoice } from "../../lib/theme";
import { getSettings, putSettings, PROVIDERS, SettingsSchema, type SettingsValues } from "./schema";

/** Settings dialog: reads/writes the backend [ai] config via GET/PUT /settings.
 * Uncontrolled form, parse-on-submit via valibot. */
export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const [models, setModels] = useState<string[]>([]);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Partial<Record<keyof SettingsValues, string>>>({});
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const formRef = useRef<HTMLFormElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const [model, setModel] = useState("");
  const [theme, setThemeState] = useState<ThemeChoice>(getTheme);

  useEffect(() => {
    const ctl = new AbortController();
    getSettings().catch((e: unknown) => setLoadError((e as Error)?.message ?? "load failed"));
    listModels(ctl.signal)
      .then((m) => {
        setModels(m.data.map((d) => d.id));
        setModelsError(m.error ?? null);
        getSettings()
          .then((s) => s.ai?.model && setModel(s.ai.model))
          .catch(() => undefined);
      })
      .catch(() => setModels([]));
    return () => ctl.abort();
  }, []);

  useEffect(() => {
    dialogRef.current?.querySelector<HTMLInputElement>("select, input")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const submit = (e: Event) => {
    e.preventDefault();
    setSaveError(null);
    setSaved(false);
    const fd = new FormData(formRef.current!);
    const raw = {
      provider: String(fd.get("provider") ?? "") as SettingsValues["provider"],
      model: model.trim(),
      api_key: String(fd.get("api_key") ?? "").trim() || undefined,
      base_url: String(fd.get("base_url") ?? "").trim() || undefined,
      enabled: fd.get("enabled") === "on",
    };
    const parsed = v.safeParse(SettingsSchema, raw);
    if (!parsed.success) {
      const errs: typeof fieldErrors = {};
      for (const issue of parsed.issues) {
        const key = issue.path?.[0]?.key as keyof SettingsValues | undefined;
        if (key && !errs[key]) errs[key] = issue.message;
      }
      setFieldErrors(errs);
      return;
    }
    setFieldErrors({});
    setSaving(true);
    putSettings({
      provider: parsed.output.provider,
      model: parsed.output.model,
      api_key: parsed.output.api_key ?? null,
      base_url: parsed.output.base_url ?? null,
      enabled: parsed.output.enabled,
    })
      .then(() => {
        setSaving(false);
        setSaved(true);
        bumpModels();
        setTimeout(onClose, 400);
      })
      .catch((err: unknown) => {
        setSaving(false);
        setSaveError((err as Error)?.message ?? "save failed");
      });
  };

  const err = (k: keyof SettingsValues) =>
    fieldErrors[k] ? <p class="text-error text-xs mt-1">{fieldErrors[k]}</p> : null;

  return (
    <div
      class="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label="settings"
        class="card bg-base-100 border border-base-300 w-full max-w-md"
      >
        <div class="card-body gap-3">
          <h2 class="card-title text-base">settings</h2>
          {loadError && (
            <p class="text-error text-sm">
              backend settings endpoints not available ({loadError}) - the server needs GET/PUT
              /settings support
            </p>
          )}
          <form ref={formRef} onSubmit={submit} noValidate>
            <fieldset class="fieldset gap-2.5">
              <legend class="fieldset-legend text-sm">ai</legend>

              <label class="label text-xs" for="set-provider">
                provider
              </label>
              <select
                id="set-provider"
                name="provider"
                class="select select-sm w-full"
                defaultValue="openai"
              >
                {PROVIDERS.map((p) => (
                  <option key={p} value={p}>
                    {p}
                  </option>
                ))}
              </select>
              {err("provider")}

              <label class="label text-xs" for="set-model">
                model
              </label>
              <ModelPicker
                id="set-model"
                models={models}
                value={model}
                onChange={setModel}
                size="sm"
                modelsError={modelsError}
              />
              {err("model")}
              {modelsError && models.length === 0 && (
                <p class="text-warning text-xs mt-1" role="note">
                  model listing failed: {modelsError}
                </p>
              )}

              <label class="label text-xs" for="set-api-key">
                api key
              </label>
              <input
                id="set-api-key"
                name="api_key"
                type="password"
                class="input input-sm w-full"
                placeholder="(unchanged if blank)"
                autocomplete="off"
              />
              {err("api_key")}

              <label class="label text-xs" for="set-base-url">
                base url
              </label>
              <input
                id="set-base-url"
                name="base_url"
                type="url"
                class="input input-sm w-full"
                placeholder="https://api.openai.com/v1"
                autocomplete="off"
              />
              {err("base_url")}

              <label class="label cursor-pointer gap-2 text-xs justify-start">
                <input type="checkbox" name="enabled" class="toggle toggle-sm" defaultChecked />
                enabled
              </label>
            </fieldset>

            <fieldset class="fieldset gap-2.5 mt-2">
              <legend class="fieldset-legend text-sm">theme</legend>
              <div role="radiogroup" aria-label="theme" class="join">
                {THEMES.map((t) => (
                  <button
                    key={t}
                    type="button"
                    role="radio"
                    aria-checked={theme === t}
                    tabIndex={theme === t ? 0 : -1}
                    class={`btn join-item btn-sm ${theme === t ? "btn-primary" : "btn-ghost"}`}
                    onClick={() => {
                      setThemeState(t);
                      setTheme(t);
                    }}
                  >
                    {t === "system" ? "System" : t === "light" ? "Light" : "Dark"}
                  </button>
                ))}
              </div>
              {theme === "system" && (
                <p class="text-xs opacity-50">follows your OS light/dark preference</p>
              )}
            </fieldset>

            {saveError && <p class="text-error text-xs mt-2">{saveError}</p>}

            <div class="card-actions justify-end mt-3">
              <button type="button" class="btn btn-ghost btn-sm" onClick={onClose}>
                cancel
              </button>
              <button type="submit" class="btn btn-primary btn-sm" disabled={saving}>
                {saving ? (
                  <span class="loading loading-dots loading-xs" />
                ) : saved ? (
                  "saved"
                ) : (
                  "save"
                )}
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
