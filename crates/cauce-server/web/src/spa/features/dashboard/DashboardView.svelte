<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/dashboard` markup — one-for-one with `templates/dashboard.html`:
  ?days=7|30 window links, the panel grid (searches/day SVG chart, hit
  rate, latency, clients, outcomes, reliability, top/zero queries,
  engine eval), the engines table and the cache block.
-->
<script lang="ts">
  import { appHref } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import type { createDashboardPage } from "./dashboard.svelte.js";
  import DayChart from "./DayChart.svelte";

  interface DashboardViewProps {
    page: ReturnType<typeof createDashboardPage>;
  }

  let { page }: DashboardViewProps = $props();
  const d = spa.dashboard;
  const s = $derived(page.state);
  const v = $derived(page.view());
</script>

<h1>{d.title}</h1>
<p class="meta window">
  {d.window}:
  <a
    href={appHref("/dashboard") + "?days=7"}
    aria-current={s.days === 7 ? "page" : undefined}>{d.days_7}</a
  >
  ·
  <a
    href={appHref("/dashboard") + "?days=30"}
    aria-current={s.days === 30 ? "page" : undefined}>{d.days_30}</a
  >
</p>

{#if s.loading}
  <p class="stats">…</p>
{:else if s.error}
  <p class="field-error" role="alert">{s.error}</p>
{:else if v}
  <div class="panels">
    <section class="panel">
      <h2 id="searches-per-day">{d.searches_per_day}</h2>
      {#if v.bars.length === 0}
        <p class="muted">{d.no_data}</p>
      {:else}
        <DayChart bars={v.bars} ariaLabelledby="searches-per-day" />
        <p class="legend">
          <span><i class="c"></i>{d.legend_cache}</span>
          <span><i class="n"></i>{d.legend_network}</span>
        </p>
      {/if}
    </section>

    <section class="panel">
      <h2>{d.hit_rate}</h2>
      {#if !v.hasData}
        <p class="muted">{d.no_data}</p>
      {:else}
        <p class="big">{v.hitRatePct}</p>
        <p class="muted">{v.totalHits} / {v.totalSearches}</p>
        {#if v.tierRows.length > 0}
          <ul class="flat">
            {#each v.tierRows as t}
              <li><span>tier {t.tier}</span><span>{t.hits} · {t.pct}</span></li>
            {/each}
          </ul>
        {/if}
      {/if}
    </section>

    <section class="panel">
      <h2>{d.latency}</h2>
      {#if !v.lat && !v.ttfr}
        <p class="muted">{d.no_data}</p>
      {:else}
        <dl class="kv">
          {#if v.ttfr}
            <dt>{d.ttfr}</dt>
            <dd>
              p50 {v.ttfr.p50_ms} ms · p90 {v.ttfr.p90_ms} ms · p99 {v.ttfr.p99_ms}
              ms
            </dd>
          {/if}
          {#if v.lat}
            <dt>{d.full}</dt>
            <dd>
              p50 {v.lat.p50_ms} ms · p90 {v.lat.p90_ms} ms · p99 {v.lat.p99_ms} ms
            </dd>
          {/if}
        </dl>
      {/if}
    </section>

    <section class="panel">
      <h2>{d.clients}</h2>
      {#if v.clients.length === 0}
        <p class="muted">{d.no_data}</p>
      {:else}
        <ul class="flat">
          {#each v.clients as c}
            <li><span>{c.name}</span><span>{c.count} · {c.pct}</span></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="panel">
      <h2>{d.outcomes}</h2>
      {#if v.outcomes.length === 0}
        <p class="muted">{d.no_data}</p>
      {:else}
        <ul class="flat">
          {#each v.outcomes as o}
            <li><span>{o.name}</span><span>{o.count} · {o.pct}</span></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="panel">
      <h2>{d.reliability}</h2>
      {#if !v.hasData && v.deadlineHits === 0 && v.staleServed === 0 && v.admissionRejected === 0}
        <p class="muted">{d.no_data}</p>
      {:else}
        <dl class="kv">
          <dt>{d.deadline_hits}</dt>
          <dd>{v.deadlineHits} · {v.deadlineRate}</dd>
          <dt>{d.stale_served}</dt>
          <dd>{v.staleServed}</dd>
          <dt>{d.admission_rejected}</dt>
          <dd>{v.admissionRejected}</dd>
        </dl>
      {/if}
    </section>

    <section class="panel">
      <h2>{d.top_queries}</h2>
      {#if v.topQueries.length === 0}
        <p class="muted">{d.no_data}</p>
      {:else}
        <ul class="flat">
          {#each v.topQueries as q}
            <li><span>{q.query}</span><span>{q.searches}</span></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="panel">
      <h2>{d.zero_results}</h2>
      {#if v.zeroQueries.length === 0}
        <p class="muted">{d.no_data}</p>
      {:else}
        <ul class="flat">
          {#each v.zeroQueries as q}
            <li><span>{q}</span></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="panel">
      <h2>{d.engine_eval}</h2>
      {#if !v.eval}
        <p class="muted">{d.eval_no_run}</p>
      {:else}
        <p class="muted">{v.eval.meta}</p>
        <ul class="flat">
          {#each v.eval.rows as e}
            <li><span>{e.engine}</span><span>{e.score}</span></li>
          {/each}
        </ul>
      {/if}
    </section>
  </div>

  <section class="panel">
    <h2>{d.engines}</h2>
    {#if v.engines.length === 0}
      <p class="muted">{d.no_engines}</p>
    {:else}
      <table class="stats">
        <thead>
          <tr>
            <th>{d.col_engine}</th>
            <th>{d.col_breaker}</th>
            <th>{d.col_reliability}</th>
            <th>{d.col_calls}</th>
            <th>{d.col_total}</th>
            <th>{d.col_http}</th>
            <th>{d.col_parse}</th>
          </tr>
        </thead>
        <tbody>
          {#each v.engines as e}
            <tr>
              <td data-label={d.col_engine}
                ><a href={appHref("/admin") + "?tab=engines#" + e.cardAnchor}
                  >{e.id}</a
                ></td
              >
              <td data-label={d.col_breaker}>{e.breaker}</td>
              <td data-label={d.col_reliability}>{e.reliability}</td>
              <td data-label={d.col_calls}>{e.requests}</td>
              <td data-label={d.col_total}>{e.total}</td>
              <td data-label={d.col_http}>{e.http}</td>
              <td data-label={d.col_parse}>{e.parse}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>

  <section class="panel">
    <h2>{d.cache}</h2>
    <dl class="kv">
      <dt>{d.cache_rows}</dt>
      <dd>{v.cacheRows} ({v.cacheExpired} expired)</dd>
      <dt>{d.cache_unexpired}</dt>
      <dd>{v.cacheUnexpired}</dd>
      <dt>{d.cache_db_size}</dt>
      <dd>{v.cacheDb}</dd>
      <dt>{d.cache_newest}</dt>
      <dd>{v.cacheNewest}</dd>
    </dl>
  </section>
{/if}

<style>
  .window a {
    color: var(--accent);
  }
  .window a[aria-current] {
    font-weight: 700;
    color: var(--fg);
    text-decoration: none;
  }
  .panels {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(18rem, 100%), 1fr));
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }
  .panel {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.75rem;
    min-width: 0;
  }
  .panel h2 {
    margin: 0 0 0.5rem;
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--muted);
    text-transform: lowercase;
  }
  .panel .big {
    font-size: 1.6rem;
    font-weight: 700;
  }
  .muted {
    color: var(--muted);
  }
  ul.flat {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 0.875rem;
  }
  ul.flat li {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    min-width: 0;
  }
  ul.flat li span {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  ul.flat li + li {
    border-top: 1px solid var(--border);
  }
  table.stats {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }
  table.stats th,
  table.stats td {
    text-align: left;
    padding: 0.25rem 0.4rem;
    border-bottom: 1px solid var(--border);
  }
  table.stats th {
    color: var(--muted);
    font-weight: 600;
  }
  @media (width < 640px) {
    table.stats,
    table.stats tbody {
      display: block;
      width: 100%;
    }
    table.stats thead {
      position: absolute;
      width: 1px;
      height: 1px;
      padding: 0;
      margin: -1px;
      overflow: hidden;
      clip: rect(0, 0, 0, 0);
      white-space: nowrap;
      border: 0;
    }
    table.stats tr {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 0.25rem 0.75rem;
      padding: 0.5rem 0;
      border-bottom: 1px solid var(--border);
    }
    table.stats td {
      min-width: 0;
      padding: 0.2rem 0;
      overflow-wrap: anywhere;
    }
    table.stats td::before {
      content: attr(data-label);
      display: block;
      color: var(--muted);
      font-size: 0.65rem;
      font-weight: 600;
    }
    table.stats td:first-child {
      grid-column: 1 / -1;
    }
  }
  .legend {
    font-size: 0.75rem;
    color: var(--muted);
    display: flex;
    gap: 0.75rem;
  }
  .legend i {
    display: inline-block;
    width: 0.6em;
    height: 0.6em;
    border-radius: 1px;
    margin-right: 0.25em;
  }
  .legend i.c {
    background: var(--accent);
  }
  .legend i.n {
    background: var(--muted);
    opacity: 0.45;
  }
  dl.kv {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.15rem 1rem;
    margin: 0;
    font-size: 0.875rem;
  }
  dl.kv dt {
    color: var(--muted);
  }
  dl.kv dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
