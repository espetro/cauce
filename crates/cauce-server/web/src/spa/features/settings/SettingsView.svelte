<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/settings` — one-for-one with `templates/settings.html` +
  `settings_cache.html`: search / engines / admission / logging /
  cache / AI fieldsets posting the same dotted field names to
  `PUT /api/config`. The admin form is a formisch `Form` (per-field
  number validation) on vendored `ui/` Bits UI controls (Button,
  Checkbox, Select). Wire degradations (documented on #266): no env provenance so
  nothing is `disabled`, no `engines_pinned` greying, no models
  datalist, no `config_path` in the hint.
-->
<script lang="ts">
  import { Field, Form } from "@formisch/svelte";

  import { spa } from "../../lib/i18n.js";
  import { capabilities } from "../../lib/capabilities.svelte.js";
  import { byok, setByok } from "../../lib/byok.svelte.js";
  import type { ByokCreds } from "../../lib/byok.svelte.js";
  import UiButton from "../../ui/button.svelte";
  import UiCheckbox from "../../ui/checkbox.svelte";
  import UiSelect from "../../ui/select.svelte";
  import type { createSettingsPage } from "./settings.svelte.js";

  interface SettingsViewProps {
    page: ReturnType<typeof createSettingsPage>;
  }

  let { page }: SettingsViewProps = $props();
  const s = spa.settings;
  const st = $derived(page.state);
  const form = $derived(page.form);

  // PUB-03: the browser-local BYOK section shows whenever the instance
  // honours overrides; the config form below stays admin-only.
  const byokOn = $derived(
    capabilities.flags.allowUserKeys || capabilities.flags.allowUserBaseUrl,
  );
  const isAdmin = $derived(capabilities.role === "admin");
  let draft = $state<ByokCreds>({ ...byok });
  let byokSaved = $state(false);

  const tierOptions = [
    { value: "", label: s.tier_default },
    { value: "1", label: "1" },
    { value: "2", label: "2" },
    { value: "3", label: "3" },
  ];
  const protocolOptions = [
    { value: "", label: s.byok_protocol_default },
    { value: "openai", label: "openai" },
    { value: "anthropic", label: "anthropic" },
  ];

  function saveByok(): void {
    setByok(draft);
    byokSaved = true;
  }
</script>

<h1>{s.page_title}</h1>

{#if byokOn}
  <form
    onsubmit={(e) => {
      e.preventDefault();
      saveByok();
    }}
  >
    <fieldset>
      <legend>{s.section_byok}</legend>
      <p class="hint">{s.byok_hint}</p>
      <div class="frow">
        <label>
          {s.ai_api_key}
          <input
            type="password"
            autocomplete="off"
            disabled={!capabilities.flags.allowUserKeys}
            bind:value={draft.api_key}
          />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.ai_base_url}
          <input
            type="text"
            disabled={!capabilities.flags.allowUserBaseUrl}
            placeholder={capabilities.flags.allowUserBaseUrl
              ? s.ai_base_url
              : s.byok_base_url_locked}
            bind:value={draft.base_url}
          />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.ai_model}
          <input
            type="text"
            disabled={!capabilities.flags.allowUserKeys}
            placeholder={s.ai_model_placeholder}
            bind:value={draft.model}
          />
        </label>
      </div>
      <div class="frow">
        <label>
          {s.byok_protocol}
          <UiSelect
            value={draft.protocol}
            onValueChange={(v) => (draft.protocol = v)}
            options={protocolOptions}
            disabled={!capabilities.flags.allowUserKeys}
          />
        </label>
      </div>
      <UiButton type="submit">{s.save}</UiButton>
      {#if byokSaved}<span class="form-status">{s.saved}</span>{/if}
    </fieldset>
  </form>
{/if}

{#if isAdmin}
<p class="hint">{s.restart_note}</p>

{#if st.loading}
  <p class="stats">…</p>
{:else if st.error !== ""}
  <p class="field-error" role="alert">{st.error}</p>
{:else if st.form != null}
  {@const f = st.form}
  <Form of={form} class="settings-form" onsubmit={(o) => page.save(o)}>
    <fieldset>
      <legend>{s.section_search}</legend>
      <Field of={form} path={["deadlineMs"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.deadline}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["ttlS"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.ttl}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["minResults"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.min_results}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["hedgeFloorMs"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.hedge_floor}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["hedgeCeilingMs"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.hedge_ceiling}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
    </fieldset>

    <fieldset>
      <legend>{s.section_engines}</legend>
      {#each f.engines as e, i (e.id)}
        <div class="engine-row">
          <strong class="engine-id">{e.id}</strong>
          <span class="hint mono">{e.kind}</span>
          <Field of={form} path={["engines", i, "enabled"]}>
            {#snippet children(field)}
              <label>
                <UiCheckbox
                  checked={field.input === true}
                  onCheckedChange={field.onInput}
                  name={field.props.name}
                />
                {s.enabled}
              </label>
            {/snippet}
          </Field>
          <Field of={form} path={["engines", i, "tier"]}>
            {#snippet children(field)}
              <label>
                {s.tier}
                <UiSelect
                  value={field.input ?? ""}
                  onValueChange={field.onInput}
                  options={tierOptions}
                  name={field.props.name}
                />
              </label>
            {/snippet}
          </Field>
          <Field of={form} path={["engines", i, "proxy"]}>
            {#snippet children(field)}
              <label class="proxy">
                {s.proxy}
                <input
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  placeholder={s.proxy_placeholder}
                />
              </label>
            {/snippet}
          </Field>
        </div>
      {/each}
      <a href="/app/admin?tab=engines">{s.browse_engines}</a>
    </fieldset>

    <fieldset>
      <legend>{s.section_admission}</legend>
      <Field of={form} path={["maxWaitMs"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.max_wait}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["maxConcurrent"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.max_concurrent}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
    </fieldset>

    <fieldset>
      <legend>{s.section_logging}</legend>
      <Field of={form} path={["retentionDays"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.retention}
              <span class="hint">{s.applies_after_restart}</span>
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
      <p class="hint">
        <a href="/api/report">{s.report_link}</a> — {s.report_hint}
      </p>
    </fieldset>

    <fieldset id="cache-block">
      <legend>{s.section_cache}</legend>
      {#if st.cacheLine !== ""}<p class="hint">{st.cacheLine}</p>{/if}
      <p class="cache-actions">
        <UiButton size="sm" onclick={() => page.bulkDelete("expired")}
          >{s.delete_expired}</UiButton
        >
        <UiButton
          size="sm"
          variant="danger"
          onclick={() => page.bulkDelete("all")}>{s.delete_all}</UiButton
        >
        <a href="/app/admin?tab=cache">{s.browse_entries}</a>
      </p>
      {#if st.cacheError !== ""}
        <p class="field-error">{st.cacheError}</p>
      {/if}
    </fieldset>

    <fieldset class="greyed">
      <legend>{s.section_ai}</legend>
      <Field of={form} path={["aiBaseUrl"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.ai_base_url}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
              />
            </label>
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["aiApiKey"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.ai_api_key}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                placeholder={s.ai_api_key_placeholder}
              />
            </label>
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["aiModel"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.ai_model}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                placeholder={s.ai_model_placeholder}
              />
            </label>
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["aiMaxTurns"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.ai_max_turns}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["aiMaxSearches"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.ai_max_searches}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["aiProviderBudgetS"]}>
        {#snippet children(field)}
          <div class="frow">
            <label>
              {s.ai_provider_budget}
              <input
                type="text"
                {...field.props}
                value={field.input ?? ""}
                aria-invalid={field.errors != null}
              />
            </label>
            {#if field.errors}
              <span class="field-error">{field.errors[0]}</span>
            {/if}
          </div>
        {/snippet}
      </Field>
      <Field of={form} path={["aiEnabled"]}>
        {#snippet children(field)}
          <label>
            <UiCheckbox
              checked={field.input === true}
              onCheckedChange={field.onInput}
              name={field.props.name}
            />
            {s.enabled}
          </label>
        {/snippet}
      </Field>
    </fieldset>

    <UiButton type="submit" disabled={form.isSubmitting}>{s.save}</UiButton>
    <span
      id="settings-status"
      role="status"
      aria-live="polite"
      class="form-status"
      class:error={st.statusError}>{st.status}</span
    >
  </Form>
{/if}
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
  .form-status {
    margin-left: 0.75rem;
    font-size: 0.8125rem;
    color: var(--muted);
  }
  .form-status.error {
    color: var(--warn);
  }
</style>
