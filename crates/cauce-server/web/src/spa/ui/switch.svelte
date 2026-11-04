<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  project, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/switch` — pill-track toggle with visible On/Off labels (DDG
  settings reference: toggles are a hidden checkbox + label). One-way
  data flow: pass `checked` + `onCheckedChange`, which is what formisch's
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
    label?: string;
    labelOn?: string;
    labelOff?: string;
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
    label,
    labelOn = "On",
    labelOff = "Off",
  }: Props = $props();
</script>

<Checkbox.Root
  class="ui-switch"
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
    <span class="ui-switch-track"
      data-state={on ? "checked" : "unchecked"}
      role="switch"
      aria-checked={on}
    >
      <span class="ui-switch-label ui-switch-label-on">{labelOn}</span>
      <span class="ui-switch-label ui-switch-label-off">{labelOff}</span>
    </span>
    {#if label}<span class="ui-switch-text">{label}</span>{/if}
  {/snippet}
</Checkbox.Root>

<style>
  :global(.ui-switch) {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
    user-select: none;
    -webkit-user-select: none;
  }

  :global(.ui-switch[disabled]) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ui-switch-track) {
    position: relative;
    display: inline-flex;
    align-items: center;
    min-width: 2.5rem;
    height: 1.25rem;
    padding: 0;
    flex: none;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--greyed-bg);
    transition:
      background 120ms ease-out,
      border-color 120ms ease-out;
  }

  :global(.ui-switch-track[data-state="checked"]) {
    background: var(--accent);
    border-color: var(--accent);
  }

  :global(.ui-switch-label) {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    font-size: 0.625rem;
    font-weight: 600;
    white-space: nowrap;
    padding: 0 0.25rem;
    color: var(--bg);
    line-height: 1;
    pointer-events: none;
    transition: color 120ms ease-out;
  }

  :global(.ui-switch-label-on) {
    inset-inline-start: 0.25rem;
  }

  :global(.ui-switch-label-off) {
    inset-inline-end: 0.25rem;
  }

  :global(.ui-switch-track[data-state="checked"] .ui-switch-label-on) {
    color: var(--on-accent);
  }

  :global(.ui-switch-track[data-state="unchecked"] .ui-switch-label-off) {
    color: var(--muted);
  }

  :global(.ui-switch-text) {
    font-size: 0.9375rem;
    color: var(--fg);
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.ui-switch-track),
    :global(.ui-switch-label) {
      transition: none;
    }
  }
</style>