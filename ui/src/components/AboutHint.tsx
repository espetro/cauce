import { useState } from "preact/hooks";

/** (?) about affordance: shows a small panel on hover/focus with what
 * oxe is + mode explanations. Keeps the inline-next-to-pill placement
 * (works at both widths without extra layout code). */
export function AboutHint() {
  const [open, setOpen] = useState(false);
  return (
    <div
      class="relative mt-2.5"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        type="button"
        class="btn btn-ghost btn-xs btn-circle opacity-40 hover:opacity-80"
        aria-label="about oxe: caching, MCP API, search modes"
        aria-expanded={open}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onClick={() => setOpen((o) => !o)}
      >
        (?)
      </button>
      {open && (
        <div
          class="absolute left-1/2 -translate-x-1/2 mt-1 w-72 max-w-[80vw] bg-base-100 border border-base-300 rounded-md shadow-sm p-3 text-xs z-50"
          role="note"
        >
          <p class="mb-1.5">
            search once, share with your agents - cached, MCP-ready · REST + MCP API on :4479
          </p>
          <p class="opacity-60">
            <span class="font-medium opacity-80">search:</span> classic link results with cache
            metadata. <span class="font-medium opacity-80">AI:</span> streaming answer with cited
            sources.
          </p>
        </div>
      )}
    </div>
  );
}
