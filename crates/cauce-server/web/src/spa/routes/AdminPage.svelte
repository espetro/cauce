<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin` route shell (FX-05, plan §7.4) — merged ops surface:
  `?tab=engines|cache|audit` + per-tab params, one run per URL.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { spa } from "../lib/i18n.js";
  import { createAdminPage } from "../features/admin/admin.svelte.js";
  import AdminView from "../features/admin/AdminView.svelte";

  interface AdminPageProps {
    params: URLSearchParams;
  }

  let { params }: AdminPageProps = $props();

  const page = createAdminPage();

  onMount(() => {
    void page.run(params).then(() => {
      // `#engine.{id}` anchors from the dashboard survive `navigate`,
      // but the keyed remount resets scroll — re-apply it once the tab
      // has rendered.
      if (location.hash !== "") {
        document.getElementById(location.hash.slice(1))?.scrollIntoView();
      }
    });
    document.title = `${spa.admin.title} · ${spa.common.brand}`;
  });
</script>

<main>
  <AdminView {page} />
</main>
