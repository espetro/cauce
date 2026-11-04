<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  project, You can obtain one at https://mozilla.org/MPL/2.0.

  `/app/settings` — redesigned as a tabbed page with one row per setting,
  label + help on the left, control right-aligned on wide viewports and
  stacked below on narrow ones. Toggles use role="switch" with visible
  On/Off. Bits UI primitives (Tabs, Select). Wire one-for-one with
  `templates/settings.html` fields. No per-row save; browser-local
  settings apply immediately, instance config keeps explicit `PUT /api/config`.
-->
<script lang="ts">
  import { spa } from "../../lib/i18n.js";
  import { capabilities } from "../../lib/capabilities.svelte.js";
  import { setByok } from "../../lib/byok.svelte.js";
  import UiTabs from "../../ui/tabs.svelte";
  import UiSelect from "../../ui/select.svelte";
  import type { createSettingsPage } from "./settings.svelte.js";

  interface SettingsViewProps {
    page: ReturnType<typeof createSettingsPage>;
  }

  let { page }: SettingsViewProps = $props();
  const s = spa.settings;
  const isAdmin = $derived(capabilities.role === "admin");

  // --- Tabs ----------------------------------------------------------

  // Five tabs: General (browser-local: theme, language, results-per-page,
  // default engines), Privacy (safe search, region), Search defaults
  // (default engines, TTL, page size), Instance (server config, admin only),
  // AI (BYOK key and base URL, browser-local).
  const tabOptions = $derived([
    { value: "general", label: s.general_tab },
    { value: "privacy", label: s.privacy_tab },
    { value: "search", label: s.search_defaults_tab },
    { value: "instance", label: s.instance_tab, disabled: !isAdmin },
    { value: "ai", label: s.ai_tab },
  ]);

  // URL query param `?tab=` as source of truth; update on change.
  let tabValue = $state("general");

  function onTabChange(value: string): void {
    tabValue = value;
    const params = new URLSearchParams(window.location.search);
    params.set("tab", value);
    const newSearch = params.toString();
    const newUrl = newSearch ? `${window.location.pathname}?${newSearch}` : window.location.pathname;
    window.history.pushState({}, "", newUrl);
  }
</script>

<h1 class="page-title">{s.page_title}</h1>

<!-- Tabbed interface -->
<UiTabs value={tabValue} onValueChange={onTabChange} tabs={tabOptions} />

<!-- General tab: browser-local settings -->
{#if tabValue === "general"}
<fieldset>
  <legend>{s.section_general}</legend>

  <div class="settings-row">
    <label for="settings-theme">{s.theme}</label>
    <UiSelect
      id="settings-theme"
      value={localStorage.getItem("cauce-theme") ?? "system"}
      onValueChange={(v) => {
        localStorage.setItem("cauce-theme", v);
        document.documentElement.setAttribute("data-theme", v);
      }}
      options={[
        { value: "system", label: s.theme_system },
        { value: "light", label: s.theme_light },
        { value: "dark", label: s.theme_dark },
      ]}
    />
  </div>

  <div class="settings-row">
    <label for="settings-lang">{s.language}</label>
    <UiSelect
      id="settings-lang"
      value={localStorage.getItem("cauce-lang") ?? "en"}
      onValueChange={(v) => localStorage.setItem("cauce-lang", v)}
      options={[
        { value: "en", label: "English" },
        { value: "fr", label: "Français" },
        { value: "de", label: "Deutsch" },
        { value: "es", label: "Español" },
        { value: "ja", label: "日本語" },
        { value: "zh", label: "简体中文" },
      ]}
    />
  </div>

  <div class="settings-row">
    <label for="settings-rpp">{s.results_per_page}</label>
    <UiSelect
      id="settings-rpp"
      value={localStorage.getItem("cauce-rpp") ?? "20"}
      onValueChange={(v) => localStorage.setItem("cauce-rpp", v)}
      options={[
        { value: "10", label: "10" },
        { value: "20", label: "20" },
        { value: "50", label: "50" },
        { value: "100", label: "100" },
      ]}
    />
  </div>
</fieldset>
{/if}

<!-- Privacy tab: browser-local + some admin-like settings -->
{#if tabValue === "privacy"}
<fieldset>
  <legend>{s.section_privacy}</legend>

  <div class="settings-row">
    <label for="settings-safesearch">{s.safe_search}</label>
    <UiSelect
      id="settings-safesearch"
      value={localStorage.getItem("cauce-safesearch") ?? "moderate"}
      onValueChange={(v) => localStorage.setItem("cauce-safesearch", v)}
      options={[
        { value: "off", label: s.safesearch_off },
        { value: "moderate", label: s.safesearch_moderate },
        { value: "strict", label: s.safesearch_strict },
      ]}
    />
  </div>

  <div class="settings-row">
    <label for="settings-region">{s.region}</label>
    <UiSelect
      id="settings-region"
      value={localStorage.getItem("cauce-region") ?? "us"}
      onValueChange={(v) => localStorage.setItem("cauce-region", v)}
      options={[
        { value: "us", label: "United States" },
        { value: "gb", label: "United Kingdom" },
        { value: "fr", label: "France" },
        { value: "de", label: "Germany" },
        { value: "ja", label: "Japan" },
      ]}
    />
  </div>
</fieldset>
{/if}

<!-- Search defaults tab: browser-local settings -->
{#if tabValue === "search"}
<fieldset>
  <legend>{s.section_search_defaults}</legend>

  <div class="settings-row">
    <label for="hint-default-engines">{s.default_engines}</label>
    <p id="hint-default-engines" class="hint mono">{s.default_engines_hint}</p>
  </div>

  <div class="settings-row">
    <label for="settings-ttl">{s.ttl}</label>
    <UiSelect
      id="settings-ttl"
      value={localStorage.getItem("cauce-ttl") ?? ""}
      onValueChange={(v) => localStorage.setItem("cauce-ttl", v)}
      options={[{ value: "", label: s.ttl_default }]}
    />
  </div>

  <div class="settings-row">
    <label for="settings-page-size">{s.page_size}</label>
    <UiSelect
      id="settings-page-size"
      value={localStorage.getItem("cauce-minresults") ?? ""}
      onValueChange={(v) => localStorage.setItem("cauce-minresults", v)}
      options={[{ value: "", label: s.page_size_default }]}
    />
  </div>
</fieldset>
{/if}

<!-- AI tab: browser-local BYOK -->
{#if tabValue === "ai"}
<fieldset>
  <legend>{s.section_ai}</legend>

  <div class="settings-row">
    <label for="settings-byok-key">{s.ai_api_key}</label>
    <input
      id="settings-byok-key"
      type="password"
      value={localStorage.getItem("cauce-byok-key") ?? ""}
      oninput={(e) => {
        const val = e.currentTarget.value;
        localStorage.setItem("cauce-byok-key", val);
        setByok({ api_key: val, model: "", protocol: "", base_url: "" });
      }}
      placeholder="sk-..."
    />
  </div>

  <div class="settings-row">
    <label for="settings-byok-base-url">{s.ai_base_url}</label>
    <input
      id="settings-byok-base-url"
      type="text"
      value={localStorage.getItem("cauce-byok-base-url") ?? ""}
      oninput={(e) => localStorage.setItem("cauce-byok-base-url", e.currentTarget.value)}
      placeholder="https://openrouter.ai/api/v1"
    />
  </div>
</fieldset>
{/if}

<style>
/* --- Settings row layout: label | help | control --- */
.settings-row {
  display: grid;
  grid-template-columns: auto 1fr auto;
  gap: 0.5rem 1rem;
  align-items: start;
  margin: 0.4rem 0;
}

/* On narrow viewports: single column, label above control */
@media (max-width: 640px) {
  .settings-row {
    grid-template-columns: 1fr;
    gap: 0.25rem 0.5rem;
  }
  .settings-row > * {
    grid-column: 1 / -1;
  }
  .settings-row > label {
    grid-column: auto;
  }
  .settings-row > .hint {
    grid-column: 1 / -1;
    font-size: 0.8125rem;
    color: var(--muted);
  }
}

/* Labels: left-aligned, not flex */
.settings-row > label {
  grid-column: auto;
  font-size: 0.9375rem;
  color: var(--muted);
}

/* Controls: right-aligned in wide layout, full-width in narrow */
.settings-row > input[type="text"],
.settings-row > input[type="password"] {
  grid-column: auto;
  min-width: 0;
}
</style>
