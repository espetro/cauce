/** (?) about affordance in the navbar: daisyUI dropdown, opens on hover
 * or focus and toggles on click. dropdown-end keeps the panel inside the
 * viewport down to ≈390px. */
export function AboutHint() {
  return (
    <div class="dropdown dropdown-end dropdown-hover dropdown-focus">
      <button
        type="button"
        class="btn btn-ghost btn-xs btn-circle opacity-40 hover:opacity-80"
        aria-label="about oxe: caching, MCP API, search modes"
      >
        (?)
      </button>
      <div
        class="dropdown-content w-72 max-w-[min(288px,68vw)] bg-base-100 border border-base-300 rounded-md shadow-sm p-3 text-xs z-50"
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
    </div>
  );
}
