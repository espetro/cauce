<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/settings` route shell (FX-05) — `GET /api/config` once per URL.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { spa } from "../lib/i18n.js";
  import { createSettingsPage } from "../features/settings/settings.svelte.js";
  import SettingsView from "../features/settings/SettingsView.svelte";

  const page = createSettingsPage();

  onMount(() => {
    void page.refresh();
    document.title = `${spa.settings.page_title} · ${spa.common.brand}`;
  });
</script>

<main>
  <SettingsView {page} />
</main>
