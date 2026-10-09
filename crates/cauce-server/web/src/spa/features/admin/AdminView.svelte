<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  `/app/admin` — plan §7.4 merged ops surface: one route, three tabs
  (`engines`, `cache`, `audit`), each a self-contained feature slice.
  The tab bar is `ui/tabs` (Bits UI Tabs) with the URL as source of
  truth: `value` mirrors `?tab=`, and `onValueChange` goes through the
  same `navigate()` the old anchors' delegated handler used, so deep
  links stay shareable and the hash stays free for engine-card
  anchors. Manual activation — focus moves on arrows, Enter/Space
  navigates. Panes render via the route as before (no Tabs.Content).
-->
<script lang="ts">
  import { navigate } from "../../app/router.svelte.js";
  import { spa } from "../../lib/i18n.js";
  import UiTabs from "../../ui/tabs.svelte";
  import AuditTab from "./AuditTab.svelte";
  import CacheTab from "./CacheTab.svelte";
  import EnginesTab from "./EnginesTab.svelte";
  import type { AdminTab, createAdminPage } from "./admin.svelte.js";

  interface AdminViewProps {
    page: ReturnType<typeof createAdminPage>;
  }

  let { page }: AdminViewProps = $props();
  const s = $derived(page.state);
  const tabs = $derived(
    page.TABS.map((t) => ({
      value: t,
      label: spa.admin["tab_" + t as keyof typeof spa.admin],
    })),
  );
</script>

<h1>{spa.admin.title}</h1>
<UiTabs
  value={s.tab}
  {tabs}
  ariaLabel={spa.admin.tabs_label}
  onValueChange={(v) => navigate(page.href(v as AdminTab))}
/>

{#if s.tab === "engines"}
  <EnginesTab tab={page.engines} />
{:else if s.tab === "cache"}
  <CacheTab tab={page.cache} />
{:else}
  <AuditTab tab={page.audit} />
{/if}
