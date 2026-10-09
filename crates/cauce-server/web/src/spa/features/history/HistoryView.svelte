<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/history` markup — one-for-one with `templates/history.html`:
  filter bar (since / origin / q / cached-only), stats line omitted
  (`history_stats` has no wire twin — see #266), day-header row groups,
  detail row per item with nested clicks + row actions + row delete.
-->
<script lang="ts">
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import type { createHistoryPage } from "./history.svelte.js";

  interface HistoryViewProps {
    page: ReturnType<typeof createHistoryPage>;
  }

  let { page }: HistoryViewProps = $props();
  const h = spa.history;
  const s = $derived(page.state);
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
  <select name="since" bind:value={s.since} aria-label={h.since_label}>
    <option value="all">{h.window_all}</option>
    <option value="24h">{h.window_24h}</option>
    <option value="7d">{h.window_7d}</option>
    <option value="30d">{h.window_30d}</option>
  </select>
  <select name="origin" bind:value={s.origin} aria-label={h.origin_label}>
    <option value="user">{h.origin_mine}</option>
    <option value="agent">{h.origin_agent}</option>
    <option value="all">{h.origin_all}</option>
  </select>
  <input
    type="search"
    name="q"
    bind:value={s.q}
    placeholder={h.query_placeholder}
    aria-label={h.query_label}
  />
  <label class="cached-only">
    <input type="checkbox" bind:checked={s.cached} />
    {h.cached_only}
  </label>
  <button type="submit">{h.filter_submit}</button>
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
                <details open={row.clicks.length > 0}>
                  <summary>{row.clicks.length} {row.clicksWord}</summary>
                  {#each row.clicks as c}
                    <div class="click-line">
                      <span class="cdom">{c.domain}</span>
                      <a href={c.url} target="_blank" rel="noopener">{c.title}</a>
                      <span class="cpos">{h.position_prefix}{c.position}</span>
                    </div>
                  {/each}
                </details>
                <span class="row-actions">
                  <a href={row.rerunUrl}>{h.rerun}</a>
                  <a href={row.jsonUrl}>{h.copy_json}</a>
                  {#if row.cachedLive}<a href={row.sourceUrl}>{h.payload}</a>{/if}
                  <button type="button" onclick={() => page.remove(row)}
                    >{h.delete}</button
                  >
                  <span class="row-error" role="status">{row.error}</span>
                </span>
              {:else if row.kind === "answer"}
                <span class="row-actions">
                  <a href={row.rerunUrl}>{h.rerun_answer}</a>
                  <button type="button" onclick={() => page.remove(row)}
                    >{h.delete}</button
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
