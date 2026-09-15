import { useState } from "preact/hooks";

/** (?) about affordance in the navbar: shows a small right-aligned panel
 * on hover/focus with what oxe is + mode explanations. dropdown-end-style
 * anchoring keeps the panel inside the viewport down to ≈390px. */
export function AboutHint() {
  const [open, setOpen] = useState(false);
  return (
    <div class="relative" onMouseEnter={() => setOpen(true)} onMouseLeave={() => setOpen(false)}>
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
          class="absolute right-0 mt-1 w-72 max-w-[min(288px,68vw)] bg-base-100 border border-base-300 rounded-md shadow-sm p-3 text-xs z-50"
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
