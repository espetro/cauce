For Preact applications, the two best options for virtualized infinite scrolling depend on whether you prioritize headless flexibility or minimal bundle size.

---

### Recommended Options

| Library | Bundle Size | Dynamic Heights | Best For |
| --- | --- | --- | --- |
| **`@tanstack/react-virtual`** (via `preact/compat`) | ~4–5 kB | Excellent (auto-measure) | Complex layouts, window scrolling, fine-grained control over fetch triggers |
| **`virtua`** (via `preact/compat`) | ~3 kB | Automatic (zero-config) | Fastest setup, automatic item resizing, very low overhead |

*Note: Since you are using Vite + Preact, ensure your `vite.config.ts` includes the `@preact/preset-vite` plugin or aliases `react` and `react-dom` to `preact/compat`.*

---

### Implementing Infinite Scroll with `@tanstack/react-virtual`

TanStack Virtual provides headless control over scrolling calculations. Infinite scroll works by checking if the last rendered item in the virtual window corresponds to the end of your loaded dataset:

```tsx
import { useRef, useEffect } from 'preact/hooks';
import { useVirtualizer } from '@tanstack/react-virtual';

interface Props {
  items: string[];
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  fetchNextPage: () => void;
}

export function InfiniteVirtualList({
  items,
  hasNextPage,
  isFetchingNextPage,
  fetchNextPage,
}: Props) {
  const parentRef = useRef<HTMLDivElement>(null);

  // Total count includes an extra dummy slot at the end for the loading spinner
  const totalCount = hasNextPage ? items.length + 1 : items.length;

  const rowVirtualizer = useVirtualizer({
    count: totalCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 60, // Estimated item height in px
    overscan: 5,            // Pre-render buffer
  });

  const virtualItems = rowVirtualizer.getVirtualItems();

  // Detect when the user reaches the end of the loaded list
  useEffect(() => {
    const lastItem = virtualItems[virtualItems.length - 1];
    if (!lastItem) return;

    if (
      lastItem.index >= items.length - 1 &&
      hasNextPage &&
      !isFetchingNextPage
    ) {
      fetchNextPage();
    }
  }, [virtualItems, items.length, hasNextPage, isFetchingNextPage]);

  return (
    <div
      ref={parentRef}
      style={{
        height: '500px',
        overflowY: 'auto',
        position: 'relative',
      }}
    >
      <div
        style={{
          height: `${rowVirtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualItems.map((virtualRow) => {
          const isLoaderRow = virtualRow.index > items.length - 1;

          return (
            <div
              key={virtualRow.key}
              ref={rowVirtualizer.measureElement}
              data-index={virtualRow.index}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualRow.start}px)`,
                padding: '12px',
                boxSizing: 'border-box',
              }}
            >
              {isLoaderRow
                ? 'Loading more items...'
                : items[virtualRow.index]}
            </div>
          );
        })}
      </div>
    </div>
  );
}

```

---

### Alternative: `virtua`

If you prefer a drop-in component rather than calculating absolute positioning manually, **`virtua`** provides a ready-made `<VList>`:

```tsx
import { VList } from 'virtua';

export function VirtuaList({ items, hasNextPage, fetchNextPage }) {
  return (
    <VList
      style={{ height: 500 }}
      onRangeChange={({ endIndex }) => {
        if (endIndex >= items.length - 1 && hasNextPage) {
          fetchNextPage();
        }
      }}
    >
      {items.map((item) => (
        <div key={item.id} class="item-card">
          {item.name}
        </div>
      ))}
    </VList>
  );
}

```

Both libraries pair seamlessly with pagination abstractions like **TanStack Query** (`useInfiniteQuery`) for fetching and caching pages.
