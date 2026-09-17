import { useEffect, useState } from "preact/hooks";
import { listModels, type ModelsResponse } from "../lib/ai";
import { currentModelsVersion, ModelPicker, onModelsBump } from "./ModelPicker";
import * as m from "../lib/i18n";
export type Mode = "traditional" | "ai";

/** Models + AI availability from GET /v1/models.
 * Refetches when settings saves bump the models version (bumpModels()).
 * `available` is tri-state: null = still querying (never demote AI on null). */
export function useModels(): {
  available: boolean | null;
  models: string[];
  error: string | null;
} {
  const [available, setAvailable] = useState<boolean | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(currentModelsVersion);
  useEffect(() => onModelsBump(() => setVersion(currentModelsVersion())), []);
  useEffect(() => {
    const ctl = new AbortController();
    listModels(ctl.signal)
      .then((m: ModelsResponse) => {
        setAvailable(Boolean(m.ai_available));
        setModels(m.data.map((d) => d.id));
        setError(m.error ?? null);
      })
      .catch(() => setAvailable(false));
    return () => ctl.abort();
  }, [version]);
  return { available, models, error };
}

const MODE_KEY = "oxe-mode";

/** Single source of truth for the search mode: the URL `mode` param when
 * present, else the persisted localStorage preference, else "traditional".
 * Both routes (/, /search) use this so the toggle and the rendered layout
 * always agree from the first paint (fixes the reload divergence where the
 * layout came from localStorage but the toggle from the absent URL param).
 * Setters persist at event time; routes own their URL updates. */
export function useSearchMode(): [Mode, (m: Mode) => void] {
  const [mode, setMode] = useState<Mode>(() => {
    if (typeof window !== "undefined") {
      const url = new URLSearchParams(window.location.search).get("mode");
      if (url === "ai" || url === "traditional") return url;
    }
    if (typeof localStorage !== "undefined" && localStorage.getItem(MODE_KEY) === "ai") {
      return "ai";
    }
    return "traditional";
  });
  const setModeAndStore = (m: Mode) => {
    setMode(m);
    localStorage.setItem(MODE_KEY, m);
  };
  return [mode, setModeAndStore];
}

const SEGMENTS: Array<{ v: Mode; label: () => string }> = [
  { v: "traditional", label: m.segments_search },
  { v: "ai", label: m.mode_label_ai },
];

const Magnifier = () => (
  <svg
    width="13"
    height="13"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    aria-hidden="true"
  >
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </svg>
);

const Sparkle = () => (
  <svg
    width="13"
    height="13"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path d="M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9L12 3z" />
  </svg>
);

/** DDG-style inline segmented mode toggle at the right end of the pill:
 * light track, active segment is a white pill with a small shadow.
 * radiogroup semantics, arrow keys switch segments. */
export function ModeSegments({
  mode,
  onChange,
  aiAvailable,
}: {
  mode: Mode;
  onChange: (m: Mode) => void;
  aiAvailable: boolean | null;
}) {
  const aiDisabled = aiAvailable === false;
  const move = (dir: 1 | -1) => {
    const i = SEGMENTS.findIndex((s) => s.v === mode);
    const next = SEGMENTS[(i + dir + SEGMENTS.length) % SEGMENTS.length];
    if (!(next.v === "ai" && aiDisabled)) onChange(next.v);
  };
  return (
    <div
      role="radiogroup"
      aria-label={m.mode_aria_label_lower()}
      class="join bg-base-200 rounded-full p-0.5 shrink-0"
    >
      {SEGMENTS.map((s) => {
        const disabled = s.v === "ai" && aiDisabled;
        const active = mode === s.v;
        return (
          <span
            key={s.v}
            class="tooltip tooltip-bottom"
            data-tip={disabled ? m.mode_tip_ai_disabled_short() : undefined}
          >
            <button
              type="button"
              role="radio"
              aria-checked={active}
              aria-disabled={disabled || undefined}
              disabled={disabled}
              tabIndex={active ? 0 : -1}
              class={`btn join-item btn-xs rounded-full border-0 ${
                active
                  ? "bg-base-100 shadow-sm font-medium"
                  : "bg-transparent opacity-60 hover:opacity-100"
              } ${disabled ? "btn-disabled opacity-30" : ""}`}
              onClick={() => {
                if (!disabled) onChange(s.v);
              }}
              onKeyDown={(e) => {
                if (e.key === "ArrowRight" || e.key === "ArrowDown") {
                  e.preventDefault();
                  move(1);
                } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
                  e.preventDefault();
                  move(-1);
                }
              }}
            >
              {s.v === "traditional" ? <Magnifier /> : <Sparkle />}
              {s.label()}
            </button>
          </span>
        );
      })}
    </div>
  );
}

const STORE_KEY = "oxe-ai-model";
const REASONING_KEY = "oxe-ai-reasoning";

/** AI second-row controls: model picker + reasoning toggle chip.
 * Pure UI state (localStorage); request wiring is a backend concern.
 * `busy` (answer run in flight) disables the reasoning toggle: switching it
 * mid-run has no effect on the stream and reads as a broken control. */
export function AiControls({
  available,
  models,
  modelsError,
  busy,
}: {
  available: boolean | null;
  models: string[];
  modelsError?: string | null;
  busy?: boolean;
}) {
  const ls = () => (typeof localStorage === "undefined" ? null : localStorage);
  const [storedModel, setStoredModel] = useState(() => ls()?.getItem(STORE_KEY) ?? "");
  const [reasoning, setReasoningState] = useState(() => ls()?.getItem(REASONING_KEY) === "1");

  // Persist at event time. Fall back to the first model when the stored
  // value is empty or a ghost (stale) entry; derive, don't effect.
  const model = storedModel && models.includes(storedModel) ? storedModel : (models[0] ?? "");
  const setModel = (m: string) => {
    setStoredModel(m);
    ls()?.setItem(STORE_KEY, m);
  };
  const setReasoning = (r: boolean) => {
    setReasoningState(Boolean(r));
    ls()?.setItem(REASONING_KEY, r ? "1" : "0");
  };

  return (
    <div class="flex flex-col sm:flex-row sm:items-center gap-1.5 sm:gap-2 w-full text-xs min-w-0">
      <div class="flex items-center gap-1.5 min-w-0 flex-1">
        <span class="shrink-0 opacity-70">{m.ai_label_model()}</span>
        <ModelPicker
          models={models}
          value={model}
          onChange={setModel}
          disabled={available === false || models.length === 0}
          size="xs"
          modelsError={modelsError}
        />
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={reasoning}
        aria-disabled={busy || undefined}
        disabled={busy}
        class={`btn btn-xs oxe-pill-control shrink-0 self-start sm:self-auto border ${reasoning ? "btn-primary btn-soft" : "btn-ghost"} ${busy ? "btn-disabled opacity-40" : ""}`}
        onClick={() => {
          if (!busy) setReasoning(!reasoning);
        }}
      >
        {m.ai_label_reasoning()}
      </button>
    </div>
  );
}
