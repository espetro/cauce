import { useState } from "preact/hooks";
import { useMountEffect } from "../lib/useMountEffect";

export interface Toast {
  id: number;
  type: "success" | "error" | "info";
  msg: string;
}

let toasts: Toast[] = [];
let nextId = 1;
const subs = new Set<(t: Toast[]) => void>();

let timers = new Map<number, ReturnType<typeof setTimeout>>();

function emit() {
  for (const fn of subs) fn(toasts);
}

/** Push a toast; auto-dismissed after 4s (timer cleared on dismiss). */
export function toast(type: Toast["type"], msg: string) {
  const t = { id: nextId++, type, msg };
  toasts = [...toasts, t];
  emit();
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
  toasts = toasts.filter((t) => t.id !== id);
  emit();
}

/** Fixed daisyUI toast stack (bottom-end). Mount once, next to the Header. */
export function Toasts() {
  const [list, setList] = useState<Toast[]>(toasts);
  useMountEffect(function subscribeToToasts() {
    subs.add(setList);
    return () => {
      subs.delete(setList);
    };
  });
  const alertClass = (t: Toast) =>
    t.type === "success" ? "alert-success" : t.type === "error" ? "alert-error" : "alert-info";
  return (
    <div class="toast toast-end toast-bottom z-50">
      {list.map((t) => (
        <div
          key={t.id}
          role="status"
          class={`alert ${alertClass(t)} text-sm py-2 animate-in fade-in slide-in-from-bottom-2 duration-300`}
          onClick={() => dismiss(t.id)}
        >
          <span>{t.msg}</span>
        </div>
      ))}
    </div>
  );
}
