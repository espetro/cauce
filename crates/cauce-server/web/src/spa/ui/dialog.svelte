<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  project, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/dialog` — vendored Bits UI `Dialog`
  (Dialog.Root/Portal/Overlay/Content/Title/Description/Close) restyled
  on the app tokens. The generic informational modal (the confirm
  AlertDialog stays separate — its contract is promise-based queueing,
  not composition). Content is caller-owned through the default
  snippet; Escape and the X Close both close it.
-->
<script lang="ts">
  import { Dialog } from "bits-ui";
  import { XIcon } from "phosphor-svelte";
  import type { Snippet } from "svelte";
  // The component's Tailwind composition (dialog.css) is a plain stylesheet imported from the script: the build
  // resolves its `@apply` against the app theme, and svelte-check's
  // CSS parser — which does not know Tailwind's at-rules — never
  // sees them.
  import "./dialog.css";

  interface Props {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    title: string;
    children: Snippet;
  }

  let { open, onOpenChange, title, children }: Props = $props();
</script>

<Dialog.Root {open} {onOpenChange}>
  <Dialog.Portal>
    <Dialog.Overlay class="ui-dialog-overlay" />
    <Dialog.Content class="ui-dialog-content">
      <div class="ui-dialog-head">
        <Dialog.Title class="ui-dialog-title">{title}</Dialog.Title>
        <Dialog.Close class="ui-dialog-close" aria-label="close">
          <XIcon size={14} />
        </Dialog.Close>
      </div>
      <Dialog.Description class="vh">{title}</Dialog.Description>
      {@render children()}
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>

