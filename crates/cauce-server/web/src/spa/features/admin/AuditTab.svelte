<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin?tab=audit` — one-for-one with `templates/audit.html`:
  count + cap note, the audit table, and `request_id` links to the
  still-HTMX `/trace/{id}` page.

  D6: the table renders through `ui/data-table` (TanStack Table v9) —
  sortable actor/action columns, resizable columns. Two user-review
  changes:
  - the facet selects moved off the page and into a filter popover
    (a filter icon beside the count line opens actor/action selects);
  - active filters render as a chip row above the table, each chip an
    X button that drops the filter. The URL query string stays the
    source of truth (`?actor=&action=`) — chips and popover reads come
    from `route.params`, writes go through `navigate()`, so a chip X
    and the popover agree without shared local state.
  - the JSON details moved into the last column as an icon action
    opening a `ui/dialog` modal.
-->
<script lang="ts">
  import { Popover } from "bits-ui";
  import {
    FunnelIcon,
    ListIcon,
    XIcon,
  } from "phosphor-svelte";
  import { navigate, route } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import UiButton from "../../ui/button.svelte";
  import UiSelect from "../../ui/select.svelte";
  import UiDialog from "../../ui/dialog.svelte";
  import UiDataTable from "../../ui/data-table.svelte";
  import type { createAuditTab } from "./audit.svelte.js";
  import type { ColumnDef } from "@tanstack/svelte-table";

  interface AuditTabProps {
    tab: ReturnType<typeof createAuditTab>;
  }

  let { tab }: AuditTabProps = $props();
  const a = spa.audit;
  const s = $derived(tab.state);

  // The URL is the filter state. `run()` copies the params into the
  // tab state on mount; the popover and the chips read the same
  // source so they cannot disagree.
  const actor = $derived(route.params.get("actor") ?? "");
  const action = $derived(route.params.get("action") ?? "");

  const actorOptions = $derived([
    { value: "", label: a.any },
    ...s.actorOptions.map((o) => ({ value: o, label: o })),
  ]);
  const actionOptions = $derived([
    { value: "", label: a.any },
    ...s.actionOptions.map((o) => ({ value: o, label: o })),
  ]);

  function applyFilters(nextActor: string, nextAction: string): void {
    const p = new URLSearchParams({ tab: "audit" });
    if (nextActor !== "") p.set("actor", nextActor);
    if (nextAction !== "") p.set("action", nextAction);
    navigate("/app/admin?" + p.toString());
  }

  function dropChip(key: "actor" | "action"): void {
    if (key === "actor") applyFilters("", action);
    else applyFilters(actor, "");
  }

  const columns: ColumnDef<any, any, any>[] = [
    { id: "ts", header: a.when_column, accessorKey: "ts", size: 150, enableSorting: false },
    { id: "actor", header: a.actor_column, accessorKey: "actor", size: 130, enableSorting: true },
    { id: "action", header: a.action_column, accessorKey: "action", size: 150, enableSorting: true },
    { id: "target", header: a.target_column, accessorKey: "target", size: 300 },
    { id: "request", header: a.request_column, accessorKey: "shortRequestId", size: 140 },
    // Details ride the last column as an icon action.
    { id: "details", header: "", size: 60, enableSorting: false },
  ];

  // The open details dialog: the row whose JSON is showing. One dialog
  // for the whole table (no per-row portal spam); its content swaps
  // with the selection.
  let detailRow: { key: string; details: string } | null = $state(null);
  const detailOpen = $derived(detailRow != null);

  function showDetails(row: any): void {
    detailRow = row.details === "" ? null : { key: row.key, details: row.details };
  }

  function closeDetails(): void {
    detailRow = null;
  }

  let filterOpen = $state(false);
  // Draft selects inside the popover; applied on Apply.
  let draftActor = $state("");
  let draftAction = $state("");

  function openFilter(): void {
    draftActor = actor;
    draftAction = action;
    filterOpen = true;
  }
</script>

<p class="meta head">
  <span>
    {s.countLine}
    {#if s.capped}
      · {a.cap_note_prefix}
      {tab.LIST_LIMIT} · {a.cap_note_suffix}
    {/if}
  </span>
  <Popover.Root open={filterOpen} onOpenChange={(o) => (filterOpen = o)}>
    <Popover.Trigger>
      {#snippet child({ props })}
        <UiButton
          {...props}
          size="icon"
          variant="ghost"
          ariaLabel={a.filter}
          onclick={openFilter}
        >
          <FunnelIcon size={14} />
        </UiButton>
      {/snippet}
    </Popover.Trigger>
    <Popover.Portal>
      <Popover.Content class="audit-filter-popover" sideOffset={6} align="end">
        <div class="filter-fields">
          <UiSelect
            ariaLabel={a.actor_label}
            value={draftActor}
            onValueChange={(v) => (draftActor = v)}
            options={actorOptions}
          />
          <UiSelect
            ariaLabel={a.action_label}
            value={draftAction}
            onValueChange={(v) => (draftAction = v)}
            options={actionOptions}
          />
        </div>
        <div class="filter-actions">
          <UiButton
            size="sm"
            onclick={() => {
              filterOpen = false;
              applyFilters(draftActor, draftAction);
            }}>{a.filter}</UiButton
          >
          {#if actor !== "" || action !== ""}
            <UiButton
              size="sm"
              variant="ghost"
              onclick={() => {
                filterOpen = false;
                applyFilters("", "");
              }}>{a.clear}</UiButton
            >
          {/if}
        </div>
      </Popover.Content>
    </Popover.Portal>
  </Popover.Root>
</p>

{#if actor !== "" || action !== ""}
  <div class="chips" role="list" aria-label={a.filter}>
    {#if actor !== ""}
      <span class="chip" role="listitem">
        {a.actor_label}: {actor}
        <button
          type="button"
          class="chip-x"
          aria-label="remove {a.actor_label}"
          onclick={() => dropChip("actor")}
        >
          <XIcon size={10} />
        </button>
      </span>
    {/if}
    {#if action !== ""}
      <span class="chip" role="listitem">
        {a.action_label}: {action}
        <button
          type="button"
          class="chip-x"
          aria-label="remove {a.action_label}"
          onclick={() => dropChip("action")}
        >
          <XIcon size={10} />
        </button>
      </span>
    {/if}
  </div>
{/if}

{#if s.loading}
  <p class="stats">…</p>
{:else if s.error}
  <p class="field-error" role="alert">{s.error}</p>
{:else if s.rows.length === 0}
  <p class="empty">{s.emptyLine}</p>
{:else}
  <UiDataTable
    {columns}
    data={s.rows}
    getRowId={(r) => r.key}
    caption={a.table_region}
    empty={s.emptyLine}
  >
    {#snippet cell(c)}
      {@const row = c.row.original}
      {#if c.column.id === "action"}
        <code>{row.action}</code>
      {:else if c.column.id === "target"}
        <span class="target" title={row.target}>{row.target}</span>
      {:else if c.column.id === "request"}
        {#if row.requestId !== ""}
          <a
            class="request-id"
            href={"/trace/" + row.requestId}
            title={row.requestId}>{row.shortRequestId}</a
          >
        {:else}
          {spa.common.dash}
        {/if}
      {:else if c.column.id === "details"}
        {#if row.details !== ""}
          <UiButton
            size="icon"
            variant="ghost"
            ariaLabel={a.details_summary}
            onclick={() => showDetails(row)}
          >
            <ListIcon size={14} />
          </UiButton>
        {:else}
          {spa.common.dash}
        {/if}
      {:else}
        {c.getValue() as string}
      {/if}
    {/snippet}
  </UiDataTable>
{/if}

<UiDialog open={detailOpen} onOpenChange={(o) => (o ? null : closeDetails())} title={a.details_summary}>
  {#if detailRow}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- tabindex=0 keeps the scrollable pre keyboard scrollable;
         ScrollArea declined (plan §3.6). -->
    <pre tabindex="0" role="group" aria-label={a.details_summary}>{detailRow.details}</pre>
  {/if}
</UiDialog>

<style>
  /* Styles live in AuditTab.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./AuditTab.css";
</style>
