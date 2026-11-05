<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin?tab=cache` — D6 rebuild from user review: the listing is
  a `ui/data-table` (TanStack v9, shadcn-base shape) instead of the
  collapsible card stack. Columns: query, created, expires, hits,
  engines, size, and a trailing actions column (details = info icon
  opening a dialog with the lazy pretty-JSON payload; delete = trash
  icon). Row checkboxes plus a header select-all feed a bulk-delete
  action in the table toolbar; the expired/all sweeps stay beside it.

  The URL keeps `q` + `offset` (server-side filtering and paging —
  the global-filter input in the table toolbar is deliberately not
  used here; the wire is the filter).
-->
<script lang="ts">
  import { CaretLeftIcon, CaretRightIcon, InfoIcon, TrashIcon } from "phosphor-svelte";
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import UiButton from "../../ui/button.svelte";
  import UiDataTable from "../../ui/data-table.svelte";
  import UiDialog from "../../ui/dialog.svelte";
  import { confirm } from "../../ui/confirm.js";
  import type { createCacheTab } from "./cache.svelte.js";
  import type { CacheRow } from "./cache.svelte.js";
  import type { ColumnDef } from "@tanstack/svelte-table";

  interface CacheTabProps {
    tab: ReturnType<typeof createCacheTab>;
  }

  let { tab }: CacheTabProps = $props();
  const c = spa.cache;
  const s = $derived(tab.state);

  const columns: ColumnDef<any, any, any>[] = [
    { id: "query", header: c.col_query, accessorKey: "query", size: 420, enableSorting: true },
    { id: "created", header: c.col_created, accessorKey: "created", size: 140, enableSorting: true },
    { id: "expires", header: c.col_expires, accessorKey: "expires", size: 140, enableSorting: false },
    { id: "hits", header: c.col_hits, accessorFn: (r: CacheRow) => r.hitsLabel, size: 70, enableSorting: true },
    { id: "engines", header: c.col_engines, accessorKey: "engines", size: 200, enableSorting: false },
    { id: "size", header: c.col_size, accessorKey: "size", size: 90, enableSorting: true },
    { id: "actions", header: "", size: 90, enableSorting: false, enableResizing: false },
  ];

  let selected: string[] = $state([]);
  let detailRow: CacheRow | null = $state(null);
  const detailOpen = $derived(detailRow != null);

  function showPayload(row: CacheRow): void {
    tab.loadPayload(row);
    detailRow = row;
  }

  async function deleteSelected(): Promise<void> {
    const n = selected.length;
    const ok = await confirm({
      title: c.delete_selected,
      description: n === 1 ? c.confirm_row : c.confirm_selected,
      danger: true,
    });
    if (!ok) return;
    await tab.removeMany(selected);
    selected = [];
  }

  function onSelectionChange(ids: string[]): void {
    selected = ids;
  }
</script>

<form
  class="filters"
  onsubmit={(e) => {
    e.preventDefault();
    tab.submit(navigate);
  }}
>
  <input
    type="search"
    name="q"
    bind:value={s.q}
    placeholder={c.filter_placeholder}
  />
  <UiButton type="submit">{c.filter_button}</UiButton>
  {#if s.searching}
    <a href="/app/admin?tab=cache">{c.filter_clear}</a>
  {/if}
</form>

<div class="meta">
  <span id="cache-count">{s.countLine}</span>
  {#if s.searching && s.filteredCap !== ""}
    <span>{s.filteredCap}</span>
  {/if}
</div>

{#if s.loading}
  <p class="stats">…</p>
{:else if s.error}
  <p class="field-error" role="alert">{s.error}</p>
{:else if s.rows.length === 0}
  <p class="empty">{s.emptyLine}</p>
{:else}
  <UiDataTable
    {columns}
    data={s.rows.filter((r) => !r.gone)}
    getRowId={(r) => r.key}
    caption={c.page_title}
    empty={s.emptyLine}
    selectable
    onSelectionChange={onSelectionChange}
    initialSorting={[{ id: "created", desc: true }]}
  >
    {#snippet toolbar()}
      {#if selected.length > 0}
        <UiButton size="sm" variant="danger" onclick={deleteSelected}
          >{c.delete_selected} ({selected.length})</UiButton
        >
      {/if}
      <UiButton size="sm" onclick={() => tab.bulk("expired")}
        >{c.delete_expired}</UiButton
      >
      <UiButton size="sm" variant="danger" onclick={() => tab.bulk("all")}
        >{c.delete_all}</UiButton
      >
      {#if s.bulkError}<span class="row-error">{s.bulkError}</span>{/if}
    {/snippet}
    {#snippet cell(cell)}
      {@const row = cell.row.original as CacheRow}
      {#if cell.column.id === "query"}
        <span class="cache-query" class:expired={row.expired} title={row.key}
          >{row.query}</span
        >
      {:else if cell.column.id === "expires"}
        <span class:expired-text={row.expired}>{row.expires}</span>
      {:else if cell.column.id === "engines"}
        <span class="engines-cell">{row.engines}</span>
      {:else if cell.column.id === "actions"}
        <span class="row-actions">
          <UiButton
            size="icon"
            variant="ghost"
            ariaLabel={c.details}
            onclick={() => showPayload(row)}
          >
            <InfoIcon size={14} />
          </UiButton>
          <UiButton
            size="icon"
            variant="ghost"
            ariaLabel={c.delete_row}
            onclick={() => tab.remove(row)}
          >
            <TrashIcon size={14} />
          </UiButton>
        </span>
      {:else}
        {cell.getValue() as string}
      {/if}
    {/snippet}
  </UiDataTable>

  {#if !s.searching && (s.offset > 0 || s.hasNext)}
    <div class="pager meta">
      {#if s.offset > 0}
        <a href={tab.pagerUrl(s.offset - 50)}
          ><CaretLeftIcon size={13} aria-hidden="true" style="vertical-align: -0.125em" />
          {c.page_prev}</a
        >
      {/if}
      {#if s.hasNext}
        <a href={tab.pagerUrl(s.offset + 50)}
          >{c.page_next}
          <CaretRightIcon size={13} aria-hidden="true" style="vertical-align: -0.125em" /></a
        >
      {/if}
    </div>
  {/if}
{/if}

<UiDialog
  open={detailOpen}
  onOpenChange={(o) => (o ? null : (detailRow = null))}
  title={c.payload_title}
>
  {#if detailRow}
    <p class="meta">{detailRow.query}</p>
    {#if detailRow.payloadLoading}
      <p class="meta">{c.payload_loading}</p>
    {:else if detailRow.payloadError !== ""}
      <p class="row-error">{detailRow.payloadError}</p>
    {:else if detailRow.payload !== ""}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <pre tabindex="0" role="group" aria-label={c.payload_title} class="code-view"
        >{detailRow.payload}</pre
      >
    {/if}
  {/if}
</UiDialog>

<style>
  .cache-query {
    font-weight: 500;
  }
  .cache-query.expired {
    color: var(--muted);
  }
  .expired-text {
    color: var(--muted);
  }
  .engines-cell {
    display: block;
    max-width: 12rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.125rem;
  }
  .pager {
    display: flex;
    gap: 1rem;
    margin-top: 0.5rem;
  }
</style>
