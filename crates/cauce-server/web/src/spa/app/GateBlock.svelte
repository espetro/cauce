<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  FX-07: the notice the central route filter renders when a route's
  `requires` isn't met. For `admin` routes it carries the token form —
  pasting a valid bearer token flips `role` to `admin` on the next
  capabilities refetch and the gate lifts. `archiving` routes just get
  the disabled line.
-->
<script lang="ts">
  import { spa } from "../lib/i18n.js";
  import { capabilities, reloadCapabilities } from "../lib/capabilities.svelte.js";
  import { adminToken, setAdminToken } from "../lib/admin.svelte.js";
  import type { RouteRequirement } from "./router.svelte.js";
  import UiButton from "../ui/button.svelte";

  interface GateBlockProps {
    requires: RouteRequirement;
  }

  let { requires }: GateBlockProps = $props();

  let draft = $state(adminToken.value);

  async function save() {
    setAdminToken(draft);
    draft = adminToken.value;
    await reloadCapabilities();
  }
</script>

<main>
  {#if requires === "admin" || requires === "admin_or_byok"}
    <h1>{spa.app.admin_gate_title}</h1>
    <p>{spa.app.admin_gate_note}</p>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <label for="admin-token">{spa.app.admin_token_label}</label>
      <input
        id="admin-token"
        type="password"
        autocomplete="off"
        bind:value={draft}
      />
      <UiButton type="submit" variant="primary">{spa.app.admin_token_save}</UiButton>
      {#if adminToken.value !== ""}
        <UiButton
          variant="default"
          onclick={() => {
            setAdminToken("");
            draft = "";
            void reloadCapabilities();
          }}>{spa.app.admin_token_clear}</UiButton
        >
      {/if}
      <p class="meta">{spa.app.admin_token_hint}</p>
    </form>
  {:else}
    <h1>{spa.app.not_found}</h1>
    <p>{spa.app.archiving_off}</p>
  {/if}
</main>
