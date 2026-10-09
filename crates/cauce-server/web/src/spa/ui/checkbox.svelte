<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/checkbox` — vendored Bits UI `Checkbox` restyled on the app
  tokens (plan §7.3 "Bits UI + owned ui/ wrappers"). One-way data flow:
  pass `checked` + `onCheckedChange`, which is what formisch's
  `field.onInput` expects.
-->
<script lang="ts">
  import { Checkbox } from "bits-ui";

  interface Props {
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    disabled?: boolean;
    name?: string;
    id?: string;
    required?: boolean;
    ariaInvalid?: boolean;
    ariaLabel?: string;
  }

  let {
    checked = false,
    onCheckedChange,
    disabled = false,
    name,
    id,
    required,
    ariaInvalid,
    ariaLabel,
  }: Props = $props();
</script>

<Checkbox.Root
  class="ui-checkbox"
  {checked}
  {onCheckedChange}
  {disabled}
  {name}
  {id}
  {required}
  aria-invalid={ariaInvalid || undefined}
  aria-label={ariaLabel}
>
  {#snippet children({ checked: on })}
    {#if on}
      <svg
        class="ui-checkbox-mark"
        viewBox="0 0 12 12"
        fill="none"
        aria-hidden="true"
      >
        <path
          d="M2.5 6.5 5 9l4.5-6"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    {/if}
  {/snippet}
</Checkbox.Root>

<style>
  :global(.ui-checkbox) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1rem;
    height: 1rem;
    padding: 0;
    flex: none;
    border: 1px solid var(--border);
    border-radius: 0.25rem;
    background: var(--bg);
    color: var(--bg);
    cursor: pointer;
    vertical-align: -0.125em;
  }

  :global(.ui-checkbox[data-state="checked"]) {
    border-color: var(--accent);
    background: var(--accent);
  }

  :global(.ui-checkbox:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  :global(.ui-checkbox[data-disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ui-checkbox .ui-checkbox-mark) {
    width: 0.75rem;
    height: 0.75rem;
  }
</style>
