<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/settings` — one-for-one with `templates/settings.html` +
  `settings_cache.html`: search / engines / admission / logging /
  cache / AI fieldsets posting the same dotted field names to
  `PUT /api/config`. Wire degradations (documented on #266): no env
  provenance so nothing is `disabled`, no `engines_pinned` greying, no
  models datalist, no `config_path` in the hint.
-->
<script lang="ts">
  import { spa } from "../../lib/i18n.js";
  import type { createSettingsPage } from "./settings.svelte.js";

  interface SettingsViewProps {
    page: ReturnType<typeof createSettingsPage>;
  }

  let { page }: SettingsViewProps = $props();
  const s = spa.settings;
  const st = $derived(page.state);
</script>

<h1>{s.page_title}</h1>
<p class="hint">{s.restart_note}</p>

{#if st.loading}
  <p class="stats">…</p>
{:else if st.error !== ""}
  <p class="field-error" role="alert">{st.error}</p>
{:else if st.form != null}
  {@const f = st.form}
  <form
    class="settings-form"
    onsubmit={(e) => {
      e.preventDefault();
      page.save();
    }}
  >
    <fieldset>
      <legend>{s.section_search}</legend>
      <div class="frow">
        <label>
          {s.deadline}
          <input type="text" bind:value={f.deadlineMs} />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.ttl}
          <input type="text" bind:value={f.ttlS} />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.min_results}
          <input type="text" bind:value={f.minResults} />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.hedge_floor}
          <input type="text" bind:value={f.hedgeFloorMs} />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.hedge_ceiling}
          <input type="text" bind:value={f.hedgeCeilingMs} />
        </label>
      </div>
    </fieldset>

    <fieldset>
      <legend>{s.section_engines}</legend>
      {#each f.engines as e}
        <div class="engine-row">
          <strong class="engine-id">{e.id}</strong>
          <span class="hint mono">{e.kind}</span>
          <label>
            <input type="checkbox" bind:checked={e.enabled} />
            {s.enabled}
          </label>
          <label>
            {s.tier}
            <select bind:value={e.tier}>
              <option value="">{s.tier_default}</option>
              <option value="1">1</option>
              <option value="2">2</option>
              <option value="3">3</option>
            </select>
          </label>
          <label class="proxy">
            {s.proxy}
            <input
              type="text"
              bind:value={e.proxy}
              placeholder={s.proxy_placeholder}
            />
          </label>
        </div>
      {/each}
      <a href="/app/admin?tab=engines">{s.browse_engines}</a>
    </fieldset>

    <fieldset>
      <legend>{s.section_admission}</legend>
      <div class="frow">
        <label>
          {s.max_wait}
          <input type="text" bind:value={f.maxWaitMs} />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.max_concurrent}
          <input type="text" bind:value={f.maxConcurrent} />
        </label>
      </div>
    </fieldset>

    <fieldset>
      <legend>{s.section_logging}</legend>
      <div class="frow">
        <label>
          {s.retention}
          <span class="hint">{s.applies_after_restart}</span>
          <input type="text" bind:value={f.retentionDays} />
        </label>
      </div>
      <p class="hint">
        <a href="/api/report">{s.report_link}</a> — {s.report_hint}
      </p>
    </fieldset>

    <fieldset id="cache-block">
      <legend>{s.section_cache}</legend>
      {#if st.cacheLine !== ""}<p class="hint">{st.cacheLine}</p>{/if}
      <p class="cache-actions">
        <button type="button" onclick={() => page.bulkDelete("expired")}
          >{s.delete_expired}</button
        >
        <button type="button" onclick={() => page.bulkDelete("all")}
          >{s.delete_all}</button
        >
        <a href="/app/admin?tab=cache">{s.browse_entries}</a>
      </p>
      {#if st.cacheError !== ""}
        <p class="field-error">{st.cacheError}</p>
      {/if}
    </fieldset>

    <fieldset class="greyed">
      <legend>{s.section_ai}</legend>
      <div class="frow">
        <label>
          {s.ai_base_url}
          <input type="text" bind:value={f.aiBaseUrl} />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.ai_api_key}
          <input
            type="text"
            bind:value={f.aiApiKey}
            placeholder={s.ai_api_key_placeholder}
          />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.ai_model}
          <input
            type="text"
            bind:value={f.aiModel}
            placeholder={s.ai_model_placeholder}
          />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.ai_max_turns}
          <input type="text" bind:value={f.aiMaxTurns} />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.ai_max_searches}
          <input type="text" bind:value={f.aiMaxSearches} />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.ai_provider_budget}
          <input type="text" bind:value={f.aiProviderBudgetS} />
        </label>
      </div>
      <label>
        <input type="checkbox" bind:checked={f.aiEnabled} />
        {s.enabled}
      </label>
    </fieldset>

    <button type="submit" disabled={st.saving}>{s.save}</button>
    <span
      id="settings-status"
      role="status"
      aria-live="polite"
      class="form-status"
      class:error={st.statusError}>{st.status}</span
    >
  </form>
{/if}

<style>
  .engine-row {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.5rem 1rem;
    padding: 0.35rem 0;
    border-bottom: 1px solid var(--border);
  }
  .engine-row:last-of-type {
    border-bottom: 0;
  }
  .engine-row label {
    display: inline-flex;
    align-items: baseline;
    gap: 0.35rem;
    font-size: 0.8125rem;
  }
  .engine-row .proxy input {
    width: 11rem;
    min-width: 0;
  }
  .engine-id {
    min-width: 8rem;
  }
  .mono {
    font-family: var(--mono);
    font-size: 0.8125rem;
  }
  .cache-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
  }
  .cache-actions button {
    padding: 0.2rem 0.6rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font-size: 0.8125rem;
    cursor: pointer;
  }
  .form-status {
    margin-left: 0.75rem;
    font-size: 0.8125rem;
    color: var(--muted);
  }
  .form-status.error {
    color: var(--warn);
  }
</style>
