<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  project, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/data-table` — the D6 table surface: TanStack Table v9 (headless)
  behind an app-owned skin. The headless core owns the column model
  (visibility, sizing, sorting); this component owns the markup, the
  CSS and the caller's cell bodies.

  Why: the history and audit tables used to overflow their container
  at the app's reading measure (fixed `max-width` body, cells with
  `white-space: nowrap`). The fix is not "scroll harder" — it is a
  table that reflows: columns can be hidden, resized and sorted, and
  the remaining columns share the available width.

  Contract:
  - `columns` — TanStack column defs. `header` is a plain string (the
    usual case); a sortable column's header renders as a button that
    toggles the sort. `cell` is the column's *value* (the accessor
    result) — the caller renders the cell body through the `cell`
    snippet, keyed by `cell.column.id`.
  - `data` — the rows, read through a getter so the table follows
    reactive reassignment (TanStack's `createTable` reads `data`
    through a getter; capturing it once would freeze the table).
  - `getRowId` — stable row identity (sorting and re-renders need it).
  - `caption` — the table's accessible name.
  - `empty` — the empty-state copy, rendered in place of the body.
  - `initialVisibility` / `initialSorting` — initial state slices.
  - `cell` — snippet: the body of one cell. Receives the TanStack
    cell; the caller switches on `cell.column.id`.
  - `detail` — optional snippet: an extra full-width row under a row
    (the history feed's click list, the audit row's JSON payload).
    Receives the row.

  The table uses `table-layout: fixed` so a resized column actually
  holds its width, and the whole table sits in a horizontally
  scrollable, keyboard-scrollable region.
-->
<script lang="ts">
  import {
    createTable,
    columnResizingFeature,
    columnSizingFeature,
    columnVisibilityFeature,
    rowSortingFeature,
    tableFeatures,
    type ColumnDef,
    type RowData,
    type SortingState,
  } from "@tanstack/svelte-table";
  import { CaretDownIcon, CaretUpIcon } from "phosphor-svelte";
  import type { Snippet } from "svelte";

  interface Props {
    columns: ColumnDef<any, any, any>[];
    data: unknown[];
    getRowId?: (row: any, index: number) => string;
    caption: string;
    empty?: string;
    initialVisibility?: Record<string, boolean>;
    initialSorting?: SortingState;
    cell: Snippet<[cell: any]>;
    detail?: Snippet<[row: any]>;
    class?: string;
  }

  let {
    columns,
    data,
    getRowId,
    caption,
    empty = "",
    initialVisibility = {},
    initialSorting = [],
    cell,
    detail,
    class: className = "",
  }: Props = $props();

  // The features every table here needs: hide a column, resize it,
  // sort by it. Sorting is opt-in per column (`enableSorting`), so a
  // column the app cannot order stays inert.
  const features = tableFeatures({
    columnVisibilityFeature,
    columnSizingFeature,
    columnResizingFeature,
    rowSortingFeature,
  });

  const table = createTable({
    features,
    get columns() {
      return columns;
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
  });

  const rows = $derived(table.getRowModel().rows);
  const headerGroups = $derived(table.getHeaderGroups());
  const sorting = $derived(table.atoms.sorting.get());

  function sortDir(id: string): "asc" | "desc" | false {
    const s = sorting.find((x) => x.id === id);
    return s ? (s.desc ? "desc" : "asc") : false;
  }

  function headerLabel(column: any): string {
    const h = column.columnDef.header;
    return typeof h === "string" ? h : column.id;
  }
</script>

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
            <th style={header.getSize() > 0 ? `width: {header.getSize()}px` : undefined}>
              {#if header.column.getCanSort()}
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
          <td class="dt-empty" colspan={headerGroups[0]?.headers.length ?? 1}>
            {empty}
          </td>
        </tr>
      {:else}
        {#each rows as row (row.id)}
          <tr>
            {#each row.getVisibleCells() as c (c.id)}
              <td>
                {@render cell(c)}
              </td>
            {/each}
          </tr>
          {#if detail}
            <tr class="dt-detail">
              <td colspan={headerGroups[0]?.headers.length ?? 1}>
                {@render detail(row)}
              </td>
            </tr>
          {/if}
        {/each}
      {/if}
    </tbody>
  </table>
</div>

<style>
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
