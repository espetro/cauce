<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin?tab=engines` — one-for-one with `templates/engines.html` +
  `engine_card.html`: summary line, then one `.engine-card` per
  `EngineView` (breaker chip + note, stat dl, reset / enable-disable /
  inline-test actions, notice line, test results).
-->
<script lang="ts">
  import { spa } from "../../lib/i18n.js";
  import { hostOf } from "../../lib/format.js";
  import type { createEnginesTab } from "./engines.svelte.js";

  interface EnginesTabProps {
    tab: ReturnType<typeof createEnginesTab>;
  }

  let { tab }: EnginesTabProps = $props();
  const e = spa.engines;
  const s = $derived(tab.state);
</script>

{#if s.loading}
  <p class="stats">…</p>
{:else if s.error}
  <p class="field-error" role="alert">{s.error}</p>
{:else}
  <p class="stats">{s.summaryLine}</p>
  {#if s.cards.length === 0}
    <p class="empty">{e.empty}</p>
  {/if}
  {#each s.cards as card (card.id)}
    <article class="engine-card" id={card.sel}>
      <header class="engine-head">
        <strong>{card.id}</strong>
        <span class="meta engine-kind">
          {card.kindTier}{#if !card.configured} · {e.not_in_config}{/if}{#if !card.live} · {e.not_running}{/if}
        </span>
        <span class="breaker breaker-{card.breakerClass}">{card.breaker}</span>
        {#if card.breakerNote !== ""}
          <span class="breaker-note meta">{card.breakerNote}</span>
        {/if}
      </header>
      <dl class="engine-stats">
        <dt>{e.stat_enabled}</dt>
        <dd>{card.enabledLabel}</dd>
        <dt>{e.stat_ewma}</dt>
        <dd>{card.ewma}</dd>
        <dt>{e.stat_last_ok}</dt>
        <dd>{card.lastOk}</dd>
        <dt>{e.stat_last_error}</dt>
        <dd class="wrap">{card.lastError}</dd>
        <dt>{e.stat_p95}</dt>
        <dd>{card.p95}</dd>
        <dt>{e.stat_reliability}</dt>
        <dd>{card.reliability}</dd>
        <dt>{e.stat_requests}</dt>
        <dd>{card.requestsToday}</dd>
      </dl>
      <div class="engine-actions">
        <button
          type="button"
          disabled={card.busy}
          onclick={() => tab.act(card, "reset")}>{e.action_reset}</button
        >
        {#if card.configured || card.live}
          <button
            type="button"
            disabled={card.busy}
            onclick={() =>
              tab.act(card, card.enabled ? "disable" : "enable")}
            >{card.toggleLabel}</button
          >
        {/if}
        <form
          onsubmit={(ev) => {
            ev.preventDefault();
            tab.runTest(card);
          }}
        >
          <input
            type="search"
            bind:value={card.test.q}
            placeholder={e.test_default}
            aria-label={e.test_aria}
          />
          <button type="submit" disabled={card.test.running}
            >{e.action_run}</button
          >
        </form>
      </div>
      {#if card.notice !== ""}
        <p class="meta engine-notice">{card.notice}</p>
      {/if}
      <div class="test-results">
        {#if card.test.running}
          <p class="meta">…</p>
        {:else if card.test.error !== ""}
          <p class="meta test-meta">{card.test.metaLine} {card.test.error}</p>
        {:else if card.test.metaLine !== ""}
          <p class="meta test-meta">{card.test.metaLine}</p>
          <div class="rows">
            {#each card.test.results as r}
              <article>
                <a href={r.url} target="_blank" rel="noopener">{r.title}</a>
                <span class="host">{hostOf(r.url)}</span>
                {#if r.snippet !== ""}<p class="snippet">{r.snippet}</p>{/if}
              </article>
            {/each}
          </div>
        {/if}
      </div>
    </article>
  {/each}
{/if}

<style>
  .engine-card {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.75rem 1rem;
    margin-bottom: 0.75rem;
    min-width: 0;
  }
  .engine-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.5rem;
  }
  .engine-kind {
    font-size: 0.8125rem;
  }
  .breaker {
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
    font-size: 0.75rem;
  }
  /* One label/value pair per row (HTMX parity): `auto 1fr` keeps each
     dd beside its own dt — an auto-fit multi-column grid would zigzag
     pairs apart at >= 3 columns and overflow at 390px. */
  .engine-stats {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.25rem 1rem;
    margin: 0.5rem 0;
    font-size: 0.875rem;
  }
  .engine-stats dt {
    color: var(--muted);
  }
  .engine-stats dd {
    margin: 0;
    min-width: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    overflow-wrap: anywhere;
  }
  .engine-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }
  .engine-actions button {
    padding: 0.2rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font-size: 0.8125rem;
    cursor: pointer;
  }
  .engine-actions form {
    display: flex;
    gap: 0.4rem;
    margin-left: auto;
    min-width: 0;
    flex: 1 1 12rem;
    max-width: 20rem;
  }
  .engine-actions input[type="search"] {
    flex: 1 1 auto;
    min-width: 0;
    padding: 0.2rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font-size: 0.8125rem;
  }
  .engine-notice {
    margin: 0.4rem 0 0;
  }
  .test-results {
    margin-top: 0.5rem;
  }
  .test-meta {
    font-size: 0.8125rem;
  }
</style>
