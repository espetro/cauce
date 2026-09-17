import { useRef, useState } from "preact/hooks";
import { SuggestionsDropdown } from "./SuggestionsDropdown";
import { useListNav, useSuggests } from "./useSuggests";
import { AiControls, ModeSegments, type Mode } from "../../components/ModeSegments";
import { useMountEffect } from "../../lib/useMountEffect";
import * as m from "../../lib/i18n";

interface Props {
  value: string;
  onInput: (v: string) => void;
  onSubmit: (v: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
  busy?: boolean;
  size?: "lg" | "md";
  ariaLabel?: string;
  /** Pill mode state (landing + search). Absent = plain search input. */
  mode?: Mode;
  onModeChange?: (m: Mode) => void;
  aiAvailable?: boolean | null;
  models?: string[];
  /** /v1/models failure hint, surfaced in the model picker empty state */
  modelsError?: string | null;
}

/** DDG-style pill search bar: rounded-full container, inline segmented
 * mode toggle at the right end, and (AI mode) a second action row that
 * reveals via a smooth morphism. Suggestions stay anchored to the pill.
 * Does not fetch (the suggests hook owns that) and does not navigate. */
export function SearchBox({
  value,
  onInput,
  onSubmit,
  placeholder,
  autoFocus,
  busy,
  size = "lg",
  ariaLabel = m.searchbox_aria_search(),
  mode,
  onModeChange,
  aiAvailable,
  models = [],
  modelsError,
}: Props) {
  const [open, setOpen] = useState(false);
  const [focused, setFocused] = useState(false);
  const boxRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const typedRef = useRef(value);
  typedRef.current = value;

  const aiMode = mode === "ai";
  const ph = placeholder ?? (aiMode ? m.searchbox_ph_ai() : m.searchbox_ph_traditional());

  // suggestions are optional in AI mode; suppress them there (less noise)
  const { items, acOn, setAcOn } = useSuggests(aiMode ? "" : value, open);
  const { activeIndex, handleKey, setActiveIndex } = useListNav(
    items.length,
    (i) => {
      const t = items[i]?.text;
      if (t) {
        setOpen(false);
        setActiveIndex(null);
        onInput(t);
        onSubmit(t);
      }
    },
    (i) => {
      const t = items[i]?.text;
      if (t) {
        onInput(t);
        inputRef.current?.focus();
      }
    },
    () => {
      setOpen(false);
      setActiveIndex(null);
      onInput(typedRef.current);
    },
  );

  useMountEffect(function closeOnOutsideClick() {
    const onDocClick = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  });

  const showDropdown = open && value.trim().length >= 2 && items.length > 0;

  const input = (
    <input
      ref={inputRef}
      type="search"
      name="q"
      enterkeyhint="search"
      autofocus={autoFocus}
      class="grow bg-transparent outline-none min-w-0"
      placeholder={ph}
      aria-label={ariaLabel}
      aria-autocomplete="list"
      aria-expanded={showDropdown}
      aria-controls="suggest-listbox"
      autocomplete="off"
      value={value}
      onInput={(e) => {
        onInput((e.target as HTMLInputElement).value);
        setOpen(true);
        setActiveIndex(null);
      }}
      onFocus={() => {
        setOpen(true);
        setFocused(true);
      }}
      onBlur={() => setFocused(false)}
      onKeyDown={(e) => {
        if (showDropdown) handleKey(e as unknown as KeyboardEvent);
        else if (e.key === "Escape") (e.target as HTMLInputElement).blur();
      }}
    />
  );

  const submitBtn = (
    <button
      type="submit"
      class="btn btn-ghost btn-sm btn-circle shrink-0"
      aria-label={m.searchbox_aria_submit()}
      disabled={busy}
    >
      {busy ? (
        <span class="loading loading-dots loading-xs" />
      ) : (
        <svg
          width="16"
          height="16"
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
      )}
    </button>
  );

  return (
    <div class="relative w-full min-w-0 max-w-[min(672px,calc(100vw-48px))]" ref={boxRef}>
      <form
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          setOpen(false);
          const q = value.trim();
          if (q) onSubmit(q);
        }}
      >
        <div
          class={`w-full bg-base-100 border border-base-300 rounded-[28px] overflow-hidden
            transition-[box-shadow,border-color] duration-200 ease-out
            ${focused ? "oxe-pill-focus" : "shadow-none"}
            ${size === "lg" ? "px-4 py-2" : "px-3 py-1.5"}`}
        >
          <div class={`flex items-center gap-1.5 ${size === "lg" ? "min-h-10" : "min-h-8"}`}>
            {input}
            {mode && onModeChange ? (
              <ModeSegments mode={mode} onChange={onModeChange} aiAvailable={aiAvailable ?? null} />
            ) : null}
            {submitBtn}
          </div>
          {mode === "ai" && (
            <div class="ai-row-in border-t border-base-200 mt-1.5 pt-1.5">
              <AiControls
                available={aiAvailable ?? null}
                models={models}
                modelsError={modelsError}
                busy={busy}
              />
            </div>
          )}
        </div>
      </form>
      {showDropdown && (
        <div id="suggest-listbox">
          <SuggestionsDropdown
            items={items}
            activeIndex={activeIndex}
            onPick={(text) => {
              setOpen(false);
              setActiveIndex(null);
              onInput(text);
              onSubmit(text);
            }}
            onHover={(i) => setActiveIndex(i)}
            acOn={acOn}
            setAcOn={setAcOn}
          />
        </div>
      )}
    </div>
  );
}
