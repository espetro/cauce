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
  // The component's Tailwind composition (multi-select.css) is a plain stylesheet imported from the script: the build
  // resolves its `@apply` against the app theme, and svelte-check's
  // CSS parser — which does not know Tailwind's at-rules — never
  // sees them.
  import "./multi-select.css";

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

