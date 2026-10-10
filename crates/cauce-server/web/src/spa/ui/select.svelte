<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/select` — vendored Bits UI `Select` restyled on the app tokens
  (plan §7.3 "Bits UI + owned ui/ wrappers"). Single-select only, one-way
  data flow (`value` + `onValueChange`) so it can sit inside a formisch
  `Field` (`field.onInput`). An option value of "" is allowed here — it
  maps to an internal sentinel because Bits UI rejects empty item values.
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
    value: string;
    options: Option[];
    onValueChange?: (value: string) => void;
    placeholder?: string;
    disabled?: boolean;
    name?: string;
    id?: string;
    ariaLabel?: string;
    ariaInvalid?: boolean;
  }

  let {
    value,
    options,
    onValueChange,
    placeholder,
    disabled = false,
    name,
    id,
    ariaLabel,
    ariaInvalid,
  }: Props = $props();

  // Bits UI items can't carry "" — map it to a sentinel in both
  // directions so callers keep plain "" semantics.
  const EMPTY = "__none__";
  const items = $derived(
    options.map((o) => ({
      ...o,
      value: o.value === "" ? EMPTY : o.value,
    })),
  );
  const rootValue = $derived(value === "" ? EMPTY : value);
  const selectedLabel = $derived(
    options.find((o) => o.value === value)?.label ?? placeholder ?? "",
  );

  function handleChange(v: string): void {
    onValueChange?.(v === EMPTY ? "" : v);
  }
</script>

<Select.Root
  type="single"
  value={rootValue}
  onValueChange={handleChange}
  {items}
  {disabled}
  {name}
>
  <!-- axe label-content-name-mismatch: the trigger's visible text is the
       selected value, so the accessible name must contain it —
       "Since: Any time", not just "Since". -->
  <Select.Trigger
    class="ui-select-trigger"
    {id}
    {disabled}
    aria-label={ariaLabel === undefined
      ? undefined
      : `${ariaLabel}: ${selectedLabel || placeholder || ""}`}
    aria-invalid={ariaInvalid || undefined}
  >
    <span class="ui-select-label" class:placeholder={selectedLabel === ""}>
      {selectedLabel === "" ? placeholder : selectedLabel}
    </span>
    <CaretDownIcon class="ui-select-chevron" aria-hidden="true" />
  </Select.Trigger>
  <Select.Portal>
    <Select.Content class="ui-select-content" sideOffset={4}>
      <Select.Viewport>
        {#each items as item (item.value)}
          <Select.Item
            class="ui-select-item"
            value={item.value}
            label={item.label}
            disabled={item.disabled}
          >
            {#snippet children({ selected })}
              <CheckIcon
                class="ui-select-check"
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
  :global(.ui-select-trigger) {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    flex: 1 1 auto;
    min-width: 0;
    padding: 0.25rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  :global(.ui-select-trigger:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  :global(.ui-select-trigger[data-disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ui-select-label.placeholder) {
    color: var(--muted);
  }

  :global(.ui-select-chevron) {
    width: 0.75rem;
    height: 0.75rem;
    flex: none;
    color: var(--muted);
  }

  :global(.ui-select-content) {
    min-width: var(--bits-select-content-available-width, 8rem);
    padding: 0.25rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    box-shadow: var(--shadow);
    z-index: 50;
  }

  :global(.ui-select-item) {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.4rem;
    border-radius: calc(var(--radius) - 0.2rem);
    font-size: 0.9375rem;
    cursor: pointer;
    user-select: none;
  }

  :global(.ui-select-item[data-highlighted]) {
    background: var(--greyed-bg);
  }

  :global(.ui-select-item[data-disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ui-select-check) {
    width: 0.75rem;
    height: 0.75rem;
    flex: none;
    color: var(--accent);
  }
</style>
