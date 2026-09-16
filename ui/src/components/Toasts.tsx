import { useToasts, dismiss, type Toast } from "../lib/toasts";

/** Fixed daisyUI toast stack (bottom-end). Mount once, next to the Header. */
export function Toasts() {
  const list = useToasts();
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
