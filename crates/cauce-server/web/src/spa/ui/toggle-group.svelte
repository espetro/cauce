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

  interface ToggleOption {
    value: string;
    label: string;
    disabled?: boolean;
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
      class="ui-toggle-item"
      value={option.value}
      disabled={option.disabled}
      aria-label={option.label}
    >
      {option.label}
    </ToggleGroup.Item>
  {/each}
</ToggleGroup.Root>

<style>
  /* Same pill segment the composer tool row already had (.segment). */
  :global(.ui-toggle-group) {
    display: inline-flex;
    gap: 0.125rem;
    padding: 0.125rem;
    border: 1px solid var(--border);
    border-radius: 999px;
  }

  :global(.ui-toggle-item) {
    padding: 0.25rem 0.75rem;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 0.8125rem;
    cursor: pointer;
    transition:
      background-color 120ms ease-out,
      color 120ms ease-out;
  }

  :global(.ui-toggle-item[data-state="on"]) {
    background: var(--accent);
    /* Same on-accent recipe as ui/checkbox's mark. */
    color: var(--bg);
  }

  :global(.ui-toggle-item:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  :global(.ui-toggle-item[data-disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ui-toggle-group[data-disabled]) {
    opacity: 0.5;
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.ui-toggle-item) {
      transition: none;
    }
  }
</style>
