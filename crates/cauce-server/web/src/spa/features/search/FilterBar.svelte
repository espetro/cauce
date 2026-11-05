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
  /* Styles live in FilterBar.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./FilterBar.css";
</style>
