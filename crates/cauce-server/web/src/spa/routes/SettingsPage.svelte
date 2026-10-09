<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/settings` route shell (FX-05) — `GET /api/config` once per URL.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { spa } from "../lib/i18n.js";
  import { capabilities } from "../lib/capabilities.svelte.js";
  import { createSettingsPage } from "../features/settings/settings.svelte.js";
  import SettingsView from "../features/settings/SettingsView.svelte";

  const page = createSettingsPage();

  onMount(() => {
    // PUB-03: a BYOK-only visitor sees just the keys section — `GET
    // /api/config` is admin-scoped, so skip the doomed fetch for them.
    if (capabilities.role === "admin") void page.refresh();
    document.title = `${spa.settings.page_title} · ${spa.common.brand}`;
  });
</script>

<main>
  <SettingsView {page} />
</main>
