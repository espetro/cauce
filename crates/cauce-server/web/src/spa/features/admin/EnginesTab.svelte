<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin?tab=engines` — D6 rebuild from user review: the engine
  cards became a `ui/data-table` (TanStack v9, shadcn-base shape) so
  the listing reads like the cache and audit tables. Columns: engine,
  kind/tier, enabled, breaker chip, ewma, p95, reliability, requests
  today, and a trailing actions column (enable/disable toggle, reset
  breaker, run test). The per-engine stats the cards used to show in a
  `dl` move into an info-icon dialog (last ok / last error + the test
  form with its results), keeping the row itself scannable.

  Shared base: `/app/settings` renders its engines config section
  through the same `ui/data-table` primitive — one skin for both.
-->
<script lang="ts">
  import {
    ArrowClockwiseIcon,
    InfoIcon,
    PlayIcon,
    PowerIcon,
  } from "phosphor-svelte";
  import { spa } from "../../lib/i18n.js";
  import { hostOf } from "../../lib/format.js";
  import UiButton from "../../ui/button.svelte";
  import UiDataTable from "../../ui/data-table.svelte";
  import UiDialog from "../../ui/dialog.svelte";
  import type { createEnginesTab } from "./engines.svelte.js";
  import type { EngineCard } from "./engines.svelte.js";
  import type { ColumnDef } from "@tanstack/svelte-table";

  interface EnginesTabProps {
    tab: ReturnType<typeof createEnginesTab>;
  }

  let { tab }: EnginesTabProps = $props();
  const e = spa.engines;
  const s = $derived(tab.state);

  const columns: ColumnDef<any, any, any>[] = [
    { id: "engine", header: e.col_engine, accessorKey: "id", size: 200, enableSorting: true },
    { id: "kind", header: e.col_kind, accessorKey: "kindTier", size: 110, enableSorting: true },
    { id: "enabled", header: e.col_enabled, accessorKey: "enabledLabel", size: 90, enableSorting: true },
    { id: "breaker", header: e.col_breaker, accessorKey: "breaker", size: 150, enableSorting: false },
    {
      id: "ewma",
      header: e.col_ewma,
      accessorFn: (c: EngineCard) => c.ewma,
      size: 100,
      enableSorting: false,
    },
    { id: "p95", header: e.col_p95, accessorFn: (c: EngineCard) => c.p95, size: 100, enableSorting: false },
    {
      id: "reliability",
      header: e.col_reliability,
      accessorFn: (c: EngineCard) => c.reliability,
      size: 110,
      enableSorting: true,
    },
    {
      id: "requests",
      header: e.col_requests,
      accessorFn: (c: EngineCard) => c.requestsToday,
      size: 90,
      enableSorting: true,
    },
    { id: "actions", header: "", size: 120, enableSorting: false, enableResizing: false },
  ];

  let detailCard: EngineCard | null = $state(null);
  const detailOpen = $derived(detailCard != null);

  function showDetails(card: EngineCard): void {
    detailCard = card;
  }

  // The dialog renders the card from `s.cards` so state updates
  // (busy flags, test results) flow while it is open.
  const liveDetail = $derived(
    detailCard != null
      ? (s.cards.find((c) => c.id === detailCard!.id) ?? null)
      : null,
  );
</script>

{#if s.loading}
  <p class="stats">…</p>
{:else if s.error}
  <p class="field-error" role="alert">{s.error}</p>
{:else}
  <p class="stats">{s.summaryLine}</p>

  <UiDataTable
    {columns}
    data={s.cards}
    getRowId={(c) => c.id}
    caption={e.page_title}
    empty={e.empty}
    initialSorting={[{ id: "engine", desc: false }]}
  >
    {#snippet cell(cell)}
      {@const card = cell.row.original as EngineCard}
      {#if cell.column.id === "engine"}
        <strong>{card.id}</strong>
        {#if !card.configured || !card.live}
          <span class="meta flags">{#if !card.configured}{e.not_in_config}{/if}{#if !card.configured && !card.live} · {/if}{#if !card.live}{e.not_running}{/if}</span>
        {/if}
      {:else if cell.column.id === "breaker"}
        <span class="breaker breaker-{card.breakerClass}">{card.breaker}</span>
        {#if card.breakerNote !== ""}
          <span class="meta breaker-note">{card.breakerNote}</span>
        {/if}
      {:else if cell.column.id === "reliability"}
        {card.reliability}
      {:else if cell.column.id === "actions"}
        <span class="row-actions">
          {#if card.configured || card.live}
            <UiButton
              size="icon"
              variant="ghost"
              ariaLabel={card.toggleLabel}
              disabled={card.busy}
              onclick={() => tab.act(card, card.enabled ? "disable" : "enable")}
            >
              <PowerIcon size={14} />
            </UiButton>
          {/if}
          <UiButton
            size="icon"
            variant="ghost"
            ariaLabel={e.action_reset}
            disabled={card.busy}
            onclick={() => tab.act(card, "reset")}
          >
            <ArrowClockwiseIcon size={14} />
          </UiButton>
          <UiButton
            size="icon"
            variant="ghost"
            ariaLabel={e.action_details}
            onclick={() => showDetails(card)}
          >
            <InfoIcon size={14} />
          </UiButton>
        </span>
      {:else if cell.column.id === "requests"}
        {cell.getValue() as number}
      {:else}
        {cell.getValue() as string}
      {/if}
    {/snippet}
  </UiDataTable>

  {#if s.cards.some((c) => c.notice !== "")}
    <div class="notices">
      {#each s.cards.filter((c) => c.notice !== "") as card (card.id)}
        <p class="meta">{card.id}: {card.notice}</p>
      {/each}
    </div>
  {/if}
{/if}

<UiDialog
  open={detailOpen}
  onOpenChange={(o) => (o ? null : (detailCard = null))}
  title={liveDetail ? liveDetail.id : e.page_title}
>
  {#if liveDetail}
    <dl class="detail-stats">
      <dt>{e.stat_enabled}</dt>
      <dd>{liveDetail.enabledLabel}</dd>
      <dt>{e.stat_last_ok}</dt>
      <dd>{liveDetail.lastOk}</dd>
      <dt>{e.stat_last_error}</dt>
      <dd class="wrap">{liveDetail.lastError}</dd>
    </dl>

    <form
      class="test-form"
      onsubmit={(ev) => {
        ev.preventDefault();
        tab.runTest(liveDetail);
      }}
    >
      <input
        type="search"
        bind:value={liveDetail.test.q}
        placeholder={e.test_default}
        aria-label={e.test_aria}
      />
      <UiButton type="submit" size="sm" disabled={liveDetail.test.running}>
        <PlayIcon size={12} /> {e.action_run}
      </UiButton>
    </form>

    <div class="test-results">
      {#if liveDetail.test.running}
        <p class="meta">…</p>
      {:else if liveDetail.test.error !== ""}
        <p class="meta">{liveDetail.test.metaLine} {liveDetail.test.error}</p>
      {:else if liveDetail.test.metaLine !== ""}
        <p class="meta">{liveDetail.test.metaLine}</p>
        <div class="rows">
          {#each liveDetail.test.results as r}
            <article>
              <a href={r.url} target="_blank" rel="noopener">{r.title}</a>
              <span class="host">{hostOf(r.url)}</span>
              {#if r.snippet !== ""}<p class="snippet">{r.snippet}</p>{/if}
            </article>
          {/each}
        </div>
      {:else}
        <p class="meta">{e.test_aria}</p>
      {/if}
    </div>
  {/if}
</UiDialog>

<style>
  .flags {
    font-size: 0.75rem;
    margin-left: 0.375rem;
  }
  .breaker {
    display: inline-block;
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.0625rem 0.5rem;
    font-size: 0.75rem;
  }
  .breaker-open {
    border-color: var(--warn);
    color: var(--warn);
  }
  .breaker-half-open {
    color: var(--accent);
    border-color: var(--accent);
  }
  .breaker-note {
    display: block;
    font-size: 0.75rem;
  }
  .row-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.125rem;
  }
  .notices {
    margin-top: 0.5rem;
  }
  .notices p {
    margin: 0.125rem 0;
  }
  .detail-stats {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.25rem 1rem;
    margin: 0 0 0.75rem;
    font-size: 0.875rem;
  }
  .detail-stats dt {
    color: var(--muted);
  }
  .detail-stats dd {
    margin: 0;
    min-width: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    overflow-wrap: anywhere;
  }
  .test-form {
    display: flex;
    gap: 0.4rem;
    margin-bottom: 0.75rem;
  }
  .test-form input {
    flex: 1 1 auto;
    min-width: 0;
    height: var(--control-h);
    padding: 0 0.625rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font-size: 0.8125rem;
  }
  .test-results .rows article {
    padding: 0.375rem 0;
    border-bottom: 1px solid var(--border);
    font-size: 0.8125rem;
  }
  .test-results .rows article:last-child {
    border-bottom: 0;
  }
  .host {
    color: var(--muted);
    margin-left: 0.375rem;
    font-size: 0.75rem;
  }
  .snippet {
    margin: 0.125rem 0 0;
    color: var(--muted);
  }
</style>
