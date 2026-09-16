import { toChildArray, type ComponentChildren, type JSX } from "preact";
import { useEffect, useState } from "preact/hooks";
import { useLocation } from "preact-iso";
import { listModels, type ModelsResponse } from "../lib/ai";
import { SettingsDialog } from "../features/settings/SettingsDialog";
import IconGitHub from "~icons/lucide/github";

interface NavItem {
  href: string;
  label: string;
  active?: boolean;
  exact?: boolean;
}

const NAV: NavItem[] = [
  { href: "/", label: "Search", exact: true },
  { href: "/history", label: "History" },
  { href: "/dashboard", label: "Dashboard" },
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

const GitHubIcon = () => <IconGitHub class="w-4 h-4" aria-hidden="true" />;

export function Header() {
  const { path, query, route } = useLocation();
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
      <a href="/" class="font-logo font-semibold tracking-tight text-base">
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
          aria-label="Settings"
          onClick={() => route(`${window.location.pathname}?settings=open`)}
        >
          Settings
        </button>
        <a
          href="https://github.com/espetro/oxe"
          target="_blank"
          rel="noopener noreferrer"
          class="btn btn-ghost btn-sm btn-circle"
          aria-label="GitHub repository"
          tabIndex={0}
        >
          <GitHubIcon />
        </a>
        <span class="text-xs opacity-50 hidden sm:inline">v{__APP_VERSION__}</span>
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
    },
  ];
  return (
    <div role="radiogroup" aria-label="Search mode" class="join">
      {opts.map((o) => (
        <span
          key={o.v}
          class="tooltip tooltip-bottom"
          data-tip={o.disabled ? "configure a model in settings to enable AI mode" : undefined}
        >
          <button
            type="button"
            role="radio"
            aria-checked={mode === o.v}
            aria-disabled={o.disabled || undefined}
            disabled={o.disabled}
            class={`btn join-item ${size === "xs" ? "btn-xs" : "btn-sm"} ${
              mode === o.v ? "btn-primary" : "btn-ghost"
            } ${o.disabled ? "btn-disabled opacity-40" : ""}`}
            onClick={() => {
              if (!o.disabled) onChange(o.v);
            }}
          >
            {mode === o.v ? o.label.toUpperCase() : o.label}
          </button>
        </span>
      ))}
    </div>
  );
}

export function Center({ children, vh = false }: { children: ComponentChildren; vh?: boolean }) {
  return (
    <div
      class={`flex w-full flex-col items-center ${
        vh ? "justify-center grow min-h-[calc(100vh-3rem)]" : ""
      }`}
    >
      {children as JSX.Element}
    </div>
  );
}

export function Empty({ children }: { children: ComponentChildren }) {
  return <p class="opacity-60 text-sm py-8 text-center">{toChildArray(children)}</p>;
}
