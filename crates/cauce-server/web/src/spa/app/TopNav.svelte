<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The minimal sticky chrome (UX-06): brand + Search / Answer / History on
  the left, theme toggle + waffle menu on the right. The waffle holds
  everything else (Dashboard, Archive, Engines, Cache, Audit, Settings,
  Docs), gated by `capabilities.flags` and `routeVisible` exactly as the
  old header was. At >= 700px the waffle is a dropdown; below that it is
  a full-screen sheet. Links without an SPA twin stay plain hrefs (full
  load); `/app`-bound links route client-side via App's delegated click.
-->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import {
    SunIcon,
    MoonIcon,
    CircleHalfIcon,
    SquaresFourIcon,
    GithubLogoIcon,
    XIcon,
  } from "phosphor-svelte";
  import { spa } from "../lib/i18n.js";
  import markUrl from "../assets/cauce-mark.png";
  import { capabilities } from "../lib/capabilities.svelte.js";
  import { appHref, routeVisible, route } from "./router.svelte.js";
  import { cycleTheme, theme, themeAria } from "./theme.svelte.js";
  import UiButton from "../ui/button.svelte";
  import UiTooltip from "../ui/tooltip.svelte";
  import UiDropdownMenu, {
    type DropdownItem,
  } from "../ui/dropdown-menu.svelte";

  interface TopNavProps {
    active?: string;
  }

  let { active = "" }: TopNavProps = $props();

  const c = spa.common;

  const DOCS_URL = "https://docs.cauce.fyi";
  const REPO_URL = "https://github.com/espetro/cauce";

  interface WaffleEntry {
    label: string;
    href: string;
    current: boolean;
  }

  let sheetOpen = $state(false);

  // The landing route owns the full logotype (mark + wordmark) above
  // the search bar; the nav drops its own brand there so the identity
  // is not doubled (W9-05).

  const entries: WaffleEntry[] = $derived.by(() => {
    const flags = capabilities.flags;
    const out: WaffleEntry[] = [
      { label: c.nav_dashboard, href: appHref("/dashboard"), current: active === "dashboard" },
    ];
    if (!capabilities.loaded) return out;
    if (flags.archiving) {
      out.push({ label: c.nav_archive, href: appHref("/archive"), current: active === "archive" });
    }
    if (flags.adminSurface) {
      out.push(
        { label: c.nav_engines, href: appHref("/admin?tab=engines"), current: active === "engines" },
        { label: c.nav_cache, href: appHref("/admin?tab=cache"), current: active === "cache" },
        { label: c.nav_audit, href: appHref("/admin?tab=audit"), current: active === "audit" },
      );
    }
    if (
      routeVisible(
        "/settings",
        flags.adminSurface,
        flags.archiving,
        flags.allowUserKeys || flags.allowUserBaseUrl,
      )
    ) {
      out.push({ label: c.nav_settings, href: appHref("/settings"), current: active === "settings" });
    }
    out.push({ label: c.nav_docs, href: DOCS_URL, current: false });
    return out;
  });

  const menuItems: DropdownItem[] = $derived(
    entries.map((e): DropdownItem => ({ type: "link", ...e })),
  );
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && (sheetOpen = false)} />

<header class="site">
  {#if route.path !== "/"}
    <a class="brand" href={appHref("/")}><img class="brand-mark" src={markUrl} alt="" />{c.brand}</a>
  {/if}
  <nav class="nav-primary" aria-label={c.nav_primary_label}>
    <a href={appHref("/")} aria-current={active === "search" ? "page" : undefined}
      >{c.nav_search}</a
    >
    <a href={appHref("/history")} aria-current={active === "history" ? "page" : undefined}>{c.nav_history}</a>
  </nav>
  <div class="nav-right">
    <UiTooltip content={c.github_label} side="bottom">
      {#snippet trigger(props)}
        <UiButton
          {...(props as ComponentProps<typeof UiButton>)}
          variant="default"
          size="icon"
          ariaLabel={c.github_label}
          href={REPO_URL}
        >
          <GithubLogoIcon size={15} aria-hidden="true" />
        </UiButton>
      {/snippet}
    </UiTooltip>
    <!-- justified: the theme control cycles three states
      (system → light → dark), so it is not a binary `Toggle` and a
      `ToggleGroup` of one makes no sense either — it stays a single
      `UiButton` with a `UiTooltip` (plan §3.7). The icon mirrors the
      active state; the tooltip carries the `theme_switch` label the
      old `title=` attribute had. -->
    <UiTooltip content={c.theme_switch} side="bottom">
      {#snippet trigger(props)}
        <UiButton
          {...(props as ComponentProps<typeof UiButton>)}
          variant="default"
          size="icon"
          ariaLabel={themeAria()}
          onclick={cycleTheme}
        >
          {#if theme.value === "light"}
            <SunIcon size={15} aria-hidden="true" />
          {:else if theme.value === "dark"}
            <MoonIcon size={15} aria-hidden="true" />
          {:else}
            <CircleHalfIcon size={15} aria-hidden="true" />
          {/if}
        </UiButton>
      {/snippet}
    </UiTooltip>
    <span class="waffle-menu">
      <UiDropdownMenu items={menuItems} ariaLabel={c.nav_menu_label}>
        {#snippet trigger(props)}
          <UiButton
            {...(props as ComponentProps<typeof UiButton>)}
            variant="default"
            size="icon"
            ariaLabel={c.nav_menu_label}
          >
            <SquaresFourIcon size={15} aria-hidden="true" />
          </UiButton>
        {/snippet}
      </UiDropdownMenu>
    </span>
    <span class="waffle-sheet-trigger">
      <UiButton
        variant="default"
        size="icon"
        ariaLabel={c.nav_menu_label}
        onclick={() => (sheetOpen = true)}
      >
        <SquaresFourIcon size={15} aria-hidden="true" />
      </UiButton>
    </span>
  </div>
</header>

{#if sheetOpen}
  <div class="waffle-sheet" role="dialog" aria-modal="true" aria-label={c.nav_menu_label}>
    <div class="waffle-sheet-bar">
      <UiButton
        variant="default"
        size="icon"
        ariaLabel={c.nav_menu_close}
        onclick={() => (sheetOpen = false)}
      >
        <XIcon size={15} aria-hidden="true" />
      </UiButton>
    </div>
    <!-- Link clicks bubble here: close the sheet, let the anchor (or
      App's delegated router) navigate. -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <nav class="waffle-sheet-links" aria-label={c.nav_menu_label} onclick={() => (sheetOpen = false)}>
      {#each entries as e (e.href)}
        <a href={e.href} aria-current={e.current ? "page" : undefined}>{e.label}</a>
      {/each}
    </nav>
  </div>
{/if}

<style>
  /* Styles live in TopNav.css — imported so the build resolves
     `@apply` while Svelte's scoping semantics are unchanged. */
  @import "./TopNav.css";
</style>
