<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/collapsible` — vendored Bits UI `Collapsible`
  (Collapsible.Root/Trigger/Content) restyled on the app tokens; replaces
  every `<details>`/`<summary>` disclosure. One-way data flow: pass
  `bind:open` or `open` + `onOpenChange` (omit all for uncontrolled use —
  `open` is `$bindable` so Root's internal state still reaches the
  trigger snippet below).
  `onOpenChangeComplete` fires after the height animation — the hook for
  lazy payload loads (archive/cache rows). The `trigger` snippet gets
  the current `open` so callers can rotate their own chevron. Height
  animates via `--bits-collapsible-content-height` (compositor-safe,
  never a layout transition). No `name`/`required`/`ariaInvalid` — this
  is a disclosure widget, not a form field.
-->
<script lang="ts">
  import { Collapsible } from "bits-ui";
  import type { Snippet } from "svelte";
  // The component's Tailwind composition (collapsible.css) is a plain stylesheet imported from the script: the build
  // resolves its `@apply` against the app theme, and svelte-check's
  // CSS parser — which does not know Tailwind's at-rules — never
  // sees them.
  import "./collapsible.css";

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    onOpenChangeComplete?: (open: boolean) => void;
    disabled?: boolean;
    id?: string;
    trigger: Snippet<[open: boolean]>;
    children: Snippet;
  }

  let {
    open = $bindable(false),
    onOpenChange,
    onOpenChangeComplete,
    disabled = false,
    id,
    trigger,
    children,
  }: Props = $props();
</script>

<Collapsible.Root
  class="ui-collapsible"
  bind:open
  {onOpenChange}
  {onOpenChangeComplete}
  {disabled}
>
  <Collapsible.Trigger class="ui-collapsible-trigger" {id} {disabled}>
    {@render trigger(open)}
  </Collapsible.Trigger>
  <Collapsible.Content class="ui-collapsible-content">
    {@render children()}
  </Collapsible.Content>
</Collapsible.Root>

