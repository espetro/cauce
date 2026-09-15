import { toChildArray, type ComponentChildren, type JSX } from "preact";
import { useEffect, useState } from "preact/hooks";
import { listModels, type ModelsResponse } from "../lib/ai";
import { SettingsDialog } from "../features/settings/SettingsDialog";

interface NavItem {
  href: string;
  label: string;
  active?: boolean;
  exact?: boolean;
}

const NAV: NavItem[] = [
  { href: "/", label: "search", exact: true },
  { href: "/history", label: "history" },
  { href: "/cache", label: "cache" },
  { href: "/health", label: "health" },
  { href: "/docs", label: "api" },
];

/** AI-mode availability from GET /v1/models (`ai_available`). */
export function useAiAvailable(): boolean {
  const [ai, setAi] = useState(false);
  useEffect(() => {
    const ctl = new AbortController();
    listModels(ctl.signal)
      .then((m: ModelsResponse) => setAi(Boolean(m.ai_available)))
      .catch(() => setAi(false));
    return () => ctl.abort();
  }, []);
  return ai;
}

export function Header({ path }: { path: string }) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const isActive = (item: NavItem) =>
    item.exact ? path === item.href : path === item.href || path.startsWith(`${item.href}/`);
  return (
    <header class="navbar bg-base-100 border-b border-base-300 px-4 h-12 min-h-12 flex items-center gap-4">
      <a href="/" class="font-semibold tracking-tight text-base">
        oxe
      </a>
      <nav class="flex items-center gap-1 flex-wrap text-sm" aria-label="main">
        {NAV.map((item) => (
          <a
            key={item.href}
            href={item.href}
            class={`px-2 py-1 rounded ${isActive(item) ? "font-semibold" : "opacity-70 hover:opacity-100"}`}
            aria-current={isActive(item) ? "page" : undefined}
          >
            {isActive(item) ? `[${item.label}]` : item.label}
          </a>
        ))}
      </nav>
      <span class="ml-auto flex items-center gap-1">
        <button
          type="button"
          class="btn btn-ghost btn-xs"
          aria-label="settings"
          onClick={() => setSettingsOpen(true)}
        >
          settings
        </button>
        <span class="text-xs opacity-50 hidden sm:inline">v0.4.0</span>
      </span>
      {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}
    </header>
  );
}

export function usePageTitle(title: string) {
  useEffect(() => {
    document.title = title ? `${title} · oxe` : "oxe";
  }, [title]);
}

export function ModeToggle({
  mode,
  onChange,
  aiAvailable,
  size,
}: {
  mode: "traditional" | "ai";
  onChange: (m: "traditional" | "ai") => void;
  aiAvailable: boolean;
  size?: "xs" | "sm";
}) {
  if (!aiAvailable) return null;
  const opts: Array<{ v: "traditional" | "ai"; label: string }> = [
    { v: "traditional", label: "traditional" },
    { v: "ai", label: "AI" },
  ];
  return (
    <div role="radiogroup" aria-label="search mode" class="join">
      {opts.map((o) => (
        <button
          key={o.v}
          type="button"
          role="radio"
          aria-checked={mode === o.v}
          class={`btn join-item ${size === "xs" ? "btn-xs" : "btn-sm"} ${mode === o.v ? "btn-primary" : "btn-ghost"}`}
          onClick={() => onChange(o.v)}
        >
          {mode === o.v ? o.label.toUpperCase() : o.label}
        </button>
      ))}
    </div>
  );
}

export function Center({ children, vh = false }: { children: ComponentChildren; vh?: boolean }) {
  return (
    <div class={`flex flex-col items-center ${vh ? "justify-center min-h-[75vh]" : ""}`}>
      {children as JSX.Element}
    </div>
  );
}

export function Empty({ children }: { children: ComponentChildren }) {
  return <p class="opacity-60 text-sm py-8 text-center">{toChildArray(children)}</p>;
}
