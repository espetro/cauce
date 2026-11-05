<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/tooltip` — vendored Bits UI `Tooltip`
  (Tooltip.Root/Trigger/Portal/Content) restyled on the app tokens.
  `Tooltip.Provider` is mounted once by `app/App.svelte` (DS-02), not
  here. The `trigger` snippet receives the merged trigger `props` to
  spread on the caller's own element (`{#snippet trigger(props)}
  <button {...props}>…` / `<UiButton {...props}>`) — delegation avoids
  nesting a button inside the primitive's own. `content` is plain text;
  `side`/`sideOffset` position the floating layer. Optional `open` +
  `onOpenChange` keep the one-way contract; omit them for normal
  hover/focus behavior. No `name`/`required`/`ariaInvalid` — not a form
  field.
-->
<script lang="ts">
  import { Tooltip } from "bits-ui";
  import type { Snippet } from "svelte";
  // The component's Tailwind composition (tooltip.css) is a plain stylesheet imported from the script: the build
  // resolves its `@apply` against the app theme, and svelte-check's
  // CSS parser — which does not know Tailwind's at-rules — never
  // sees them.
  import "./tooltip.css";

  interface Props {
    content: string;
    side?: "top" | "right" | "bottom" | "left";
    sideOffset?: number;
    trigger: Snippet<[props: Record<string, unknown>]>;
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    disabled?: boolean;
    delayDuration?: number;
  }

  let {
    content,
    side = "top",
    sideOffset = 4,
    trigger,
    open,
    onOpenChange,
    disabled = false,
    delayDuration,
  }: Props = $props();
</script>

<Tooltip.Root {open} {onOpenChange} {disabled} {delayDuration}>
  <Tooltip.Trigger {disabled}>
    {#snippet child({ props })}
      {@render trigger(props)}
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Portal>
    <Tooltip.Content class="ui-tooltip-content" {side} {sideOffset}>
      {content}
    </Tooltip.Content>
  </Tooltip.Portal>
</Tooltip.Root>

