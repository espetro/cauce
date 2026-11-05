<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/toggle-group` — vendored Bits UI `ToggleGroup`
  (ToggleGroup.Root/Item, `type="single"`) restyled on the app tokens;
  the pill segment that replaces hand-rolled `aria-pressed` button bars
  (Omnibox Search/✦Ask, dashboard `?days=`). `value` is `$bindable` —
  bind it (a getter/setter pair works); passing a bare `value` leaves
  the group's presses on a component-local fallback that can drift
  from the parent's state. `onValueChange` is an optional extra hook.
  Roving focus and `aria-pressed` come free from the primitive.
  No `name`/`required`/`ariaInvalid` — not a form field.
-->
<script lang="ts">
  import { ToggleGroup } from "bits-ui";
  // The component's Tailwind composition (toggle-group.css) is a plain stylesheet imported from the script: the build
  // resolves its `@apply` against the app theme, and svelte-check's
  // CSS parser — which does not know Tailwind's at-rules — never
  // sees them.
  import "./toggle-group.css";

  interface ToggleOption {
    value: string;
    label: string;
    disabled?: boolean;
    /**
     * Soft-disable: `aria-disabled` without Bits' `disabled` — the item
     * stays focusable so its own explanation (a wrapper popover) is
     * keyboard-reachable; the activation is refused upstream.
     */
    softDisabled?: boolean;
  }

  interface Props {
    value: string;
    options: ToggleOption[];
    onValueChange?: (value: string) => void;
    disabled?: boolean;
    id?: string;
    ariaLabel?: string;
  }

  let {
    value = $bindable(),
    options,
    onValueChange,
    disabled = false,
    id,
    ariaLabel,
  }: Props = $props();
</script>

<ToggleGroup.Root
  type="single"
  class="ui-toggle-group"
  bind:value
  {onValueChange}
  {disabled}
  {id}
  aria-label={ariaLabel}
>
  {#each options as option (option.value)}
    <ToggleGroup.Item
      class="ui-toggle-item{option.softDisabled ? ' ui-item-soft-disabled' : ''}"
      value={option.value}
      disabled={option.disabled}
      aria-disabled={option.softDisabled || undefined}
      aria-label={option.label}
    >
      {option.label}
    </ToggleGroup.Item>
  {/each}
</ToggleGroup.Root>

