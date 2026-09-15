import type { ComponentChildren } from "preact";

/** Page chrome below the persistent <Header/> (hoisted in main.tsx).
 * - "home": landing hero, vertically centered in the remaining viewport
 * - "app":  routed screen; pass route-specific width/padding via `class` */
export function Layout({
  children,
  variant = "app",
  class: className,
}: {
  children: ComponentChildren;
  variant?: "home" | "app";
  class?: string;
}) {
  if (variant === "home") {
    return <main class="flex-1 flex items-center justify-center">{children}</main>;
  }
  return <main class={`flex-1 ${className ?? ""}`}>{children}</main>;
}
