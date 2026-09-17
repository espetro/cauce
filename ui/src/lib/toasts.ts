// Toast state as a nanostores atom (replaces the hand-rolled subscriber set
// in the old components/Toasts.tsx). Producers across features call toast();
// the single <Toasts> renderer uses useStore(). Auto-dismiss after 4s,
// timer cleared on manual dismiss.
import { atom } from "nanostores";
import { useStore } from "@nanostores/preact";

export interface Toast {
  id: number;
  type: "success" | "error" | "info";
  msg: string;
}

export const $toasts = atom<Toast[]>([]);

let nextId = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

/** Push a toast; auto-dismissed after 4s (timer cleared on dismiss). */
export function toast(type: Toast["type"], msg: string) {
  const t = { id: nextId++, type, msg };
  $toasts.set([...$toasts.get(), t]);
  timers.set(
    t.id,
    setTimeout(() => dismiss(t.id), 4000),
  );
}

export function dismiss(id: number) {
  const timer = timers.get(id);
  if (timer) {
    clearTimeout(timer);
    timers.delete(id);
  }
  $toasts.set($toasts.get().filter((t) => t.id !== id));
}

/** Reactive toast list for the renderer. */
export function useToasts(): Toast[] {
  return useStore($toasts);
}
