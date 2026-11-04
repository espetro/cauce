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

<style>
  :global(.ui-dialog-overlay) {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    z-index: 50;
    /* ScrollLock sets body pointer-events:none — re-arm the scrim
       so it eats clicks instead of leaking them to the page (same
       recipe as ui/alert-dialog). */
    pointer-events: auto;
    animation: ui-dialog-overlay-in 200ms var(--ease-out);
  }

  @keyframes ui-dialog-overlay-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  :global(.ui-dialog-content) {
    position: fixed;
    top: 50%;
    left: 50%;
    width: min(92vw, 40rem);
    max-height: 80vh;
    overflow: auto;
    padding: 1rem 1.25rem;
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) * 1.5);
    background: var(--bg);
    color: var(--fg);
    box-shadow: var(--shadow);
    z-index: 51;
    transform: translate(-50%, -50%);
    animation: ui-dialog-content-in 220ms var(--ease-out);
  }

  @keyframes ui-dialog-content-in {
    from {
      opacity: 0;
      transform: translate(-50%, -50%) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translate(-50%, -50%) scale(1);
    }
  }

  :global(.ui-dialog-head) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 0.5rem;
  }

  :global(.ui-dialog-title) {
    margin: 0;
    font-size: 1rem;
    font-weight: 700;
  }

  :global(.ui-dialog-close) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--muted);
    cursor: pointer;
  }

  :global(.ui-dialog-close:hover) {
    background: var(--greyed-bg);
    color: var(--fg);
  }

  :global(.ui-dialog-close:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
