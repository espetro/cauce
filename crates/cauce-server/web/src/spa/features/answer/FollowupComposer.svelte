<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The pinned follow-up composer (§7.1/§7.2): one input pinned to the
  bottom of the thread. While a stream is in flight the input holds
  the submitted question disabled and the submit swaps to `stop`
  (§7.2.2 — interruption is a feature, and the text survives so the
  user can edit + resubmit).
-->
<script lang="ts">
  import { SA } from "../../lib/i18n.js";

  interface ComposerProps {
    value: string;
    busy: boolean;
    onsubmit: (q: string) => void;
    onstop: () => void;
  }

  let { value = $bindable(""), busy, onsubmit, onstop }: ComposerProps = $props();

  function submit(event: SubmitEvent): void {
    event.preventDefault();
    onsubmit(value);
  }
</script>

<form id="answer-followup" onsubmit={submit}>
  <input
    id="followup-q"
    name="q"
    type="text"
    autocomplete="off"
    required
    placeholder={SA.followup_placeholder}
    aria-label={SA.followup_placeholder}
    disabled={busy}
    bind:value
  />
  {#if busy}
    <button type="button" id="answer-stop" onclick={onstop}>{SA.stop}</button>
  {:else}
    <button type="submit" disabled={!value.trim()}>{SA.followup_submit}</button>
  {/if}
</form>
