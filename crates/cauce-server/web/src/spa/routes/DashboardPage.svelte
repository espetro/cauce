<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/dashboard` route shell (FX-05) — `?days=7|30`, feature state,
  one run per URL.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { spa } from "../lib/i18n.js";
  import { createDashboardPage } from "../features/dashboard/dashboard.svelte.js";
  import DashboardView from "../features/dashboard/DashboardView.svelte";

  interface DashboardPageProps {
    params: URLSearchParams;
  }

  let { params }: DashboardPageProps = $props();

  const page = createDashboardPage();

  onMount(() => {
    void page.run(params);
    document.title = `${spa.dashboard.title} · ${spa.common.brand}`;
  });
</script>

<main>
  <DashboardView {page} />
</main>
