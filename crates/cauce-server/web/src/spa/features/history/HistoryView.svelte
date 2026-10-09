<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/history` markup — one-for-one with `templates/history.html`:
  filter bar (since / origin / q / cached-only), stats line omitted
  (`history_stats` has no wire twin — see #266), day-header row groups,
  detail row per item with nested clicks + row actions + row delete.
  Interactive controls ride the ui/ wrappers (plan §3 DS-09): UiSelect
  facet selects, UiCheckbox cached-only, UiCollapsible per-row clicks,
  UiButton submit + row delete.
-->
<script lang="ts">
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import UiButton from "../../ui/button.svelte";
  import UiCheckbox from "../../ui/checkbox.svelte";
  import UiCollapsible from "../../ui/collapsible.svelte";
  import UiSelect from "../../ui/select.svelte";
  import type { createHistoryPage } from "./history.svelte.js";

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
</script>

<h1>{h.title}</h1>

<form
  class="filters"
  action="/app/history"
  method="get"
  onsubmit={(e) => {
    e.preventDefault();
    page.submit(navigate);
  }}
>
  <UiSelect
    name="since"
    ariaLabel={h.since_label}
    value={s.since}
    onValueChange={(v) => (s.since = v)}
    options={sinceOptions}
  />
  <UiSelect
    name="origin"
    ariaLabel={h.origin_label}
    value={s.origin}
    onValueChange={(v) => (s.origin = v)}
    options={originOptions}
  />
  <input
    type="search"
    name="q"
    bind:value={s.q}
    placeholder={h.query_placeholder}
    aria-label={h.query_label}
  />
  <label class="cached-only">
    <UiCheckbox
      checked={s.cached}
      onCheckedChange={(v) => (s.cached = v)}
      ariaLabel={h.cached_only}
    />
    {h.cached_only}
  </label>
  <UiButton type="submit">{h.filter_submit}</UiButton>
  {#if page.filtersActive()}
    <a class="clear" href="/app/history">{h.clear}</a>
  {/if}
</form>

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
  <table class="data history">
    <thead>
      <tr>
        <th class="c-when">{h.col_when}</th>
        <th class="c-query">{h.col_query}</th>
        <th class="c-source">{h.col_source}</th>
        <th class="c-engines h-md">{h.col_engines}</th>
        <th class="c-n">{h.col_results}</th>
        <th class="c-ms h-md">{h.col_latency}</th>
        <th class="c-by">{h.col_client}</th>
      </tr>
    </thead>
    {#each s.rows as row (row.kind + "-" + row.id + "-" + row.when)}
      {#if row.gone}{:else}
        {#if row.dayHeader}
          <tbody class="day-group">
            <tr class="day"><td colspan="7">{row.dayHeader}</td></tr>
          </tbody>
        {/if}
        <tbody>
          <tr class={row.kind}>
            <td class="c-when">{row.when}</td>
            <td class="c-query">
              {#if row.kind === "search" || row.kind === "answer"}
                <a href={row.queryUrl}>{row.query}</a>
                {#if row.queryChip}<span class="meta-chip">{row.queryChip}</span>{/if}
              {:else}
                {h.click_only}
              {/if}
            </td>
            <td class="c-source">
              {#if row.sourceUrl}<a href={row.sourceUrl}>{row.source}</a
                >{:else}{row.source}{/if}
            </td>
            <td class="c-engines h-md">{row.engines}</td>
            <td class="c-n">{row.resultCount}</td>
            <td class="c-ms h-md">{row.latency}</td>
            <td class="c-by">
              {row.client}
              {#if row.originChip}<span class="meta-chip">{h.chip_agent}</span>{/if}
            </td>
          </tr>
          <tr class="detail">
            <td colspan="7">
              <span class="meta2">{row.resultCount} · {row.client} · {row.when}</span>
              {#if row.kind === "search"}
                <UiCollapsible open={row.clicks.length > 0}>
                  {#snippet trigger(open)}
                    <span class="clicks-chevron" class:open aria-hidden="true"
                      >▸</span
                    >{row.clicks.length} {row.clicksWord}
                  {/snippet}
                  {#each row.clicks as c}
                    <div class="click-line">
                      <span class="cdom">{c.domain}</span>
                      <a href={c.url} target="_blank" rel="noopener">{c.title}</a>
                      <span class="cpos">{h.position_prefix}{c.position}</span>
                    </div>
                  {/each}
                </UiCollapsible>
                <span class="row-actions">
                  <a href={row.rerunUrl}>{h.rerun}</a>
                  <a href={row.jsonUrl}>{h.copy_json}</a>
                  {#if row.cachedLive}<a href={row.sourceUrl}>{h.payload}</a>{/if}
                  <UiButton
                    size="sm"
                    variant="danger"
                    onclick={() => page.remove(row)}>{h.delete}</UiButton
                  >
                  <span class="row-error" role="status">{row.error}</span>
                </span>
              {:else if row.kind === "answer"}
                <span class="row-actions">
                  <a href={row.rerunUrl}>{h.rerun_answer}</a>
                  <UiButton
                    size="sm"
                    variant="danger"
                    onclick={() => page.remove(row)}>{h.delete}</UiButton
                  >
                  <span class="row-error" role="status">{row.error}</span>
                </span>
              {:else}
                {#each row.clicks as c}
                  <div class="click-line">
                    <span class="cdom">{c.domain}</span>
                    <a href={c.url} target="_blank" rel="noopener">{c.title}</a>
                    <span class="cpos">{h.position_prefix}{c.position}</span>
                  </div>
                {/each}
              {/if}
            </td>
          </tr>
        </tbody>
      {/if}
    {/each}
  </table>
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
</style>
