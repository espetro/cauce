<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/alert-dialog` — vendored Bits UI `AlertDialog`
  (AlertDialog.Root/Portal/Overlay/Content/Title/Description/Action/
  Cancel) restyled on the app tokens. Composed by `ui/confirm`'s
  UiConfirmHost — features never mount it directly. Controlled via
  `bind:open` plus `onConfirm`/`onCancel` callbacks; Escape and
  programmatic close both surface as `onCancel`. `danger` switches
  the action to the warn fill for destructive ops.

  `open` is `$bindable` and bound through to `AlertDialog.Root`: a
  bare `{open}` prop lets Root's internal `$bindable` fallback drift
  from the parent (same trap as `ui/toggle-group`'s value — a close
  write lands in the fallback and the parent's unchanged expression
  never pushes back). The `answered` latch swallows the trailing
  close write a settled Cancel/Action click still emits — without
  it the write would cancel the NEXT queued confirm — and the
  effect re-arms it on every episode change (`key` identity or
  `open` flip).
-->
<script lang="ts">
  import { AlertDialog } from "bits-ui";
  // The component's Tailwind composition (alert-dialog.css) is a plain stylesheet imported from the script: the build
  // resolves its `@apply` against the app theme, and svelte-check's
  // CSS parser — which does not know Tailwind's at-rules — never
  // sees them.
  import "./alert-dialog.css";

  interface Props {
    open: boolean;
    /** Identity of the request being shown — a change re-arms
        `answered`; without it two queued requests with identical
        title/description are indistinguishable episodes. */
    key?: unknown;
    title: string;
    description: string;
    confirmLabel: string;
    cancelLabel: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let {
    open = $bindable(),
    key,
    title,
    description,
    confirmLabel,
    cancelLabel,
    danger = false,
    onConfirm,
    onCancel,
  }: Props = $props();

  // Action/Cancel are Close primitives — their click still pushes a
  // close write through the binding, which must not be reported as
  // a second (cancel) answer. Prop-driven opens never go through
  // the setter, so open/key changes are the re-arm signal.
  let answered = false;

  $effect(() => {
    void key;
    void open;
    answered = false;
  });

  // Binding setter — every close Root initiates (Escape, Close
  // primitives) lands here, so an unsettled close IS a cancel
  // answer; no close path can bypass the queue.
  function setOpen(next: boolean): void {
    if (next) {
      open = true;
      return;
    }
    if (!answered) onCancel();
    answered = false;
    open = false;
  }

  function handleConfirm(): void {
    answered = true;
    onConfirm();
  }

  function handleCancel(): void {
    answered = true;
    onCancel();
  }
</script>

<AlertDialog.Root bind:open={() => open, setOpen}>
  <AlertDialog.Portal>
    <AlertDialog.Overlay class="ui-alert-dialog-overlay" />
    <AlertDialog.Content class="ui-alert-dialog-content">
      <AlertDialog.Title class="ui-alert-dialog-title">
        {title}
      </AlertDialog.Title>
      <AlertDialog.Description class="ui-alert-dialog-description">
        {description}
      </AlertDialog.Description>
      <div class="ui-alert-dialog-actions">
        <AlertDialog.Cancel
          class="ui-alert-dialog-btn"
          onclick={handleCancel}
        >
          {cancelLabel}
        </AlertDialog.Cancel>
        <AlertDialog.Action
          class="ui-alert-dialog-btn"
          data-variant={danger ? "danger" : "primary"}
          onclick={handleConfirm}
        >
          {confirmLabel}
        </AlertDialog.Action>
      </div>
    </AlertDialog.Content>
  </AlertDialog.Portal>
</AlertDialog.Root>

