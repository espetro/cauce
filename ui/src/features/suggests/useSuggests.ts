import { useEffect, useRef, useState } from "preact/hooks";
import { ddgAc, suggest } from "../../lib/api";
export const AC_KEY = "oxe-ac";

export interface Suggestion {
  text: string;
  group: "history" | "web";
}

/** History-first suggestions from the local GET /suggest endpoint,
 * followed by DDG web suggestions via the backend GET /ac proxy
 * (gated by the oxe-ac localStorage toggle). */
export function useSuggests(
  query: string,
  open: boolean,
): {
  items: Suggestion[];
  acOn: boolean;
  setAcOn: (v: boolean) => void;
} {
  const [history, setHistory] = useState<string[]>([]);
  const [web, setWeb] = useState<string[]>([]);
  const [acOn, setAcOnState] = useState(
    () => typeof localStorage === "undefined" || localStorage.getItem(AC_KEY) !== "off",
  );

  // One consolidated fetch pass: history suggestions are immediate, web
  // suggestions debounced 300ms behind the same query; a single timer +
  // AbortController pair per keystroke guards stale races for both.
  useEffect(
    function fetchSuggestions() {
      const v = query.trim().toLowerCase();
      if (!open || v.length < 2) {
        setHistory([]);
        setWeb([]);
        return;
      }
      const webCtlRef = { current: null as AbortController | null };
      const t = setTimeout(() => {
        const ctl = new AbortController();
        webCtlRef.current = ctl;
        if (acOn) {
          ddgAc(v, ctl.signal)
            .then(setWeb)
            .catch(() => {});
        } else {
          setWeb([]);
        }
      }, 300);
      const ctl = new AbortController();
      suggest(v, ctl.signal)
        .then(setHistory)
        .catch(() => {});
      return function cancelSuggestions() {
        clearTimeout(t);
        ctl.abort();
        webCtlRef.current?.abort();
      };
    },
    [query, open, acOn],
  );

  const setAcOn = (v: boolean) => {
    setAcOnState(v);
    localStorage.setItem(AC_KEY, v ? "on" : "off");
  };

  const items = mergeSuggests(history, web);
  return { items, acOn, setAcOn };
}

export const GROUP_LABEL: Record<Suggestion["group"], string> = {
  history: "your history",
  web: "web suggestions",
};

/** Merge history entries, dedup case-insensitively, most recent first. */
export function mergeSuggests(history: string[], web: string[] = []): Suggestion[] {
  const seen = new Set<string>();
  const items: Suggestion[] = [];
  for (const text of history.slice(0, 3)) {
    const k = text.trim().toLowerCase();
    if (k && !seen.has(k)) {
      seen.add(k);
      items.push({ text, group: "history" });
    }
  }
  for (const text of web.slice(0, 4)) {
    const k = text.trim().toLowerCase();
    if (k && !seen.has(k)) {
      seen.add(k);
      items.push({ text, group: "web" });
    }
  }
  return items;
}

export interface DropdownHandle {
  activeIndex: number | null;
  onKey: (e: KeyboardEvent) => void;
}

export function useListNav(
  count: number,
  onPick: (i: number) => void,
  onFill: (i: number) => void,
  onClose: () => void,
): {
  activeIndex: number | null;
  handleKey: (e: KeyboardEvent) => void;
  setActiveIndex: (i: number | null) => void;
} {
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const ref = useRef({ count, onPick, onFill, onClose });
  ref.current = { count, onPick, onFill, onClose };
  const handleKey = (e: KeyboardEvent) => {
    const { count: n, onPick: pick, onFill: fill, onClose: close } = ref.current;
    if (!n) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => (i == null ? 0 : (i + 1) % n));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) => (i == null ? n - 1 : (i - 1 + n) % n));
    } else if (e.key === "Enter" && activeIndex != null && activeIndex < n) {
      e.preventDefault();
      pick(activeIndex);
    } else if (
      (e.key === "Tab" || e.key === "ArrowRight") &&
      activeIndex != null &&
      activeIndex < n
    ) {
      e.preventDefault();
      fill(activeIndex);
    } else if (e.key === "Escape") {
      close();
    }
  };
  return { activeIndex, handleKey, setActiveIndex };
}
