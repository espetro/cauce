<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/alert-dialog` — vendored Bits UI `AlertDialog`
  (AlertDialog.Root/Portal/Overlay/Content/Title/Description/Action/
  Cancel) restyled on the app tokens. Composed by `ui/confirm`'s
  UiConfirmHost — features never mount it directly. One-way data flow:
  controlled `open` plus `onConfirm`/`onCancel` callbacks; Escape and
  programmatic close both surface as `onCancel`, and the `answered`
  guard keeps a button answer from also reporting the Close primitive's
  `onOpenChange(false)` as a dismissal. `danger` switches the action to
  the warn fill for destructive ops.
-->
<script lang="ts">
  import { AlertDialog } from "bits-ui";

  interface Props {
    open: boolean;
    title: string;
    description: string;
    confirmLabel: string;
    cancelLabel: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let {
    open,
    title,
    description,
    confirmLabel,
    cancelLabel,
    danger = false,
    onConfirm,
    onCancel,
  }: Props = $props();

  // Action/Cancel are Close primitives — their click also fires
  // `onOpenChange(false)`, which must not be reported as a second
  // (cancel) answer.
  let answered = false;

  function handleOpenChange(next: boolean): void {
    if (next) {
      answered = false;
      return;
    }
    if (!answered) onCancel();
    answered = false;
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

<AlertDialog.Root {open} onOpenChange={handleOpenChange}>
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

<style>
  :global(.ui-alert-dialog-overlay) {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    z-index: 50;
    animation: ui-alert-overlay-in 200ms var(--ease-out);
  }

  @keyframes ui-alert-overlay-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  :global(.ui-alert-dialog-content) {
    position: fixed;
    top: 50%;
    left: 50%;
    width: min(92vw, 24rem);
    padding: 1rem 1.25rem;
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) * 1.5);
    background: var(--bg);
    color: var(--fg);
    box-shadow: var(--shadow);
    z-index: 51;
    transform: translate(-50%, -50%);
    animation: ui-alert-content-in 220ms var(--ease-out);
  }

  @keyframes ui-alert-content-in {
    from {
      opacity: 0;
      transform: translate(-50%, -50%) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translate(-50%, -50%) scale(1);
    }
  }

  :global(.ui-alert-dialog-title) {
    margin: 0 0 0.375rem;
    font-size: 1rem;
    font-weight: 700;
  }

  :global(.ui-alert-dialog-description) {
    margin: 0;
    color: var(--muted);
    font-size: 0.9375rem;
    line-height: 1.45;
  }

  :global(.ui-alert-dialog-actions) {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1.125rem;
  }

  /* Same recipe as `ui/button` (DS-00): the Cancel/Action primitives
     here can't compose `UiButton` because ui/button lives on the
     parallel DS-00 branch — keep the visual contract identical so the
     merged tree reads as one system. */
  :global(.ui-alert-dialog-btn) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 2rem;
    padding: 0.3rem 0.8rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    font-size: 0.9375rem;
    cursor: pointer;
    user-select: none;
    transition:
      background-color 140ms var(--ease-out),
      border-color 140ms var(--ease-out),
      color 140ms var(--ease-out),
      transform 140ms var(--ease-out);
  }

  :global(.ui-alert-dialog-btn:hover:not([data-disabled])) {
    background: var(--greyed-bg);
  }

  :global(.ui-alert-dialog-btn:active:not([data-disabled])) {
    transform: scale(0.97);
  }

  :global(.ui-alert-dialog-btn:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  :global(.ui-alert-dialog-btn[data-variant="primary"]) {
    border-color: transparent;
    background: var(--accent);
    color: var(--bg);
  }

  :global(.ui-alert-dialog-btn[data-variant="primary"]:hover:not([data-disabled])) {
    background: color-mix(in srgb, var(--accent) 88%, var(--fg));
  }

  :global(.ui-alert-dialog-btn[data-variant="danger"]) {
    border-color: transparent;
    background: var(--warn);
    color: var(--bg);
  }

  :global(.ui-alert-dialog-btn[data-variant="danger"]:hover:not([data-disabled])) {
    background: color-mix(in srgb, var(--warn) 88%, var(--fg));
  }
</style>
