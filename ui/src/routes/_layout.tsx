import type { ComponentChildren } from "preact";
import { Header } from "../components/Header";
import { AboutHint } from "../components/AboutHint";
import { ErrorBoundary } from "../components/ErrorBoundary";
import { Toasts } from "../components/Toasts";

/** Root layout, applied to every route via import.meta.glob in main.tsx.
 * Hoists the persistent <Header/> (owns the global ?settings= dialog),
 * the toast stack, and the fixed bottom-left (?) hint. The boundary only
 * covers routed content, so a render crash keeps the shell alive. */
export default function Layout({ children }: { children: ComponentChildren }) {
  return (
    <div class="min-h-screen flex flex-col">
      <Header />
      <main class="flex-1 flex flex-col">
        <ErrorBoundary>{children}</ErrorBoundary>
      </main>
      <Toasts />
      <AboutHint />
    </div>
  );
}
