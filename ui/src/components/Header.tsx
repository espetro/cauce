import { toChildArray, type ComponentChildren, type JSX } from "preact";
import { useEffect } from "preact/hooks";

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

export function Header({ path }: { path: string }) {
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
      <span class="ml-auto text-xs opacity-50 hidden sm:inline">v0.4.0</span>
    </header>
  );
}

export function usePageTitle(title: string) {
  useEffect(() => {
    document.title = title ? `${title} · oxe` : "oxe";
  }, [title]);
}

/** Feature seam: AI mode surface ships in a later stage. */
export const AI_MODE_ENABLED = false;

export function ModeToggle({
  mode,
  onChange,
  size,
}: {
  mode: "traditional" | "ai";
  onChange: (m: "traditional" | "ai") => void;
  size?: "xs" | "sm";
}) {
  if (!AI_MODE_ENABLED) return null;
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
