import { toChildArray, type ComponentChildren, type JSX } from "preact";
import { useEffect, useState } from "preact/hooks";
import { useLocation } from "preact-iso";
import { listModels, type ModelsResponse } from "../lib/ai";
import { SettingsDialog } from "../features/settings/SettingsDialog";
import { AboutHint } from "./AboutHint";

interface NavItem {
  href: string;
  label: string;
  active?: boolean;
  exact?: boolean;
}

const NAV: NavItem[] = [
  { href: "/", label: "search", exact: true },
  { href: "/history", label: "history" },
  { href: "/dashboard", label: "dashboard" },
];

/** AI-mode availability from GET /v1/models (`ai_available`).
 * Tri-state: null = still querying (never demote AI mode on null). */
export function useAiAvailable(): boolean | null {
  const [ai, setAi] = useState<boolean | null>(null);
  useEffect(() => {
    const ctl = new AbortController();
    listModels(ctl.signal)
      .then((m: ModelsResponse) => setAi(Boolean(m.ai_available)))
      .catch(() => setAi(false));
    return () => ctl.abort();
  }, []);
  return ai;
}

const GitHubIcon = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
    <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.4 7.4 0 0 1 2-.27c.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A7.995 7.995 0 0 0 16 8c0-4.42-3.58-8-8-8z" />
  </svg>
);

export function Header({ path }: { path: string }) {
  const { query, route } = useLocation();
  // ?settings=open is addressable on any route; strip on close.
  const settingsOpen = query?.settings === "open";
  const closeSettings = () => {
    const sp = new URLSearchParams(window.location.search);
    sp.delete("settings");
    const qs = sp.toString();
    route(`${window.location.pathname}${qs ? `?${qs}` : ""}`, true);
  };
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
        <AboutHint />
        <button
          type="button"
          class="btn btn-ghost btn-xs"
          aria-label="settings"
          onClick={() => route(`${window.location.pathname}?settings=open`)}
        >
          settings
        </button>
        <a
          href="https://github.com/espetro/oxe"
          target="_blank"
          rel="noopener noreferrer"
          class="btn btn-ghost btn-sm btn-circle"
          aria-label="GitHub repository"
          title="GitHub repository"
        >
          <GitHubIcon />
        </a>
        <span class="text-xs opacity-50 hidden sm:inline">v0.4.0</span>
      </span>
      {settingsOpen && <SettingsDialog onClose={closeSettings} />}
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
  aiAvailable: boolean | null;
  size?: "xs" | "sm";
}) {
  const opts: Array<{
    v: "traditional" | "ai";
    label: string;
    disabled: boolean;
    title?: string;
  }> = [
    { v: "traditional", label: "traditional", disabled: false },
    {
      v: "ai",
      label: "AI",
      disabled: aiAvailable === false,
      title: aiAvailable === false ? "configure a model in settings to enable AI mode" : undefined,
    },
  ];
  return (
    <div role="radiogroup" aria-label="search mode" class="join">
      {opts.map((o) => (
        <button
          key={o.v}
          type="button"
          role="radio"
          aria-checked={mode === o.v}
          aria-disabled={o.disabled || undefined}
          disabled={o.disabled}
          title={o.title}
          class={`btn join-item ${size === "xs" ? "btn-xs" : "btn-sm"} ${
            mode === o.v ? "btn-primary" : "btn-ghost"
          } ${o.disabled ? "btn-disabled opacity-40" : ""}`}
          onClick={() => {
            if (!o.disabled) onChange(o.v);
          }}
        >
          {mode === o.v ? o.label.toUpperCase() : o.label}
        </button>
      ))}
    </div>
  );
}

export function Center({ children, vh = false }: { children: ComponentChildren; vh?: boolean }) {
  return (
    <div
      class={`flex w-full flex-col items-center ${
        vh ? "justify-center min-h-[75vh] -mt-[30vh]" : ""
      }`}
    >
      {children as JSX.Element}
    </div>
  );
}

export function Empty({ children }: { children: ComponentChildren }) {
  return <p class="opacity-60 text-sm py-8 text-center">{toChildArray(children)}</p>;
}
