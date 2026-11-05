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
  import { CheckIcon } from "phosphor-svelte";
  // The component's Tailwind composition (checkbox.css) is a plain stylesheet imported from the script: the build
  // resolves its `@apply` against the app theme, and svelte-check's
  // CSS parser — which does not know Tailwind's at-rules — never
  // sees them.
  import "./checkbox.css";

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
      <CheckIcon class="ui-checkbox-mark" aria-hidden="true" />
    {/if}
  {/snippet}
</Checkbox.Root>

