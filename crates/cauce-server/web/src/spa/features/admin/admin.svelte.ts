/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/app/admin` shell (FX-05, plan §7.4): the three ops surfaces —
 * `engines`, `cache`, `audit` — as tabs under one route. `tab`
 * selects the pane; each tab interprets the rest of the query params
 * (`q`/`offset`/`actor`/`action`) itself. Invalid/missing `tab`
 * falls back to `engines`.
 */

import { createAuditTab } from "./audit.svelte.js";
import { createCacheTab } from "./cache.svelte.js";
import { createEnginesTab } from "./engines.svelte.js";

export type AdminTab = "engines" | "cache" | "audit";

const TABS: AdminTab[] = ["engines", "cache", "audit"];

export function isAdminTab(s: string | null): s is AdminTab {
  return s === "engines" || s === "cache" || s === "audit";
}

export function createAdminPage() {
  const state = $state({
    tab: "engines" as AdminTab,
  });

  const engines = createEnginesTab();
  const cache = createCacheTab();
  const audit = createAuditTab();
  const loaded = new Set<AdminTab>();

  async function run(params: URLSearchParams): Promise<void> {
    const tab = params.get("tab");
    state.tab = isAdminTab(tab) ? tab : "engines";
    if (loaded.has(state.tab)) {
      // Tab already mounted — still re-run so its own params apply.
      if (state.tab === "engines") await engines.refresh();
      else if (state.tab === "cache") await cache.run(params);
      else await audit.run(params);
      return;
    }
    loaded.add(state.tab);
    if (state.tab === "engines") await engines.refresh();
    else if (state.tab === "cache") await cache.run(params);
    else await audit.run(params);
  }

  function href(tab: AdminTab): string {
    return "/app/admin?tab=" + tab;
  }

  return { state, TABS, engines, cache, audit, run, href };
}
