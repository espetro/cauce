<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/dashboard` markup — one-for-one with `templates/dashboard.html`:
  ?days=7|30 window segment, the panel grid (searches/day SVG chart, hit
  rate, latency, clients, outcomes, reliability, top/zero queries,
  engine eval), the engines table and the cache block.
-->
<script lang="ts">
  import { appHref, navigate } from "../../app/router.svelte.js";
  import { capabilities } from "../../lib/capabilities.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import UiToggleGroup from "../../ui/toggle-group.svelte";
  import type { createDashboardPage } from "./dashboard.svelte.js";
  import DayChart from "./DayChart.svelte";

  interface DashboardViewProps {
    page: ReturnType<typeof createDashboardPage>;
  }

  let { page }: DashboardViewProps = $props();
  const d = spa.dashboard;
  const s = $derived(page.state);
  const v = $derived(page.view());

  // ToggleGroup single writes "" when the pressed item is re-pressed;
  // the window must always resolve to a days value. The setter ignores
  // "", and because the binding is a getter/setter pair the group keeps
  // reading `s.days` instead of drifting to a local fallback. The
  // ?days= navigation remounts the route (`{#key routeKey}`) and is
  // also the deep-link path — `/app/dashboard?days=30` loads the 30-day
  // window straight from the URL.
  function pickDays(days: string) {
    if (days === "7" || days === "30") {
      navigate(appHref("/dashboard") + "?days=" + days);
    }
  }
</script>

<h1 class="page-title">{d.title}</h1>

{#if capabilities.loaded && !capabilities.flags.sharedStats}
  <!-- FX-07: the public dashboard is a single instance card — ops
       telemetry lives behind the admin surface (`/app/admin`). -->
  <section class="panel instance-card">
    <h2>{capabilities.instanceName}</h2>
    <dl class="kv">
      <dt>{spa.app.instance_card}</dt>
      <dd>{capabilities.instanceName}</dd>
      <dt>{spa.app.instance_version}</dt>
      <dd>{capabilities.version}</dd>
      <dt>{spa.app.instance_engines}</dt>
      <dd>{capabilities.engineCount}</dd>
    </dl>
    <p class="muted">{spa.app.instance_privacy}</p>
  </section>
{:else}
<div class="meta window">
  {d.window}:
  <UiToggleGroup
    bind:value={() => String(s.days), pickDays}
    options={[
      { value: "7", label: d.days_7 },
      { value: "30", label: d.days_30 },
    ]}
    ariaLabel={d.window}
  />
</div>

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
{/if}

<style>
  /* Styles live in DashboardView.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./DashboardView.css";
</style>
