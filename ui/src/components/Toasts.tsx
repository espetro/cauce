import { useEffect, useState } from "preact/hooks";

export interface Toast {
  id: number;
  type: "success" | "error" | "info";
  msg: string;
}

let toasts: Toast[] = [];
let nextId = 1;
const subs = new Set<(t: Toast[]) => void>();

function emit() {
  for (const fn of subs) fn(toasts);
}

/** Push a toast; auto-dismissed by the container after 4s. */
export function toast(type: Toast["type"], msg: string) {
  const t = { id: nextId++, type, msg };
  toasts = [...toasts, t];
  emit();
  setTimeout(() => dismiss(t.id), 4000);
}

export function dismiss(id: number) {
  toasts = toasts.filter((t) => t.id !== id);
  emit();
}

/** Fixed daisyUI toast stack (bottom-end). Mount once, next to the Header. */
export function Toasts() {
  const [list, setList] = useState<Toast[]>(toasts);
  useEffect(() => {
    subs.add(setList);
    return () => {
      subs.delete(setList);
    };
  }, []);
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
