<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The SPA shell (FX-03): sticky chrome + the routed page. `/app/*` clicks
  navigate client-side; every other link hands off to the residual
  server-rendered pages (`/trace/{id}`, `/answer/{id}`) or redirects.
  `{#key}` remounts the outlet per navigation so each route shell owns a
  fresh feature state (the SSR pages' per-request semantics).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { route, navigate, onPopState, routeVisible, ROUTE_REQUIRES } from "./router.svelte.js";
  import { applyTheme } from "./theme.svelte.js";
  import { capabilities, loadCapabilities } from "../lib/capabilities.svelte.js";
  import { spa } from "../lib/i18n.js";
  import TopNav from "./TopNav.svelte";
  import GateBlock from "./GateBlock.svelte";
  import HomePage from "../routes/HomePage.svelte";
  import SearchPage from "../routes/SearchPage.svelte";
  import AnswerPage from "../routes/AnswerPage.svelte";
  import HistoryPage from "../routes/HistoryPage.svelte";
  import DashboardPage from "../routes/DashboardPage.svelte";
  import SettingsPage from "../routes/SettingsPage.svelte";
  import ArchivePage from "../routes/ArchivePage.svelte";
  import AdminPage from "../routes/AdminPage.svelte";

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

  const active = $derived.by(() => {
    switch (route.path) {
      case "/search":
        return "search";
      case "/answer":
        return "answer";
      case "/history":
        return "history";
      case "/dashboard":
        return "dashboard";
      case "/archive":
        return "archive";
      case "/settings":
        return "settings";
      case "/admin":
        return route.params.get("tab") ?? "engines";
      default:
        return "";
    }
  });
  const routeKey = $derived(route.path + "?" + route.params.toString());
</script>

<TopNav {active} />
{#key routeKey}
  {#if !routeVisible(route.path, capabilities.loaded && capabilities.flags.adminSurface, capabilities.loaded && capabilities.flags.archiving, capabilities.flags.allowUserKeys || capabilities.flags.allowUserBaseUrl)}
    <GateBlock requires={ROUTE_REQUIRES[route.path]} />
  {:else if route.path === "/"}
    <HomePage />
  {:else if route.path === "/search"}
    <SearchPage params={route.params} />
  {:else if route.path === "/answer"}
    <AnswerPage params={route.params} />
  {:else if route.path === "/history"}
    <HistoryPage params={route.params} />
  {:else if route.path === "/dashboard"}
    <DashboardPage params={route.params} />
  {:else if route.path === "/settings"}
    <SettingsPage />
  {:else if route.path === "/archive"}
    <ArchivePage params={route.params} />
  {:else if route.path === "/admin"}
    <AdminPage params={route.params} />
  {:else}
    <main>
      <p>{spa.app.not_found}</p>
      <p>
        <a href={location.pathname.replace(/^\/app/, "") || "/"}>{spa.app.open_html}</a>
      </p>
    </main>
  {/if}
{/key}
