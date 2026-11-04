<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/history` markup — one-for-one with `templates/history.html`:
  filter bar (since / origin / q / cached-only), day-header row groups
  (`detail` snippet), detail row per item with nested clicks + row
  actions + row delete. Interactive controls ride the ui/ wrappers
  (plan §3 DS-09): UiSelect facet selects, UiCheckbox cached-only,
  UiCollapsible per-row clicks, UiButton submit + row delete.

  D6: the feed renders through `ui/data-table` (TanStack Table v9) —
  sortable when/time columns, resizable columns, and the day grouping
  rides the `detail` snippet so the table keeps a stable column model.
-->
<script lang="ts">
  import { CaretRightIcon } from "phosphor-svelte";
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import UiButton from "../../ui/button.svelte";
  import UiCollapsible from "../../ui/collapsible.svelte";
  import UiDataTable from "../../ui/data-table.svelte";
  import FilterBar from "../../features/search/FilterBar.svelte";
  import type { createHistoryPage } from "./history.svelte.js";
  import type { HistRow } from "./history.svelte.js";
  import type { ColumnDef } from "@tanstack/svelte-table";

  interface HistoryViewProps {
    page: ReturnType<typeof createHistoryPage>;
  }

  let { page }: HistoryViewProps = $props();
  const h = spa.history;
  const s = $derived(page.state);

  const sinceOptions = $derived([
    { value: "all", label: h.window_all },
    { value: "24h", label: h.window_24h },
    { value: "7d", label: h.window_7d },
    { value: "30d", label: h.window_30d },
  ]);
  const originOptions = $derived([
    { value: "user", label: h.origin_mine },
    { value: "agent", label: h.origin_agent },
    { value: "all", label: h.origin_all },
  ]);

  const filterFields = $derived([
    {
      key: "since",
      label: h.since_label,
      type: "select" as const,
      value: s.since,
      options: sinceOptions,
    },
    {
      key: "origin",
      label: h.origin_label,
      type: "select" as const,
      value: s.origin,
      options: originOptions,
    },
    {
      key: "q",
      label: h.query_label,
      type: "text" as const,
      value: s.q,
      placeholder: h.query_placeholder,
      submitOnChange: false,
    },
    {
      key: "cached",
      label: h.cached_only,
      type: "checkbox" as const,
      value: s.cached,
      ariaLabel: h.cached_only,
    },
  ]);

  // D6: the column model. All sortable columns sort on their raw
  // values; the query column's accessor is the display string.
  const columns: ColumnDef<any, any, any>[] = [
    // `when` is a display string ("14:30"), so sorting on it would
    // order by clock time across days — the feed is already ordered
    // newest-first by the server, so the column stays inert.
    { id: "when", header: h.col_when, accessorKey: "when", enableSorting: false },
    { id: "query", header: h.col_query, accessorKey: "query", enableSorting: true },
    { id: "source", header: h.col_source, accessorKey: "source", enableSorting: false },
    { id: "engines", header: h.col_engines, accessorKey: "engines", enableSorting: false },
    {
      id: "resultCount",
      header: h.col_results,
      accessorFn: (r: HistRow) => Number(r.resultCount),
      enableSorting: true,
    },
    { id: "latency", header: h.col_latency, accessorKey: "latency", enableSorting: false },
    { id: "client", header: h.col_client, accessorKey: "client", enableSorting: true },
  ];


</script>

<h1 class="page-title">{h.title}</h1>

<FilterBar
  fields={filterFields}
  onChange={(key, value) => {
    if (key === "since") s.since = String(value);
    else if (key === "origin") s.origin = String(value);
    else if (key === "q") s.q = String(value);
    else if (key === "cached") s.cached = value === true;
  }}
  showSubmit
  submitLabel={h.filter_submit}
  onSubmit={() => page.submit(navigate)}
/>
{#if page.filtersActive()}
  <a class="clear" href="/app/history">{h.clear}</a>
{/if}

{#if s.loading}
  <p class="stats">…</p>
{:else if s.error}
  <p class="field-error" role="alert">{s.error}</p>
{:else if s.rows.length === 0}
  <p class="empty">
    {s.emptyMessage}
    {#if s.originAllUrl}<a href={s.originAllUrl}>{h.ef_origin_all}</a>{/if}
  </p>
{:else}
  <UiDataTable
    {columns}
    data={s.rows.filter((r) => !r.gone)}
    getRowId={(r) => r.kind + "-" + r.id + "-" + r.when}
    caption={h.title}
    empty={s.emptyMessage}
  >
    {#snippet cell(c)}
      {@const row = c.row.original as HistRow}
      {#if c.column.id === "when"}
        {row.when}
      {:else if c.column.id === "query"}
        {#if row.kind === "search" || row.kind === "answer"}
          <a href={row.queryUrl}>{row.query}</a>
          {#if row.queryChip}<span class="meta-chip">{row.queryChip}</span>{/if}
        {:else}
          {h.click_only}
        {/if}
      {:else if c.column.id === "source"}
        {#if row.sourceUrl}<a href={row.sourceUrl}>{row.source}</a
          >{:else}{row.source}{/if}
      {:else if c.column.id === "client"}
        {row.client}
        {#if row.originChip}<span class="meta-chip">{h.chip_agent}</span>{/if}
      {:else}
        {c.getValue() as string}
      {/if}
    {/snippet}
    {#snippet detail(row)}
      {@const r = row.original as HistRow}
      {#if r.dayHeader}
        <div class="day-header">{r.dayHeader}</div>
      {/if}
      <span class="meta2">{r.resultCount} · {r.client} · {r.when}</span>
      {#if r.kind === "search"}
        <UiCollapsible open={r.clicks.length > 0}>
          {#snippet trigger(open)}
            <span class="clicks-chevron" class:open aria-hidden="true"
              ><CaretRightIcon size={12} style="vertical-align: -0.125em" /></span
            >{r.clicks.length} {r.clicksWord}
          {/snippet}
          {#each r.clicks as c}
            <div class="click-line">
              <span class="cdom">{c.domain}</span>
              <a href={c.url} target="_blank" rel="noopener">{c.title}</a>
              <span class="cpos">{h.position_prefix}{c.position}</span>
            </div>
          {/each}
        </UiCollapsible>
        <span class="row-actions">
          <a href={r.rerunUrl}>{h.rerun}</a>
          <a href={r.jsonUrl}>{h.copy_json}</a>
          {#if r.cachedLive}<a href={r.sourceUrl}>{h.payload}</a>{/if}
          <UiButton
            size="sm"
            variant="danger"
            onclick={() => page.remove(r)}>{h.delete}</UiButton
          >
          <span class="row-error" role="status">{r.error}</span>
        </span>
      {:else if r.kind === "answer"}
        <span class="row-actions">
          <a href={r.rerunUrl}>{h.rerun_answer}</a>
          <UiButton
            size="sm"
            variant="danger"
            onclick={() => page.remove(r)}>{h.delete}</UiButton
          >
          <span class="row-error" role="status">{r.error}</span>
        </span>
      {:else}
        {#each r.clicks as c}
          <div class="click-line">
            <span class="cdom">{c.domain}</span>
            <a href={c.url} target="_blank" rel="noopener">{c.title}</a>
            <span class="cpos">{h.position_prefix}{c.position}</span>
          </div>
        {/each}
      {/if}
    {/snippet}
  </UiDataTable>
{/if}

{#if s.capped}
  <div class="meta">
    <span>{h.capped_showing} {s.rows.length} — {h.capped_hint}</span>
  </div>
{/if}

<style>
  .clicks-chevron {
    display: inline-block;
    transition: transform 140ms var(--ease-out);
  }
  .clicks-chevron.open {
    transform: rotate(90deg);
  }
  .day-header {
    color: var(--muted);
    font-size: 0.8125rem;
    padding-top: 0.25rem;
  }
</style>
