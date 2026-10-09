<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The SPA shell (FX-03): sticky chrome + the routed page. `/app/*` clicks
  navigate client-side; every other link hands off to the HTMX pages.
  `{#key}` remounts the outlet per navigation so each route shell owns a
  fresh feature state (the SSR pages' per-request semantics).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { route, navigate, onPopState } from "./router.svelte.js";
  import { applyTheme } from "./theme.svelte.js";
  import { loadCapabilities } from "../lib/capabilities.svelte.js";
  import { spa } from "../lib/i18n.js";
  import TopNav from "./TopNav.svelte";
  import HomePage from "../routes/HomePage.svelte";
  import SearchPage from "../routes/SearchPage.svelte";

  onMount(() => {
    applyTheme();
    void loadCapabilities();
    const onClick = (event: MouseEvent) => {
      if (
        event.defaultPrevented ||
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey
      ) {
        return;
      }
      const a =
        event.target instanceof Element ? event.target.closest("a[href]") : null;
      if (!(a instanceof HTMLAnchorElement)) return;
      if (a.target || a.hasAttribute("download") || a.rel === "external") return;
      const url = new URL(a.href, location.origin);
      if (url.origin !== location.origin || !url.pathname.startsWith("/app")) return;
      event.preventDefault();
      navigate(url.pathname + url.search + url.hash);
    };
    document.addEventListener("click", onClick);
    window.addEventListener("popstate", onPopState);
    return () => {
      document.removeEventListener("click", onClick);
      window.removeEventListener("popstate", onPopState);
    };
  });

  const active = $derived(route.path === "/search" ? "search" : "");
  const routeKey = $derived(route.path + "?" + route.params.toString());
</script>

<TopNav {active} />
{#key routeKey}
  {#if route.path === "/"}
    <HomePage />
  {:else if route.path === "/search"}
    <SearchPage params={route.params} />
  {:else}
    <main>
      <p>{spa.app.not_found}</p>
      <p>
        <a href={location.pathname.replace(/^\/app/, "") || "/"}>{spa.app.open_html}</a>
      </p>
    </main>
  {/if}
{/key}
