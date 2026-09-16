import { useId, useRef, useState } from "preact/hooks";
import { useMountEffect } from "../lib/useMountEffect";

/** Module-level models version counter. SettingsDialog calls `bumpModels()`
 * after a successful PUT /settings; every mounted useModels() refetches. */
let modelsVersion = 0;

export function bumpModels() {
  modelsVersion += 1;
  document.dispatchEvent(new CustomEvent("oxe-models-bump"));
}

export function onModelsBump(cb: () => void): () => void {
  const handler = () => cb();
  document.addEventListener("oxe-models-bump", handler);
  return () => document.removeEventListener("oxe-models-bump", handler);
}

export function currentModelsVersion(): number {
  return modelsVersion;
}

interface Props {
  models: string[];
  value: string;
  onChange: (m: string) => void;
  disabled?: boolean;
  id?: string;
  /** compact (search-bar) vs full (settings) sizing */
  size?: "xs" | "sm";
  label?: string;
  /** backend error hint from /v1/models; shown in the empty state */
  modelsError?: string | null;
}

/** Filterable model combobox (SRP: pick one model from a large list).
 * Text input filters, click/focus opens, Enter selects, Escape closes;
 * aria-combobox semantics; list is max-height + scroll (446-model lists). */
export function ModelPicker({
  models,
  value,
  onChange,
  disabled,
  id,
  size = "sm",
  label = "AI model",
  modelsError,
}: Props) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const [active, setActive] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const autoId = useId();
  const listId = `model-picker-list-${autoId}`;

  // filter on render (sub-50-item slice); no memo needed
  const filterLc = filter.trim().toLowerCase();
  const filtered = (
    filterLc ? models.filter((m) => m.toLowerCase().includes(filterLc)) : models
  ).slice(0, 50);

  useMountEffect(function closeOnOutsideClick() {
    const onDocClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  });

  const pick = (m: string) => {
    onChange(m);
    setOpen(false);
    setFilter("");
    inputRef.current?.blur();
  };

  /** Commit typed text on blur: exact (case-insensitive) match against the
   * model list selects that model; anything else is a custom model id. */
  const commit = () => {
    const t = filter.trim();
    if (!t || t === value) {
      setOpen(false);
      setFilter("");
      return;
    }
    const exact = models.find((m) => m.toLowerCase() === t.toLowerCase());
    pick(exact ?? t);
  };

  const h = size === "xs" ? "select-xs" : "select-sm";
  return (
    <div class="relative" ref={rootRef}>
      <div role="combobox" aria-expanded={open} aria-haspopup="listbox" aria-owns={listId}>
        <input
          ref={inputRef}
          id={id}
          type="text"
          role="searchbox"
          aria-label={label}
          aria-autocomplete="list"
          aria-controls={listId}
          autocomplete="off"
          class={`input ${h} w-full pr-6 min-w-0`}
          value={open ? filter : value}
          placeholder={value || "filter models…"}
          disabled={disabled}
          onInput={(e) => {
            const v = (e.target as HTMLInputElement).value;
            setFilter(v);
            setOpen(true);
            setActive(0);
          }}
          onFocus={() => {
            setFilter("");
            setOpen(true);
            setActive(0);
          }}
          onBlur={commit}
          onKeyDown={(e) => {
            const ke = e as unknown as KeyboardEvent;
            if (ke.key === "ArrowDown") {
              e.preventDefault();
              setOpen(true);
              setActive((i) => Math.min(i + 1, filtered.length - 1));
            } else if (ke.key === "ArrowUp") {
              e.preventDefault();
              setActive((i) => Math.max(i - 1, 0));
            } else if (ke.key === "Enter" && open && filtered[active]) {
              e.preventDefault();
              pick(filtered[active]);
            } else if (ke.key === "Escape") {
              setOpen(false);
              setFilter("");
            }
          }}
        />
        <span
          class="absolute right-2 top-1/2 -translate-y-1/2 text-[10px] opacity-40 pointer-events-none select-none"
          aria-hidden="true"
        >
          ▾
        </span>
      </div>
      {open && filtered.length === 0 && (
        <ul
          id={listId}
          role="listbox"
          aria-label={label}
          class="absolute left-0 right-0 top-full mt-1 z-50 bg-base-100 border border-base-300 rounded-md shadow-sm py-2 text-xs m-0 list-none p-0"
        >
          <li role="option" aria-selected={false} aria-disabled="true" class="px-3 opacity-60">
            {modelsError
              ? `Model listing failed: ${modelsError}`
              : "No models - check provider / API key in settings"}
          </li>
        </ul>
      )}
      {open && filtered.length > 0 && (
        <ul
          id={listId}
          role="listbox"
          aria-label={label}
          class="absolute left-0 right-0 top-full mt-1 z-50 bg-base-100 border border-base-300 rounded-md shadow-sm py-1 text-xs m-0 list-none p-0 max-h-64 overflow-y-auto"
        >
          {filtered.map((m, i) => (
            <li
              key={m}
              role="option"
              aria-selected={m === value}
              class={`px-3 py-1.5 cursor-pointer truncate ${i === active ? "bg-base-200" : ""}`}
              onMouseDown={(e) => {
                e.preventDefault();
                pick(m);
              }}
              onMouseEnter={() => setActive(i)}
            >
              {m}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
