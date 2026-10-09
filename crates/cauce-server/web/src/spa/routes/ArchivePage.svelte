<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/archive` route shell (FX-05) — `?q=`/`?offset=`, feature state,
  one run per URL.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { spa } from "../lib/i18n.js";
  import { createArchivePage } from "../features/archive/archive.svelte.js";
  import ArchiveView from "../features/archive/ArchiveView.svelte";

  interface ArchivePageProps {
    params: URLSearchParams;
  }

  let { params }: ArchivePageProps = $props();

  const page = createArchivePage();

  onMount(() => {
    void page.run(params);
    document.title = `${spa.archive.page_title} · ${spa.common.brand}`;
  });
</script>

<main>
  <ArchiveView {page} />
</main>
