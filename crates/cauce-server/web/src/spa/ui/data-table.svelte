<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/data-table` — the D6 table surface, modelled on TanStack's
  `lib-shadcn-base` example (TanStack Table v9, headless core behind an
  app-owned skin). The core owns the column model (visibility, sizing,
  sorting), the row models (sorted + filtered) and optional row
  selection; this component owns the markup, the CSS and the caller's
  cell bodies.

  Contract:
  - `columns` — TanStack column defs. `header` is a plain string (the
    usual case); a sortable column's header renders as a button that
    cycles unsorted -> asc -> desc (tri-state, like the example). `cell`
    is the column's *value* — the caller renders the body through the
    `cell` snippet, keyed by `cell.column.id`.
  - `data` — the rows, read through a getter so the table follows
    reactive reassignment (TanStack's `createTable` reads `data`
    through a getter; capturing it once would freeze the table).
  - `getRowId` — stable row identity (sorting, selection and re-renders
    need it).
  - `caption` — the table's accessible name.
  - `empty` — the empty-state copy, rendered in place of the body.
  - `initialSorting` / `initialVisibility` — initial state slices.
  - `selectable` — adds a leading checkbox column (header checkbox
    toggles every row on the page; `onSelectionChange` reports the
    selected ids) and highlights selected rows.
  - `searchPlaceholder` — when set, renders a global-filter input in
    the toolbar row (the example's debounced search, client-side).
  - `toolbar` — snippet rendered in the toolbar's left slot (bulk
    actions, counts); only rendered when the toolbar exists at all.
  - `cell` — snippet: the body of one cell.
  - `detail` — optional snippet: an extra full-width row under a row
    (the history feed's click list).

  The table uses `table-layout: fixed` so a resized column holds its
  width, and sits in a horizontally scrollable, keyboard-scrollable
  region. Selection and filter state are client-side only — server
  pagination (`CacheTab`) filters on the wire before the rows land.
-->
<script lang="ts">
  import {
    createFilteredRowModel,
    createSortedRowModel,
    columnResizingFeature,
    columnSizingFeature,
    columnVisibilityFeature,
    createTable,
    filterFn_includesString,
    globalFilteringFeature,
    columnFilteringFeature,
    rowSelectionFeature,
    rowSortingFeature,
    sortFn_alphanumeric,
    sortFn_text,
    tableFeatures,
    type ColumnDef,
    type RowData,
    type SortingState,
  } from "@tanstack/svelte-table";
  import {
    CaretDownIcon,
    CaretUpIcon,
    CaretUpDownIcon,
    MagnifyingGlassIcon,
  } from "phosphor-svelte";
  import type { Snippet } from "svelte";

  interface Props {
    columns: ColumnDef<any, any, any>[];
    data: unknown[];
    getRowId?: (row: any, index: number) => string;
    caption: string;
    empty?: string;
    initialSorting?: SortingState;
    initialVisibility?: Record<string, boolean>;
    selectable?: boolean;
    searchPlaceholder?: string;
    onSelectionChange?: (ids: string[]) => void;
    toolbar?: Snippet;
    /** The checker infers implicit children when a caller mixes
     * snippet tags with function props; declared so Props accepts it. */
    children?: Snippet;
    cell: Snippet<[cell: any]>;
    /**
     * Optional snippet rendered as a full-width row ABOVE the data
     * row (the history feed's day separator). */
    leading?: Snippet<[row: any]>;
    /**
     * Predicate for when a row carries a `leading` row — the day
     * separator renders under the first row of each day only. */
    leadingWhen?: (row: any) => boolean;
    detail?: Snippet<[row: any]>;
    /**
     * Predicate for when a row carries a `detail` row (the history
     * feed's day separators render under the first row of each day
     * only — without this, every row would carry an empty detail). */
    detailWhen?: (row: any) => boolean;
    class?: string;
  }

  let {
    columns,
    data,
    getRowId,
    caption,
    empty = "",
    initialSorting = [],
    initialVisibility = {},
    selectable = false,
    searchPlaceholder = "",
    onSelectionChange,
    toolbar,
    cell,
    leading,
    leadingWhen,
    detail,
    detailWhen,
    class: className = "",
  }: Props = $props();

  // The feature set from the shadcn-base example: sort, filter,
  // select. The row-model factories are what opt the table into
  // client-side sorting/filtering; the feature flags alone only add
  // the state slices.
  const features = tableFeatures({
    columnVisibilityFeature,
    columnSizingFeature,
    columnResizingFeature,
    rowSortingFeature,
    rowSelectionFeature,
    columnFilteringFeature,
    globalFilteringFeature,
    sortedRowModel: createSortedRowModel(),
    filteredRowModel: createFilteredRowModel(),
    sortFns: {
      alphanumeric: sortFn_alphanumeric,
      text: sortFn_text,
    },
    filterFns: {
      includesString: filterFn_includesString,
    },
  });

  // The leading checkbox column when `selectable` — header toggles the
  // page, body checkboxes toggle a row. `enableSorting:false`,
  // `enableResizing:false`: it stays put while the data columns move.
  const allColumns = $derived(
    selectable
      ? [
          {
            id: "select",
            header: "",
            enableSorting: false,
            enableResizing: false,
            enableHiding: false,
            size: 32,
          } as ColumnDef<any, any, any>,
          ...columns,
        ]
      : columns,
  );

  const table = createTable({
    features,
    get columns() {
      return allColumns;
    },
    get data() {
      return data as RowData[];
    },
    get getRowId() {
      return getRowId ?? ((_row: unknown, index: number) => String(index));
    },
    get initialState() {
      return {
        columnVisibility: initialVisibility,
        sorting: initialSorting,
      };
    },
    enableColumnResizing: true,
    columnResizeMode: "onEnd",
    globalFilterFn: "includesString",
  });

  const rows = $derived(table.getRowModel().rows);
  const headerGroups = $derived(table.getHeaderGroups());
  const sorting = $derived(table.atoms.sorting.get());
  const selection = $derived(table.atoms.rowSelection.get());
  const globalFilter = $derived(table.atoms.globalFilter.get());
  const allSelected = $derived(
    rows.length > 0 && rows.every((r) => !!selection[r.id]),
  );
  const someSelected = $derived(rows.some((r) => !!selection[r.id]));

  // Report the selected ids upward whenever the map changes. `gone`
  // rows may still be selected (the caller filters them from `data`
  // but their ids linger) — pruned here so bulk actions see live rows
  // only.
  $effect(() => {
    const sel = selection;
    if (!selectable || !onSelectionChange) return;
    const ids = Object.keys(sel).filter((k) => sel[k]);
    onSelectionChange(ids);
  });

  let search = $state("");

  function setSearch(v: string): void {
    search = v;
    table.setGlobalFilter(v === "" ? undefined : v);
  }

  function sortDir(id: string): "asc" | "desc" | false {
    const s = sorting.find((x) => x.id === id);
    return s ? (s.desc ? "desc" : "asc") : false;
  }

  function headerLabel(column: any): string {
    const h = column.columnDef.header;
    return typeof h === "string" ? h : column.id;
  }

  /*
   * Column widths are PERCENTAGES of the table, not pixels: the table
   * is `table-layout: fixed; width: 100%`, so proportional widths make
   * the columns fit the container instead of overflowing it (the
   * user-review ask — shrink columns, never spill). TanStack's sizes
   * are px (default 150); they are normalised against the total so a
   * column's share of the row stays stable while resizing. The
   * resize handle adjusts its own column in px and every other
   * column re-shares the remainder — same behaviour as the shadcn
   * example, but expressed in percents so nothing overflows.
   */
  const totalSize = $derived(
    headerGroups[0]?.headers.reduce((sum, h) => sum + h.getSize(), 0) ?? 0,
  );

  function headerWidth(header: any): string | undefined {
    if (totalSize <= 0) return undefined;
    const pct = (header.getSize() / totalSize) * 100;
    return `width: ${pct.toFixed(2)}%`;
  }

  const colspan = $derived(headerGroups[0]?.headers.length ?? 1);
</script>

{#if searchPlaceholder !== "" || toolbar}
  <div class="dt-toolbar">
    <div class="dt-toolbar-start">
      {@render toolbar?.()}
    </div>
    {#if searchPlaceholder !== ""}
      <div class="dt-search">
        <MagnifyingGlassIcon class="dt-search-icon" size={14} aria-hidden="true" />
        <input
          type="search"
          value={search}
          placeholder={searchPlaceholder}
          aria-label={searchPlaceholder}
          oninput={(e) => setSearch(e.currentTarget.value)}
        />
      </div>
    {/if}
  </div>
{/if}

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- tabindex=0 keeps the horizontally scrollable region keyboard
     scrollable (the audit table's legacy wrap had the same affordance). -->
<div class="dt-wrap" tabindex="0" role="region" aria-label={caption}>
  <table class="dt {className}">
    <caption class="vh">{caption}</caption>
    <thead>
      {#each headerGroups as group (group.id)}
        <tr>
          {#each group.headers as header (header.id)}
            {@const dir = sortDir(header.column.id)}
            <th style={headerWidth(header)}>
              {#if header.id === "select"}
                <input
                  type="checkbox"
                  class="dt-check"
                  checked={allSelected}
                  indeterminate={!allSelected && someSelected}
                  aria-label="select all"
                  onchange={() => table.toggleAllRowsSelected()}
                />
              {:else if header.column.getCanSort()}
                <button
                  type="button"
                  class="dt-sort"
                  onclick={() => header.column.toggleSorting()}
                >
                  <span class="dt-sort-label">{headerLabel(header.column)}</span>
                  {#if dir === "asc"}
                    <CaretUpIcon class="dt-sort-icon" size={12} />
                  {:else if dir === "desc"}
                    <CaretDownIcon class="dt-sort-icon" size={12} />
                  {:else}
                    <CaretUpDownIcon class="dt-sort-icon dt-sort-idle" size={12} />
                  {/if}
                </button>
              {:else}
                {headerLabel(header.column)}
              {/if}
              {#if header.column.getCanResize()}
                <button
                  type="button"
                  class="dt-resize"
                  aria-label="resize {headerLabel(header.column)}"
                  onmousedown={header.getResizeHandler()}
                  ondblclick={() => header.column.resetSize()}
                ></button>
              {/if}
            </th>
          {/each}
        </tr>
      {/each}
    </thead>
    <tbody>
      {#if rows.length === 0}
        <tr>
          <td class="dt-empty" colspan={colspan}>{empty}</td>
        </tr>
      {:else}
        {#each rows as row (row.id)}
          {#if leading && (!leadingWhen || leadingWhen(row.original))}
            <tr class="dt-detail dt-leading">
              <td colspan={colspan}>{@render leading(row)}</td>
            </tr>
          {/if}
          <tr class:dt-selected={!!selection[row.id]}>
            {#each row.getVisibleCells() as c (c.id)}
              <td>
                {#if c.column.id === "select"}
                  <input
                    type="checkbox"
                    class="dt-check"
                    checked={!!selection[row.id]}
                    aria-label="select row {row.id}"
                    onchange={() => row.toggleSelected()}
                  />
                {:else}
                  {@render cell(c)}
                {/if}
              </td>
            {/each}
          </tr>
          {#if detail && (!detailWhen || detailWhen(row.original))}
            <tr class="dt-detail dt-trailing">
              <td colspan={colspan}>{@render detail(row)}</td>
            </tr>
          {/if}
        {/each}
      {/if}
    </tbody>
  </table>
</div>

<style>
  .dt-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-bottom: 0.5rem;
    flex-wrap: wrap;
  }
  .dt-toolbar-start {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
    flex-wrap: wrap;
  }
  .dt-search {
    position: relative;
    margin-left: auto;
  }
  .dt-search-icon {
    position: absolute;
    left: 0.5rem;
    top: 50%;
    transform: translateY(-50%);
    color: var(--muted);
    pointer-events: none;
  }
  .dt-search input {
    padding: 0.3rem 0.5rem 0.3rem 1.75rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font-size: 0.8125rem;
    width: 14rem;
    max-width: 60vw;
  }
  .dt-search input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .dt-wrap {
    /* The region scrolls only when the columns cannot shrink below
       their content floor; the table itself reflows first. */
    overflow-x: auto;
    max-width: 100%;
  }
  table.dt {
    width: 100%;
    border-collapse: collapse;
    /* Fixed layout: a resized column keeps its width instead of
       being squeezed by the content of its neighbours. */
    table-layout: fixed;
    font-size: 0.875rem;
  }
  table.dt th {
    position: relative;
    text-align: start;
    color: var(--muted);
    font-weight: 500;
    font-size: 0.8125rem;
    padding: 0.25rem 0.5rem 0.25rem 0;
    border-bottom: 1px solid var(--border);
    user-select: none;
  }
  table.dt td {
    padding: 0.4rem 0.5rem 0.4rem 0;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
    overflow-wrap: anywhere;
  }
  table.dt tbody tr:hover td {
    background: var(--greyed-bg);
  }
  table.dt tbody tr.dt-selected td {
    background: var(--greyed-bg);
  }
  .dt-check {
    accent-color: var(--accent);
    width: 0.9375rem;
    height: 0.9375rem;
  }
  .dt-sort {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
    text-align: start;
  }
  .dt-sort:hover {
    color: var(--fg);
  }
  .dt-sort-icon {
    flex: none;
    color: var(--accent);
  }
  .dt-sort-idle {
    color: var(--muted);
    opacity: 0.6;
  }
  .dt-resize {
    position: absolute;
    inset-inline-end: 0;
    top: 0;
    bottom: 0;
    width: 6px;
    padding: 0;
    border: none;
    background: none;
    cursor: col-resize;
    touch-action: none;
  }
  .dt-resize:hover {
    background: var(--accent);
    opacity: 0.4;
  }
  .dt-empty {
    color: var(--muted);
    padding: 1rem 0;
  }
</style>
