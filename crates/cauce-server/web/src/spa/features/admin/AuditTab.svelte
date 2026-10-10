<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin?tab=audit` — one-for-one with `templates/audit.html`:
  actor/action facet selects (UiSelect, "" stays the `any` sentinel),
  count + cap note, the `.audit-table` (target + actor columns collapse
  under 700px; the action cell re-prefixes `actor ·` at narrow widths),
  per-row UiCollapsible JSON cells, and `request_id` links to the
  still-HTMX `/trace/{id}` page.
-->
<script lang="ts">
  import { CaretRightIcon } from "phosphor-svelte";
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import UiButton from "../../ui/button.svelte";
  import UiCollapsible from "../../ui/collapsible.svelte";
  import UiSelect from "../../ui/select.svelte";
  import type { createAuditTab } from "./audit.svelte.js";

  interface AuditTabProps {
    tab: ReturnType<typeof createAuditTab>;
  }

  let { tab }: AuditTabProps = $props();
  const a = spa.audit;
  const s = $derived(tab.state);

  const actorOptions = $derived([
    { value: "", label: a.any },
    ...s.actorOptions.map((o) => ({ value: o, label: o })),
  ]);
  const actionOptions = $derived([
    { value: "", label: a.any },
    ...s.actionOptions.map((o) => ({ value: o, label: o })),
  ]);
</script>

<form
  class="filters"
  onsubmit={(e) => {
    e.preventDefault();
    tab.submit(navigate);
  }}
>
  <UiSelect
    name="actor"
    ariaLabel={a.actor_label}
    value={s.actor}
    onValueChange={(v) => (s.actor = v)}
    options={actorOptions}
  />
  <UiSelect
    name="action"
    ariaLabel={a.action_label}
    value={s.action}
    onValueChange={(v) => (s.action = v)}
    options={actionOptions}
  />
  <UiButton type="submit">{a.filter}</UiButton>
  {#if s.filtered}<a href="/app/admin?tab=audit">{a.clear}</a>{/if}
</form>

<p class="meta">
  {s.countLine}
  {#if s.capped}
    · {a.cap_note_prefix}
    {tab.LIST_LIMIT} · {a.cap_note_suffix}
  {/if}
</p>

{#if s.loading}
  <p class="stats">…</p>
{:else if s.error}
  <p class="field-error" role="alert">{s.error}</p>
{:else if s.rows.length === 0}
  <p class="empty">{s.emptyLine}</p>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- tabindex=0 keeps the horizontally scrollable region keyboard
       scrollable (HTMX parity: the table wrap was tab-focusable). -->
  <div
    class="audit-table-wrap"
    tabindex="0"
    role="region"
    aria-label={a.table_region}
  >
    <table class="audit-table">
      <thead>
        <tr>
          <th>{a.when_column}</th>
          <th class="c-actor">{a.actor_column}</th>
          <th class="c-action">
            <span class="actor-prefix">{a.actor_column} · </span>{a.action_column}
          </th>
          <th class="col-target">{a.target_column}</th>
          <th>{a.request_column}</th>
          <th>{a.details_summary}</th>
        </tr>
      </thead>
      <tbody>
        {#each s.rows as row (row.key)}
          <tr>
            <td>{row.ts}</td>
            <td class="c-actor">{row.actor}</td>
            <td class="c-action"
              ><span class="actor-prefix">{row.actor} · </span><code
                >{row.action}</code
              ></td
            >
            <td class="target col-target" title={row.target}>{row.target}</td>
            <td>
              {#if row.requestId !== ""}
                <a
                  class="request-id"
                  href={"/trace/" + row.requestId}
                  title={row.requestId}>{row.shortRequestId}</a
                >
              {:else}
                {spa.common.dash}
              {/if}
            </td>
            <td class="details-cell">
              {#if row.details !== ""}
                <UiCollapsible>
                  {#snippet trigger(open)}
                    <span class="details-summary">
                      <span class="details-chevron" class:open aria-hidden="true"
                        ><CaretRightIcon size={12} style="vertical-align: -0.125em" /></span
                      >{a.details_summary}
                    </span>
                  {/snippet}
                  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                  <!-- tabindex=0 keeps the scrollable pre keyboard
                       scrollable; ScrollArea declined (plan §3.6). -->
                  <pre tabindex="0" role="group" aria-label={a.details_summary}
                    >{row.details}</pre
                  >
                </UiCollapsible>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .audit-table-wrap {
    overflow-x: auto;
    max-width: 100%;
  }
  .audit-table {
    border-collapse: collapse;
    width: 100%;
    font-size: 0.8125rem;
  }
  .audit-table td,
  .audit-table th {
    border: 1px solid var(--border);
    padding: 0.3rem 0.5rem;
    text-align: left;
    white-space: nowrap;
  }
  .audit-table td.target {
    max-width: 16rem;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .audit-table td.details-cell {
    white-space: normal;
    min-width: 0;
  }
  .details-summary {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }
  .details-chevron {
    display: inline-block;
    transition: transform 140ms var(--ease-out);
  }
  .details-chevron.open {
    transform: rotate(90deg);
  }
  .audit-table td.details-cell pre {
    max-width: 28rem;
    max-height: 16rem;
    overflow: auto;
    font-size: 0.75rem;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .audit-table .actor-prefix {
    display: none;
  }
  @media (max-width: 700px) {
    .audit-table .col-target {
      display: none;
    }
    .audit-table .c-actor {
      display: none;
    }
    .audit-table .actor-prefix {
      display: inline;
      color: var(--muted);
    }
  }
</style>
