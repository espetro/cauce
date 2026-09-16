import { useEffect, useRef, useState } from "preact/hooks";
import * as v from "valibot";
import { listModels, testConnection } from "../../lib/ai";
import { bumpModels, ModelPicker } from "../../components/ModelPicker";
import { toast } from "../../components/Toasts";
import { getTheme, setTheme, THEMES, type ThemeChoice } from "../../lib/theme";
import { getSettings, putSettings, PROVIDERS, SettingsSchema, type SettingsValues } from "./schema";

/** Settings dialog: native <dialog class="modal"> + <form method="dialog">.
 * Reads/writes the backend [ai] config via GET/PUT /settings; parse-on-submit
 * via valibot (save PUTs then closes the dialog). api_key is redacted
 * server-side so it stays blank ("unchanged if blank"). Esc and backdrop
 * clicks close the native dialog for free; the close event notifies the
 * parent so it strips the ?settings param. */
export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const [models, setModels] = useState<string[]>([]);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Partial<Record<keyof SettingsValues, string>>>({});
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; detail: string } | null>(null);
  const formRef = useRef<HTMLFormElement>(null);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [model, setModel] = useState("");
  const [provider, setProvider] = useState<string>("openai");
  const [theme, setThemeState] = useState<ThemeChoice>(getTheme);

  // hydrate saved config (provider/model drive the controlled pickers)
  useEffect(() => {
    const ctl = new AbortController();
    const setField = (name: string, value: string) => {
      const el = formRef.current?.elements?.namedItem(name);
      if (el instanceof HTMLInputElement || el instanceof HTMLSelectElement) {
        el.value = value;
      }
    };
    getSettings(ctl.signal)
      .then((s) => {
        const ai = s.ai;
        if (!ai) return;
        if (PROVIDERS.includes(ai.provider as (typeof PROVIDERS)[number])) {
          setProvider(ai.provider);
          setField("provider", ai.provider);
        }
        if (ai.model) setModel(ai.model);
        if (ai.base_url) setField("base_url", ai.base_url);
        const enabled = formRef.current?.elements?.namedItem("enabled");
        if (enabled instanceof HTMLInputElement) enabled.checked = ai.enabled;
      })
      .catch((e: unknown) => {
        if ((e as Error)?.name !== "AbortError")
          setLoadError((e as Error)?.message ?? "load failed");
      });
    listModels(ctl.signal)
      .then((m) => {
        setModels(m.data.map((d) => d.id));
        setModelsError(m.error ?? null);
      })
      .catch(() => setModels([]));
    return () => ctl.abort();
  }, []);

  // open + focus management; the close event (Esc, backdrop, cancel button,
  // save) bubbles to the parent which strips the ?settings param.
  useEffect(
    function wireDialogOnMount() {
      const dlg = dialogRef.current;
      if (!dlg) return;
      dlg.showModal();
      dlg.addEventListener("close", onClose);
      dlg.querySelector<HTMLInputElement>("select, input")?.focus();
      return () => dlg.removeEventListener("close", onClose);
    },
    [onClose],
  );

  const submit = (e: Event) => {
    e.preventDefault();
    setSaveError(null);
    setTestResult(null);
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
        bumpModels();
        toast("success", "Settings saved");
        dialogRef.current?.close();
      })
      .catch((err: unknown) => {
        setSaving(false);
        const msg = (err as Error).message ?? "save failed";
        setSaveError(msg);
        toast("error", `Settings save failed: ${msg}`);
      });
  };

  /** Verify the form's provider/model/key/base_url with POST /settings/test. */
  const runTest = () => {
    if (testing) return;
    setTestResult(null);
    setSaveError(null);
    const fd = new FormData(formRef.current!);
    const provider = String(fd.get("provider") ?? "");
    const modelTrimmed = model.trim();
    const baseUrl = String(fd.get("base_url") ?? "").trim();
    const apiKey = String(fd.get("api_key") ?? "").trim();
    if (!modelTrimmed) {
      setTestResult({ ok: false, detail: "Pick a model first" });
      return;
    }
    setTesting(true);
    testConnection({
      provider,
      model: modelTrimmed,
      base_url: baseUrl || undefined,
      api_key: apiKey || undefined,
    })
      .then((r) => {
        setTestResult({ ok: Boolean(r.ok), detail: r.detail });
        toast(
          r.ok ? "success" : "error",
          r.detail || (r.ok ? "Connection ok" : "Connection failed"),
        );
      })
      .catch((err: unknown) => {
        const detail = (err as Error).message ?? "test failed";
        setTestResult({ ok: false, detail });
        toast("error", detail);
      })
      .finally(() => setTesting(false));
  };

  const err = (k: keyof SettingsValues) =>
    fieldErrors[k] ? <p class="text-error text-xs mt-1">{fieldErrors[k]}</p> : null;

  return (
    <dialog ref={dialogRef} class="modal" aria-label="Settings">
      <div class="modal-box w-full max-w-md animate-in fade-in zoom-in-95 duration-150">
        <h2 class="text-base font-semibold mb-3">Settings</h2>
        {loadError && (
          <div role="alert" class="alert alert-error text-sm mb-3">
            Backend settings endpoints not available ({loadError}) - the server needs GET/PUT
            /settings support
          </div>
        )}
        <form ref={formRef} onSubmit={submit} noValidate>
          <fieldset class="fieldset gap-2.5">
            <legend class="fieldset-legend text-sm">AI</legend>

            <label class="label text-xs" for="set-provider">
              Provider
            </label>
            <select
              id="set-provider"
              name="provider"
              class="select select-sm w-full"
              value={provider}
              onInput={(e) => setProvider((e.target as HTMLSelectElement).value)}
            >
              {PROVIDERS.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
            {err("provider")}

            <label class="label text-xs" for="set-model">
              Model
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
                Model listing failed: {modelsError}
              </p>
            )}

            <label class="label text-xs" for="set-api-key">
              API key
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
              Base URL
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

            <div class="flex items-center gap-2 mt-1">
              <button
                type="button"
                class="btn btn-outline btn-sm"
                disabled={testing}
                onClick={runTest}
              >
                {testing ? <span class="loading loading-spinner loading-xs" /> : "Test connection"}
              </button>
              {testResult && (
                <span
                  class={`text-xs ${testResult.ok ? "text-success" : "text-error"}`}
                  role="status"
                >
                  {testResult.detail}
                </span>
              )}
            </div>

            <label class="label cursor-pointer gap-2 text-xs justify-start">
              <input type="checkbox" name="enabled" class="toggle toggle-sm" defaultChecked />
              Enabled
            </label>
          </fieldset>

          <fieldset class="fieldset gap-2.5 mt-2">
            <legend class="fieldset-legend text-sm">Theme</legend>
            <div role="radiogroup" aria-label="Theme" class="join">
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
              <p class="text-xs opacity-50">Follows your OS light/dark preference</p>
            )}
          </fieldset>

          {saveError && (
            <div role="alert" class="alert alert-error text-xs mt-2">
              <span>{saveError}</span>
            </div>
          )}

          <div class="modal-action">
            <button
              type="button"
              class="btn btn-ghost btn-sm"
              onClick={() => dialogRef.current?.close()}
            >
              Cancel
            </button>
            <button type="submit" class="btn btn-primary btn-sm" disabled={saving}>
              {saving ? <span class="loading loading-dots loading-xs" /> : "Save"}
            </button>
          </div>
        </form>
      </div>
      <form method="dialog" class="modal-backdrop">
        <button aria-label="Close settings">close</button>
      </form>
    </dialog>
  );
}
