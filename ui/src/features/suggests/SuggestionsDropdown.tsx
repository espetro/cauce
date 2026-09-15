import type { Suggestion } from "./useSuggests";
import { GROUP_LABEL } from "./useSuggests";

interface Props {
  items: Suggestion[];
  activeIndex: number | null;
  onPick: (text: string) => void;
  onHover: (i: number | null) => void;
}

/** Zero-weight suggestions dropdown: canvas background, hairline border,
 * muted caps group labels (not options). Does not submit. */
export function SuggestionsDropdown({ items, activeIndex, onPick, onHover }: Props) {
  if (!items.length) return null;
  let idx = -1;
  let lastGroup: Suggestion["group"] | null = null;
  return (
    <ul
      role="listbox"
      class="absolute left-0 right-0 top-full mt-1 z-50 bg-base-100 border border-base-300 rounded-md shadow-sm py-1 text-sm m-0 list-none p-0"
    >
      {items.map((s) => {
        idx += 1;
        const i = idx;
        const showLabel = s.group !== lastGroup;
        lastGroup = s.group;
        return (
          <li role="none" key={`${s.group}-${s.text}`}>
            {showLabel && (
              <div
                class="px-3 pt-2 pb-1 text-[11px] uppercase tracking-wide opacity-40 select-none"
                role="presentation"
              >
                {GROUP_LABEL[s.group]}
              </div>
            )}
            <div
              role="option"
              aria-selected={activeIndex === i}
              class={`px-3 py-1.5 cursor-pointer ${activeIndex === i ? "bg-base-200" : ""}`}
              onMouseDown={(e) => {
                e.preventDefault();
                onPick(s.text);
              }}
              onMouseEnter={() => onHover(i)}
              onMouseLeave={() => onHover(null)}
            >
              {s.text}
            </div>
          </li>
        );
      })}
    </ul>
  );
}
