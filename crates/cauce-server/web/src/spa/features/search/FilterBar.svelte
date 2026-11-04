<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `FilterBar` — one shared filter-row primitive for the `/app/search` and
  `/app/history` surfaces (plan §182-188, wave D3). The two surfaces render
  the same grid of one-control-per-cell entries and read as one system.

  Layout contract (plan §5 + D3): a grid, not a wrapping flex row; every
  control is one cell; labels truncate with an ellipsis; the only stretching
  control is the free-text input, placed last; below ~700px the row collapses
  to one column.

  The primitive is fully controlled: the parent owns every field's value and
  decides when a change becomes a query (selects/toggles commit on change, the
  free-text query commits on submit). The caller passes fields pre-shaped by
  the capability gate so the row never advertises an engine filter the selected
  engines cannot honour.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import UiButton from "../../ui/button.svelte";
  import UiCheckbox from "../../ui/checkbox.svelte";
  import UiMultiSelect from "../../ui/multi-select.svelte";
  import UiSelect from "../../ui/select.svelte";

  export type FilterFieldType = "select" | "multi-select" | "checkbox" | "text";

  export interface FilterFieldOption {
    value: string;
    label: string;
  }

  export interface FilterField {
    key: string;
    label: string;
    type: FilterFieldType;
    value: unknown;
    options?: FilterFieldOption[];
    placeholder?: string;
    disabled?: boolean;
    hint?: string;
    ariaLabel?: string;
    /** Select/toggle fields commit on change; text fields commit on submit. */
    submitOnChange?: boolean;
  }

  interface FilterBarProps {
    fields: FilterField[];
    showSubmit?: boolean;
    submitLabel?: string;
    onChange?: (key: string, value: unknown) => void;
    onSubmit?: () => void;
  }

  let {
    fields,
    showSubmit = false,
    submitLabel = "",
    onChange,
    onSubmit,
  }: FilterBarProps = $props();

  function commit(key: string, value: unknown): void {
    onChange?.(key, value);
  }

  /** Local text drafts for fields on a submit path (the free-text query commits on submit). */
  let textValues = $state<Record<string, string>>({});

  function handleTextInput(e: Event, field: FilterField): void {
    const el = e.currentTarget;
    if (!(el instanceof HTMLInputElement)) return;
    if (field.submitOnChange === false) {
      textValues[field.key] = el.value;
      return;
    }
    commit(field.key, el.value);
  }

  function handleSubmit(): void {
    for (const field of fields) {
      if (field.type === "text" && field.submitOnChange === false && field.key in textValues) {
        commit(field.key, textValues[field.key]);
      }
    }
    textValues = {};
    onSubmit?.();
  }

  function textValue(field: FilterField): string {
    if (field.type === "text" && field.submitOnChange === false && field.key in textValues) {
      return textValues[field.key];
    }
    return String(field.value ?? "");
  }
</script>

<div class="filter-bar" data-testid="filter-bar">
  {#each fields as field (field.key)}
    {@const f = field}
    <div class="fb-cell">
      <span class="fb-label">{f.label}</span>
      {#if f.type === "select"}
        <UiSelect
          ariaLabel={f.ariaLabel ?? f.label}
          value={String(f.value ?? "")}
          placeholder={f.placeholder}
          disabled={f.disabled}
          options={f.options ?? []}
          onValueChange={(v) => commit(f.key, v)}
        />
      {:else if f.type === "multi-select"}
        <UiMultiSelect
          ariaLabel={f.ariaLabel ?? f.label}
          value={(f.value as string[] | undefined) ?? []}
          placeholder={f.placeholder}
          disabled={f.disabled}
          options={f.options ?? []}
          onValueChange={(v) => commit(f.key, v)}
        />
      {:else if f.type === "checkbox"}
        <UiCheckbox
          checked={f.value === true}
          disabled={f.disabled}
          ariaLabel={f.ariaLabel ?? f.label}
          onCheckedChange={(v) => commit(f.key, v)}
        />
      {:else if f.type === "text"}
        <input
          class="fb-input"
          type="search"
          value={textValue(f)}
          placeholder={f.placeholder}
          aria-label={f.ariaLabel ?? f.label}
          disabled={f.disabled}
          data-disabled={f.disabled ? "" : undefined}
          oninput={(e) => handleTextInput(e, f)}
        />
      {/if}
      {#if f.hint}
        <p class="fb-hint">{f.hint}</p>
      {/if}
    </div>
  {/each}
</div>
{#if showSubmit}
  <div class="fb-submit-row">
    <UiButton type="submit" onclick={handleSubmit}>{submitLabel}</UiButton>
  </div>
{/if}

<style>
  .filter-bar {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(10rem, 1fr));
    gap: 0.75rem;
    align-items: end;
  }

  .fb-cell {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.25rem;
    min-width: 0;
  }

  .fb-label {
    /* Truncate long labels instead of wrapping. */
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--muted);
  }

  .fb-cell :global(.ui-select-trigger) {
    width: 100%;
  }

  .fb-cell :global(.ui-multi-select-trigger) {
    width: 100%;
  }

  .fb-input {
    width: 100%;
    min-width: 0;
    padding: 0 0.625rem;
    height: var(--control-h);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    font-size: var(--text-base);
  }

  .fb-input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .fb-input[data-disabled] {
    opacity: 0.5;
    cursor: default;
  }

  .fb-hint {
    margin: 0;
    font-size: 0.75rem;
    color: var(--muted);
  }

  .fb-submit-row {
    display: flex;
    justify-content: flex-end;
    margin-top: 0.75rem;
  }

  @media (max-width: 700px) {
    .filter-bar {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .fb-input {
      transition: none;
    }
  }
</style>
