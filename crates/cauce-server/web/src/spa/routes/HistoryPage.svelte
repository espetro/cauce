<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/history` route shell (FX-05) — owns the feature state, runs the
  query once per URL (the `{#key}` remount gives per-request semantics).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { spa } from "../lib/i18n.js";
  import { createHistoryPage } from "../features/history/history.svelte.js";
  import HistoryView from "../features/history/HistoryView.svelte";

  interface HistoryPageProps {
    params: URLSearchParams;
  }

  let { params }: HistoryPageProps = $props();

  const page = createHistoryPage();

  onMount(() => {
    void page.run(params);
    document.title = `${spa.history.title} · ${spa.common.brand}`;
  });
</script>

<main>
  <HistoryView {page} />
</main>
