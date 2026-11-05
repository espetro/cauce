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
  // The component's Tailwind composition (select.css) is a plain stylesheet imported from the script: the build
  // resolves its `@apply` against the app theme, and svelte-check's
  // CSS parser — which does not know Tailwind's at-rules — never
  // sees them.
  import "./select.css";

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

  // Bits UI items reject "" as a value — map it to a sentinel in both
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

