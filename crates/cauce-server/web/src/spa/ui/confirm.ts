/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `ui/confirm` — promise-based replace for `window.confirm`, backed by
 * the single `UiAlertDialog` that `UiConfirmHost` mounts in `App.svelte`.
 *
 * Call sites (the feature stores) `await confirm({...})`; the request
 * queues FIFO and resolves `true` on the action button, `false` on
 * cancel/Escape. `window.confirm` was synchronous so overlapping calls
 * were impossible — the queue keeps that contract sane if two ever do
 * overlap.
 *
 * This file stays plain `.ts` (no runes) so feature stores can import
 * it without a Svelte-file dependency; `UiConfirmHost` pulls the queue
 * through `subscribeConfirm`/`settleConfirm` into its own `$state`.
 */

export interface ConfirmOptions {
  title: string;
  description: string;
  /** Defaults to `spa.common.confirm` in the host. */
  confirmLabel?: string;
  /** Renders the action with the warn fill — destructive ops. */
  danger?: boolean;
}

export interface PendingConfirm extends ConfirmOptions {
  resolve: (ok: boolean) => void;
}

interface ConfirmListener {
  (head: PendingConfirm | null): void;
}

let queue: PendingConfirm[] = [];
const listeners = new Set<ConfirmListener>();

function emit(): void {
  const head = queue[0] ?? null;
  for (const listener of listeners) listener(head);
}

export function confirm(options: ConfirmOptions): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    queue.push({ ...options, resolve });
    emit();
  });
}

/** UiConfirmHost subscribes here; returns an unsubscribe function. */
export function subscribeConfirm(listener: ConfirmListener): () => void {
  listeners.add(listener);
  listener(queue[0] ?? null);
  return () => {
    listeners.delete(listener);
  };
}

/** UiConfirmHost resolves the head request and advances the queue. */
export function settleConfirm(ok: boolean): void {
  const head = queue.shift();
  if (head) head.resolve(ok);
  emit();
}
