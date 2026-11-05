<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/history` markup — the D6 rebuild from user review: the feed is
  a `ui/data-table` (TanStack Table v9) with the same skin as the
  cache, engines and audit tables.

  The rows are a *day-grouped feed*, not a sortable dataset: the
  server already returns newest-first and the day headers only make
  sense in that order, so every column is non-sortable. The day
  separator rides the `detail` snippet (a full-width row under the
  first row of each day), and the per-row actions (re-run, copy JSON,
  payload, delete) live in the trailing actions column as icon
  actions, matching the cache and engines tables. The click list
  stays an inline collapsible in the query cell — expanding it must
  not push the table's row rhythm.

  Interactive controls ride the ui/ wrappers (plan §3 DS-09): UiSelect
  facet selects, UiCheckbox cached-only, UiCollapsible per-row
  clicks, UiButton submit + row delete.
-->
<script lang="ts">
  import {
    ArrowClockwiseIcon,
    CaretRightIcon,
    CopyIcon,
    InfoIcon,
    TrashIcon,
  } from "phosphor-svelte";
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

  // The column model. The feed is chronological, so no column sorts:
  // a sort would scramble the day grouping the server ordered.
  const columns: ColumnDef<any, any, any>[] = [
    { id: "when", header: h.col_when, accessorKey: "when", size: 90, enableSorting: false },
    { id: "query", header: h.col_query, accessorKey: "query", size: 420, enableSorting: false },
    { id: "source", header: h.col_source, accessorKey: "source", size: 200, enableSorting: false },
    { id: "engines", header: h.col_engines, accessorKey: "engines", size: 180, enableSorting: false },
    {
      id: "resultCount",
      header: h.col_results,
      accessorFn: (r: HistRow) => Number(r.resultCount),
      size: 70,
      enableSorting: false,
    },
    { id: "latency", header: h.col_latency, accessorKey: "latency", size: 80, enableSorting: false },
    { id: "client", header: h.col_client, accessorKey: "client", size: 110, enableSorting: false },
    { id: "actions", header: "", size: 150, enableSorting: false, enableResizing: false },
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
        <span class="when">
          {#if row.dayHeader}<span class="day-header">{row.dayHeader}</span>{:else}{row.when}{/if}
        </span>
      {:else if c.column.id === "query"}
        {#if row.kind === "search" || row.kind === "answer"}
          <a href={row.queryUrl}>{row.query}</a>
          {#if row.queryChip}<span class="meta-chip">{row.queryChip}</span>{/if}
        {:else}
          {h.click_only}
        {/if}
        {#if row.clicks.length > 0}
          <UiCollapsible>
            {#snippet trigger(open)}
              <span class="clicks-chevron" class:open aria-hidden="true"
                ><CaretRightIcon size={12} style="vertical-align: -0.125em" /></span
              >{row.clicks.length} {row.clicksWord}
            {/snippet}
            <div class="click-list">
              {#each row.clicks as click}
                <div class="click-line">
                  <span class="cdom">{click.domain}</span>
                  <a href={click.url} target="_blank" rel="noopener">{click.title}</a>
                  <span class="cpos">{h.position_prefix}{click.position}</span>
                </div>
              {/each}
            </div>
          </UiCollapsible>
        {/if}
      {:else if c.column.id === "source"}
        {#if row.sourceUrl}<a href={row.sourceUrl}>{row.source}</a
          >{:else}{row.source}{/if}
      {:else if c.column.id === "client"}
        {row.client}
        {#if row.originChip}<span class="meta-chip">{h.chip_agent}</span>{/if}
      {:else if c.column.id === "actions"}
        <span class="row-actions">
          {#if row.kind === "answer"}
            <UiButton
              size="icon"
              variant="ghost"
              ariaLabel={h.rerun_answer}
              href={row.rerunUrl}
            >
              <ArrowClockwiseIcon size={14} />
            </UiButton>
          {:else if row.kind === "search"}
            <UiButton
              size="icon"
              variant="ghost"
              ariaLabel={h.rerun}
              href={row.rerunUrl}
            >
              <ArrowClockwiseIcon size={14} />
            </UiButton>
            <UiButton
              size="icon"
              variant="ghost"
              ariaLabel={h.copy_json}
              href={row.jsonUrl}
            >
              <CopyIcon size={14} />
            </UiButton>
            {#if row.cachedLive}
              <UiButton
                size="icon"
                variant="ghost"
                ariaLabel={h.payload}
                href={row.sourceUrl}
              >
                <InfoIcon size={14} />
              </UiButton>
            {/if}
          {/if}
          <UiButton
            size="icon"
            variant="ghost"
            ariaLabel={h.delete}
            onclick={() => page.remove(row)}
          >
            <TrashIcon size={14} />
          </UiButton>
        </span>
      {:else}
        {c.getValue() as string}
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
  .when {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .clicks-chevron {
    display: inline-block;
    transition: transform 140ms var(--ease-out);
    margin-left: 0.25rem;
  }
  .clicks-chevron.open {
    transform: rotate(90deg);
  }
  .click-list {
    margin: 0.25rem 0 0.25rem 0.875rem;
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }
  .click-line {
    display: flex;
    align-items: baseline;
    gap: 0.375rem;
    font-size: 0.8125rem;
  }
  .cdom {
    color: var(--muted);
    flex: none;
  }
  .click-line a {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cpos {
    color: var(--muted);
    flex: none;
  }
  .row-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.125rem;
  }
  .day-header {
    display: block;
    color: var(--accent);
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
</style>
