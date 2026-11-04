<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/multi-select` — vendored Bits UI `Select` (`type="multiple"`)
  restyled on the app tokens. One-way data flow: pass `value` (string[])
  + `onValueChange`, which is what formisch's `field.onInput` expects and
  what a controlled `FilterBar` needs. An option value of "" is allowed
  here — it maps to an internal sentinel because Bits UI rejects empty
  item values.
-->
<script lang="ts">
  import { Select } from "bits-ui";
  import { CaretDownIcon, CheckIcon } from "phosphor-svelte";

  interface Option {
    value: string;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    value: string[];
    options: Option[];
    onValueChange?: (value: string[]) => void;
    placeholder?: string;
    disabled?: boolean;
    name?: string;
    id?: string;
    ariaLabel?: string;
    ariaInvalid?: boolean;
  }

  let {
    value = [],
    options,
    onValueChange,
    placeholder,
    disabled = false,
    name,
    id,
    ariaLabel,
    ariaInvalid,
  }: Props = $props();

  // Bits UI items reject "" as a value — map it to a sentinel in both
  // directions so callers keep plain "" semantics.
  const EMPTY = "__none__";
  const items = $derived(
    options.map((o) => ({
      ...o,
      value: o.value === "" ? EMPTY : o.value,
    })),
  );
  const rootValue = $derived(value.map((v) => (v === "" ? EMPTY : v)));
  const selectedLabels = $derived(
    options.filter((o) => value.includes(o.value)).map((o) => o.label),
  );
  const selectedCount = $derived(value.length);

  function handleChange(v: string[]): void {
    onValueChange?.(
      v.map((v) => (v === EMPTY ? "" : v)),
    );
  }

  function triggerLabel(): string {
    if (selectedCount === 0) return placeholder ?? "";
    if (selectedCount === options.length && options.length > 0) return "All";
    return selectedLabels.join(", ");
  }
</script>

<Select.Root
  type="multiple"
  value={rootValue}
  onValueChange={handleChange}
  {items}
  {disabled}
  {name}
>
  <!-- axe label-content-name-mismatch: the trigger's visible text is the
       selection summary, so the accessible name must contain it. -->
  <Select.Trigger
    class="ui-multi-select-trigger"
    {id}
    {disabled}
    aria-label={ariaLabel === undefined
      ? undefined
      : `${ariaLabel}: ${triggerLabel()}`}
    aria-invalid={ariaInvalid || undefined}
  >
    <span class="ui-multi-select-label" class:placeholder={selectedCount === 0}>
      {selectedCount === 0 ? placeholder : triggerLabel()}
    </span>
    <CaretDownIcon class="ui-multi-select-chevron" aria-hidden="true" />
  </Select.Trigger>
  <Select.Portal>
    <Select.Content class="ui-multi-select-content" sideOffset={4}>
      <Select.Viewport>
        {#each items as item (item.value)}
          <Select.Item
            class="ui-multi-select-item"
            value={item.value}
            label={item.label}
            disabled={item.disabled}
          >
            {#snippet children({ selected })}
              <CheckIcon
                class="ui-multi-select-check"
                aria-hidden="true"
                style={selected ? "visibility:visible" : "visibility:hidden"}
              />
              {item.label}
            {/snippet}
          </Select.Item>
        {/each}
      </Select.Viewport>
    </Select.Content>
  </Select.Portal>
</Select.Root>

<style>
  :global(.ui-multi-select-trigger) {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    flex: 1 1 auto;
    min-width: 0;
    /* DS-20D: the shared control height. */
    height: var(--control-h);
    padding: 0 0.625rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  :global(.ui-multi-select-trigger:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  :global(.ui-multi-select-trigger[data-disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ui-multi-select-label.placeholder) {
    color: var(--muted);
  }

  :global(.ui-multi-select-chevron) {
    width: 0.75rem;
    height: 0.75rem;
    flex: none;
    color: var(--muted);
  }

  :global(.ui-multi-select-content) {
    min-width: var(--bits-select-content-available-width, 8rem);
    padding: 0.25rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    box-shadow: var(--shadow);
    z-index: 50;
  }

  :global(.ui-multi-select-item) {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.4rem;
    border-radius: calc(var(--radius) - 0.2rem);
    font-size: 0.9375rem;
    cursor: pointer;
    user-select: none;
  }

  :global(.ui-multi-select-item[data-highlighted]) {
    background: var(--greyed-bg);
  }

  :global(.ui-multi-select-item[data-disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ui-multi-select-check) {
    width: 0.75rem;
    height: 0.75rem;
    flex: none;
    color: var(--accent);
  }
</style>
