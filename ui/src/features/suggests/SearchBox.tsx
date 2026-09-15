import { useEffect, useRef, useState } from "preact/hooks";
import { SuggestionsDropdown } from "./SuggestionsDropdown";
import { useListNav, useSuggests } from "./useSuggests";

interface Props {
  value: string;
  onInput: (v: string) => void;
  onSubmit: (v: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
  busy?: boolean;
  size?: "lg" | "md";
  ariaLabel?: string;
}

/** Controlled search input wired with the suggestions feature.
 * Does not fetch (the suggests hook owns that) and does not navigate. */
export function SearchBox({
  value,
  onInput,
  onSubmit,
  placeholder = "search the web…",
  autoFocus,
  busy,
  size = "lg",
  ariaLabel = "search",
}: Props) {
  const [open, setOpen] = useState(false);
  const boxRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const typedRef = useRef(value);
  typedRef.current = value;

  const { items, acOn, setAcOn } = useSuggests(value, open);
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

  useEffect(() => {
    const onDocClick = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, []);

  const showDropdown = open && value.trim().length >= 2 && items.length > 0;

  return (
    <div
      class={`relative w-full ${size === "lg" ? "max-w-[560px]" : "max-w-[560px]"}`}
      ref={boxRef}
    >
      <form
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          setOpen(false);
          const q = value.trim();
          if (q) onSubmit(q);
        }}
      >
        <label
          class={`${size === "lg" ? "input input-lg" : "input"} w-full flex items-center gap-2 bg-base-100`}
        >
          <input
            ref={inputRef}
            type="search"
            name="q"
            enterkeyhint="search"
            autofocus={autoFocus}
            class="grow bg-transparent outline-none min-w-0"
            placeholder={placeholder}
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
            onFocus={() => setOpen(true)}
            onKeyDown={(e) => {
              if (showDropdown) handleKey(e as unknown as KeyboardEvent);
              else if (e.key === "Escape") (e.target as HTMLInputElement).blur();
            }}
          />
          <button
            type="submit"
            class="btn btn-ghost btn-sm"
            aria-label="submit search"
            disabled={busy}
          >
            {busy ? (
              <span class="loading loading-dots loading-xs" />
            ) : (
              <span aria-hidden="true">→</span>
            )}
          </button>
        </label>
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
            onToggleAc={setAcOn}
          />
        </div>
      )}
    </div>
  );
}
