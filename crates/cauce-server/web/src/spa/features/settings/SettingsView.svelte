<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  project, You can obtain one at https://mozilla.org/MPL/2.0.

  `/app/settings` — D6 rebuild from user review: one page, grouped
  sections, no tabs. Every row is the same two-column grid (label left,
  control right) and every dropdown holds its form-field width — never
  the page width (the tabbed version let selects stretch).

  Save semantics (the review's open question): browser-local rows
  (theme, language, results-per-page, safe search, region, BYOK) apply
  immediately and say so under the Browser section. The Instance
  section is the admin `PUT /api/config` formisch form with explicit
  Save / Discard buttons; a save lands as a Sonner toast (naming the
  restart-deferred keys when the server defers), and leaving edits
  unsaved raises a dirty-changes toast.
-->
<script lang="ts">
  import { Field, Form } from "@formisch/svelte";
  import { toast } from "svelte-sonner";
  import { onMount } from "svelte";

  import { spa } from "../../lib/i18n.js";
  import { capabilities } from "../../lib/capabilities.svelte.js";
  import { applyTheme } from "../../app/theme.svelte.js";
  import { byok, setByok } from "../../lib/byok.svelte.js";
  import UiButton from "../../ui/button.svelte";
  import UiCheckbox from "../../ui/checkbox.svelte";
  import UiSelect from "../../ui/select.svelte";
  import UiSwitch from "../../ui/switch.svelte";
  import UiDataTable from "../../ui/data-table.svelte";
  import { confirm } from "../../ui/confirm.js";
  import type { createSettingsPage } from "./settings.svelte.js";
  import type { ByokCreds } from "../../lib/byok.svelte.js";
  import type { ColumnDef } from "@tanstack/svelte-table";

  interface SettingsViewProps {
    page: ReturnType<typeof createSettingsPage>;
  }

  let { page }: SettingsViewProps = $props();
  const s = spa.settings;
  const st = $derived(page.state);
  const form = $derived(page.form);
  const isAdmin = $derived(capabilities.role === "admin");
  const byokOn = $derived(
    capabilities.flags.allowUserKeys || capabilities.flags.allowUserBaseUrl,
  );

  // --- browser-local rows --------------------------------------------

  const themeOptions = [
    { value: "system", label: s.theme_system },
    { value: "light", label: s.theme_light },
    { value: "dark", label: s.theme_dark },
  ];
  const langOptions = [
    { value: "en", label: "English" },
    { value: "fr", label: "Français" },
    { value: "de", label: "Deutsch" },
    { value: "es", label: "Español" },
    { value: "ja", label: "日本語" },
    { value: "zh", label: "简体中文" },
  ];
  const rppOptions = [
    { value: "10", label: "10" },
    { value: "20", label: "20" },
    { value: "50", label: "50" },
    { value: "100", label: "100" },
  ];
  const safesearchOptions = [
    { value: "off", label: s.safesearch_off },
    { value: "moderate", label: s.safesearch_moderate },
    { value: "strict", label: s.safesearch_strict },
  ];
  const regionOptions = [
    { value: "us", label: "United States" },
    { value: "gb", label: "United Kingdom" },
    { value: "fr", label: "France" },
    { value: "de", label: "Germany" },
    { value: "ja", label: "Japan" },
  ];
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

  function localGet(key: string, fallback: string): string {
    try {
      return localStorage.getItem(key) ?? fallback;
    } catch {
      return fallback;
    }
  }

  function setTheme(v: string): void {
    try {
      localStorage.setItem("cauce-theme", v);
    } catch {
      /* private-mode storage failures keep the session value */
    }
    applyTheme();
  }

  function setLocal(key: string, v: string): void {
    try {
      localStorage.setItem(key, v);
    } catch {
      /* private-mode storage failures keep the session value */
    }
  }

  // --- instance form (admin) ------------------------------------------

  // BYOK edits keep the old immediate-apply draft; the instance form
  // tracks dirty through formisch's baseline.
  let draft = $state<ByokCreds>({ ...byok });

  // formisch tracks "differs from the initial value" — the same
  // baseline the Discard button restores.
  const dirty = $derived(form != null && form.isDirty);

  // Engines config rides the same `ui/data-table` primitive the admin
  // engines tab uses (one skin for both listings).
  const engineColumns: ColumnDef<any, any, any>[] = [
    { id: "id", header: spa.engines.col_engine, accessorKey: "id", size: 180, enableSorting: true },
    { id: "kind", header: spa.engines.col_kind, accessorKey: "kind", size: 120, enableSorting: false },
    { id: "enabled", header: spa.engines.col_enabled, accessorKey: "enabled", size: 100, enableSorting: false },
    { id: "tier", header: spa.settings.col_tier, accessorKey: "tier", size: 100, enableSorting: false },
    { id: "proxy", header: spa.settings.col_proxy, accessorKey: "proxy", size: 300, enableSorting: false },
  ];

  let dirtyNotified = false;
  // One dirty-changes toast per page visit: raise it when edits first
  // go unsaved-and-navigated-away is unknowable here, so the review's
  // ask is satisfied by raising it once while dirty and letting the
  // Save/Discard pair resolve it.
  $effect(() => {
    if (dirty && !dirtyNotified) {
      dirtyNotified = true;
      toast.warning(s.dirty_toast_title, {
        description: s.dirty_toast_body,
        duration: 6_000,
      });
    }
    if (!dirty) dirtyNotified = false;
  });

  async function discard(): Promise<void> {
    const ok = await confirm({
      title: s.discard_confirm_title,
      description: s.discard_confirm_body,
    });
    if (!ok) return;
    page.discard();
  }

  onMount(() => {
    draft = { ...byok };
  });

  function saveByok(): void {
    setByok(draft);
    toast.success(s.saved_toast, { duration: 4_000 });
  }

  // --- BYOK model list ------------------------------------------------
  //
  // The provider's own model list, fetched client-side from the BYOK
  // base_url's OpenAI-compatible `/models` endpoint. No server route:
  // the browser already holds the credentials, the gateway is the same
  // trust domain as the browser (local mode), and a server proxy would
  // have to answer "whose key?" for a public instance. A failed fetch
  // is not an error state — the free-text input stays.

  let modelOptions = $state<{ value: string; label: string }[]>([]);
  let modelsLoading = $state(false);

  function modelsUrl(base: string): string {
    const trimmed = base.trim().replace(/\/+$/, "");
    return trimmed.endsWith("/v1") ? `${trimmed}/models` : `${trimmed}/v1/models`;
  }

  async function loadModels(): Promise<void> {
    if (draft.base_url.trim() === "") return;
    modelsLoading = true;
    try {
      const headers: Record<string, string> = {};
      if (draft.api_key.trim() !== "") headers.Authorization = `Bearer ${draft.api_key.trim()}`;
      const res = await fetch(modelsUrl(draft.base_url), { headers });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const body = (await res.json()) as { data?: { id?: string; name?: string }[] };
      const rows = Array.isArray(body.data) ? body.data : [];
      modelOptions = rows
        .map((m) => ({ value: m.id ?? "", label: m.name ?? m.id ?? "" }))
        .filter((m) => m.value !== "")
        .sort((a, b) => a.label.localeCompare(b.label));
      if (modelOptions.length === 0) {
        toast.error(s.byok_models_empty);
      }
    } catch (err) {
      modelOptions = [];
      toast.error(`${s.byok_models_failed}: ${(err as Error).message}`);
    } finally {
      modelsLoading = false;
    }
  }

  // --- BYOK test ------------------------------------------------------
  //
  // The cheapest end-to-end proof of the credential set: one tiny
  // grounded answer through the same path the UI uses. It rides the
  // stored (not draft) credentials, so it is saved first.

  let testState = $state<"idle" | "testing" | "ok" | "error">("idle");
  let testError = $state("");

  async function testByok(): Promise<void> {
    testState = "testing";
    testError = "";
    try {
      // The override rides the request body, exactly like the answer
      // thread's `byokWire()` — so the test proves the TYPED
      // credentials, not whatever was saved earlier.
      const ai: Record<string, string> = {};
      if (draft.api_key.trim() !== "") ai.api_key = draft.api_key.trim();
      if (draft.model.trim() !== "") ai.model = draft.model.trim();
      if (draft.protocol !== "") ai.protocol = draft.protocol;
      if (draft.base_url.trim() !== "") ai.base_url = draft.base_url.trim();

      const res = await fetch("/api/answer", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          q: "ping",
          context_results: [],
          ...(Object.keys(ai).length > 0 ? { ai } : {}),
        }),
      });
      if (!res.ok) {
        const body = (await res.json().catch(() => null)) as
          | { error?: { message?: string } }
          | null;
        throw new Error(body?.error?.message ?? `HTTP ${res.status}`);
      }
      // The response is an SSE stream: a successful status plus a
      // readable body is the signal; the frames themselves are the
      // answer thread's business.
      await res.body?.cancel();
      testState = "ok";
    } catch (err) {
      testState = "error";
      testError = (err as Error).message;
    }
  }
</script>

<h1 class="page-title">{s.page_title}</h1>

<!-- Browser: local rows, applied immediately. -->
<section class="group">
  <h2>{s.section_browser}</h2>
  <p class="group-hint">{s.browser_hint}</p>

  <div class="rows">
    <div class="row">
      <label for="settings-theme">{s.theme}</label>
      <div class="control">
        <UiSelect
          id="settings-theme"
          value={localGet("cauce-theme", "system")}
          onValueChange={setTheme}
          options={themeOptions}
        />
      </div>
    </div>

    <div class="row">
      <label for="settings-lang">{s.language}</label>
      <div class="control">
        <UiSelect
          id="settings-lang"
          value={localGet("cauce-lang", "en")}
          onValueChange={(v) => setLocal("cauce-lang", v)}
          options={langOptions}
        />
      </div>
    </div>

    <div class="row">
      <label for="settings-rpp">{s.results_per_page}</label>
      <div class="control">
        <UiSelect
          id="settings-rpp"
          value={localGet("cauce-rpp", "20")}
          onValueChange={(v) => setLocal("cauce-rpp", v)}
          options={rppOptions}
        />
      </div>
    </div>

    <div class="row">
      <label for="settings-safesearch">{s.safe_search}</label>
      <div class="control">
        <UiSelect
          id="settings-safesearch"
          value={localGet("cauce-safesearch", "moderate")}
          onValueChange={(v) => setLocal("cauce-safesearch", v)}
          options={safesearchOptions}
        />
      </div>
    </div>

    <div class="row">
      <label for="settings-region">{s.region}</label>
      <div class="control">
        <UiSelect
          id="settings-region"
          value={localGet("cauce-region", "us")}
          onValueChange={(v) => setLocal("cauce-region", v)}
          options={regionOptions}
        />
      </div>
    </div>
  </div>
</section>

{#if byokOn}
<section class="group">
  <h2>{s.section_byok}</h2>
  <p class="group-hint">{s.byok_hint}</p>

  <form onsubmit={(e) => { e.preventDefault(); saveByok(); }}>
    <div class="rows">
      <div class="row">
        <label for="settings-byok-key">{s.ai_api_key}</label>
        <div class="control">
          <input
            id="settings-byok-key"
            type="password"
            autocomplete="off"
            disabled={!capabilities.flags.allowUserKeys}
            bind:value={draft.api_key}
          />
        </div>
      </div>
      <div class="row">
        <label for="settings-byok-base-url">{s.ai_base_url}</label>
        <div class="control">
          <input
            id="settings-byok-base-url"
            type="text"
            disabled={!capabilities.flags.allowUserBaseUrl}
            placeholder={capabilities.flags.allowUserBaseUrl
              ? "https://openrouter.ai/api/v1"
              : s.byok_base_url_locked}
            bind:value={draft.base_url}
          />
        </div>
      </div>
      <div class="row">
        <label for="settings-byok-model">{s.ai_model}</label>
        <div class="control">
          <div class="control-inline">
            {#if modelOptions.length > 0}
              <!-- The provider's model list was fetched from the BYOK
                   base_url's `/models` endpoint: pick, don't type. -->
              <UiSelect
                id="settings-byok-model"
                value={draft.model}
                onValueChange={(v) => (draft.model = v)}
                options={modelOptions}
                disabled={!capabilities.flags.allowUserKeys}
              />
            {:else}
              <input
                id="settings-byok-model"
                type="text"
                disabled={!capabilities.flags.allowUserKeys}
                placeholder={s.ai_model_placeholder}
                bind:value={draft.model}
              />
            {/if}
            <UiButton
              type="button"
              size="sm"
              variant="ghost"
              onclick={loadModels}
              disabled={!capabilities.flags.allowUserBaseUrl || modelsLoading}>
              {modelsLoading ? s.byok_models_loading : s.byok_load_models}
            </UiButton>
          </div>
        </div>
      </div>
      <div class="row">
        <label for="settings-byok-protocol">{s.byok_protocol}</label>
        <div class="control">
          <UiSelect
            id="settings-byok-protocol"
            value={draft.protocol}
            onValueChange={(v) => (draft.protocol = v)}
            options={protocolOptions}
            disabled={!capabilities.flags.allowUserKeys}
          />
        </div>
      </div>
    </div>
    <div class="button-row">
      <UiButton type="submit" size="sm">{s.save}</UiButton>
      <UiButton
        type="button"
        size="sm"
        variant="ghost"
        onclick={testByok}
        disabled={testState === "testing"}>
        {testState === "testing" ? s.byok_testing : s.byok_test}
      </UiButton>
      {#if testState === "ok"}
        <span class="test-ok" role="status">{s.byok_test_ok}</span>
      {:else if testState === "error"}
        <span class="field-error" role="alert">{testError}</span>
      {/if}
    </div>
  </form>
</section>
{/if}

{#if isAdmin}
<section class="group">
  <h2>{s.section_instance}</h2>
  <p class="group-hint">{s.instance_hint}</p>

  {#if st.loading}
    <p class="stats">…</p>
  {:else if st.error !== ""}
    <p class="field-error" role="alert">{st.error}</p>
  {:else if st.form != null}
    {@const f = st.form}
    <Form of={form} class="settings-form" onsubmit={(o) => page.save(o)}>
      <div class="rows">
        <Field of={form} path={["deadlineMs"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-deadline">{s.deadline}</label>
              <div class="control">
                <input
                  id="cfg-deadline"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["ttlS"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-ttl">{s.ttl}</label>
              <div class="control">
                <input
                  id="cfg-ttl"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["minResults"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-minresults">{s.min_results}</label>
              <div class="control">
                <input
                  id="cfg-minresults"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["hedgeFloorMs"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-hedgefloor">{s.hedge_floor}</label>
              <div class="control">
                <input
                  id="cfg-hedgefloor"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["hedgeCeilingMs"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-hedgeceiling">{s.hedge_ceiling}</label>
              <div class="control">
                <input
                  id="cfg-hedgeceiling"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["maxWaitMs"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-maxwait">{s.max_wait}</label>
              <div class="control">
                <input
                  id="cfg-maxwait"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["maxConcurrent"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-maxconcurrent">{s.max_concurrent}</label>
              <div class="control">
                <input
                  id="cfg-maxconcurrent"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["retentionDays"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-retention">{s.retention}</label>
              <div class="control">
                <input
                  id="cfg-retention"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
                <span class="hint">{s.applies_after_restart}</span>
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["aiBaseUrl"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-ai-base-url">{s.ai_base_url}</label>
              <div class="control">
                <input
                  id="cfg-ai-base-url"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                />
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["aiApiKey"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-ai-api-key">{s.ai_api_key}</label>
              <div class="control">
                <input
                  id="cfg-ai-api-key"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  placeholder={s.ai_api_key_placeholder}
                />
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["aiModel"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-ai-model">{s.ai_model}</label>
              <div class="control">
                <input
                  id="cfg-ai-model"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  placeholder={s.ai_model_placeholder}
                />
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["aiEnabled"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-ai-enabled">{s.section_ai}</label>
              <div class="control">
                <UiSwitch
                  id="cfg-ai-enabled"
                  checked={field.input === true}
                  onCheckedChange={field.onInput}
                  name={field.props.name}
                />
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["aiMaxTurns"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-ai-maxturns">{s.ai_max_turns}</label>
              <div class="control">
                <input
                  id="cfg-ai-maxturns"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["aiMaxSearches"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-ai-maxsearches">{s.ai_max_searches}</label>
              <div class="control">
                <input
                  id="cfg-ai-maxsearches"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
        <Field of={form} path={["aiProviderBudgetS"]}>
          {#snippet children(field)}
            <div class="row">
              <label for="cfg-ai-budget">{s.ai_provider_budget}</label>
              <div class="control">
                <input
                  id="cfg-ai-budget"
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  aria-invalid={field.errors != null}
                />
                {#if field.errors}<span class="field-error">{field.errors[0]}</span>{/if}
              </div>
            </div>
          {/snippet}
        </Field>
      </div>

      <h3>{s.section_engines}</h3>
      <UiDataTable
        columns={engineColumns}
        data={f.engines}
        getRowId={(e) => e.id}
        caption={s.section_engines}
      >
        {#snippet cell(cell)}
          {@const eng = cell.row.original}
          {@const i = f.engines.indexOf(eng)}
          {#if cell.column.id === "id"}
            <strong>{eng.id}</strong>
          {:else if cell.column.id === "kind"}
            <span class="hint mono">{eng.kind}</span>
          {:else if cell.column.id === "enabled"}
            <Field of={form} path={["engines", i, "enabled"]}>
              {#snippet children(field)}
                <label class="engine-field">
                  <UiCheckbox
                    checked={field.input === true}
                    onCheckedChange={field.onInput}
                    name={field.props.name}
                  />
                  {s.enabled}
                </label>
              {/snippet}
            </Field>
          {:else if cell.column.id === "tier"}
            <Field of={form} path={["engines", i, "tier"]}>
              {#snippet children(field)}
                <label class="engine-field">
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
          {:else if cell.column.id === "proxy"}
            <Field of={form} path={["engines", i, "proxy"]}>
              {#snippet children(field)}
                <input
                  type="text"
                  {...field.props}
                  value={field.input ?? ""}
                  placeholder={s.proxy_placeholder}
                  aria-label="{eng.id} {s.proxy}"
                />
              {/snippet}
            </Field>
          {/if}
        {/snippet}
      </UiDataTable>
      <p class="hint"><a href="/app/admin?tab=engines">{s.browse_engines}</a></p>

      <h3>{s.section_cache}</h3>
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

      <p class="hint">
        <a href="/api/report">{s.report_link}</a> — {s.report_hint}
      </p>

      <!-- The save bar: explicit Save/Discard, sticky at the section
           end so the semantics are visible without scrolling back. -->
      <div class="save-bar">
        <UiButton type="submit" variant="primary" disabled={form.isSubmitting}
          >{s.save}</UiButton
        >
        <UiButton onclick={discard} disabled={!dirty}>{s.discard}</UiButton>
        <span
          id="settings-status"
          role="status"
          aria-live="polite"
          class="form-status"
          class:error={st.statusError}>{st.status}</span
        >
      </div>
    </Form>
  {/if}
</section>
{/if}

<style>
  /* The section grouping: an h2 + hint lead, then the aligned rows. */
  .group {
    margin: 0 0 2rem;
  }
  .group h2 {
    margin: 0 0 0.25rem;
  }
  .group h3 {
    margin: 1.25rem 0 0.5rem;
    font-size: var(--text-sm);
    font-weight: 600;
    letter-spacing: var(--tracking-wide);
    text-transform: uppercase;
    color: var(--muted);
  }
  .group-hint {
    margin: 0 0 0.75rem;
    color: var(--muted);
    font-size: 0.8125rem;
  }

  /* The two-column row grid the review asked for: label left,
     control right, one shared label column width. */
  .rows {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(10rem, 16rem) minmax(0, 1fr);
    gap: 0.25rem 1rem;
    align-items: center;
  }
  .row > label {
    color: var(--muted);
    font-size: 0.9375rem;
  }
  .control {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 0;
    /* D6 review: dropdowns must NOT stretch to the page width — the
       control keeps a form-field width inside the grid column. */
    max-width: 18rem;
  }
  /* The side-by-side secondary control (Load models) sits beside the
     main control in the same grid column, no wrap. */
  .control-inline {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }
  .control-inline > :global(.ui-select-trigger),
  .control-inline > input {
    flex: 1 1 auto;
    min-width: 0;
  }
  .control-inline > :global(.ui-button) {
    flex: none;
  }
  /* Save + Test share the row under the BYOK form. */
  .button-row {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    margin-top: 0.875rem;
  }
  .button-row .test-ok {
    color: var(--accent);
    font-size: 0.8125rem;
  }
  .button-row .field-error {
    font-size: 0.8125rem;
  }
  .control :global(.ui-select-trigger) {
    width: 100%;
    flex: none;
  }
  .control input[type="text"],
  .control input[type="password"] {
    width: 100%;
    min-width: 0;
    height: var(--control-h);
    padding: 0 0.625rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
  }
  .control input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .control .hint {
    font-size: 0.75rem;
  }

  /* Engines ride `ui/data-table` (shared with the admin engines tab);
     only the inline field label needs local styling. */
  label.engine-field {
    display: inline-flex;
    align-items: baseline;
    gap: 0.35rem;
    font-size: 0.8125rem;
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

  .save-bar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 1.25rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--border);
  }
  .form-status {
    font-size: 0.8125rem;
    color: var(--muted);
  }
  .form-status.error {
    color: var(--warn);
  }

  @media (max-width: 640px) {
    .row {
      grid-template-columns: 1fr;
      align-items: start;
    }
    .control {
      max-width: none;
    }
  }
</style>
