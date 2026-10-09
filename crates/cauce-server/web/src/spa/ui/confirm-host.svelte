<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `ui/confirm-host` — `<UiConfirmHost>`, the single shared AlertDialog
  that renders `ui/confirm` requests. Mounted once in `app/App.svelte`;
  feature stores only `await confirm({...})`, never mount a dialog.
-->
<script lang="ts">
  import UiAlertDialog from "./alert-dialog.svelte";
  import {
    subscribeConfirm,
    settleConfirm,
    type PendingConfirm,
  } from "./confirm.js";
  import { spa } from "../lib/i18n.js";

  let request = $state<PendingConfirm | null>(null);

  $effect(() => subscribeConfirm((head) => (request = head)));
</script>

<UiAlertDialog
  open={request !== null}
  title={request?.title ?? ""}
  description={request?.description ?? ""}
  confirmLabel={request?.confirmLabel ?? spa.common.confirm}
  cancelLabel={spa.common.cancel}
  danger={request?.danger ?? false}
  onConfirm={() => settleConfirm(true)}
  onCancel={() => settleConfirm(false)}
/>
