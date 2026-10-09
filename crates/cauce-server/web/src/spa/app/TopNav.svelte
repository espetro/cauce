<!--
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.

  The variant-E sticky chrome (§7.1): same header on every surface —
  brand + primary nav left, quiet operator group + settings + theme
  toggle right, operator links collapsing into a `more` dropdown below
  700px (the shape the old `templates/header.html` had). Links without
  an SPA twin stay plain `/...` hrefs (full load); `/app`-bound links —
  including the menu's link items — route client-side via App's
  delegated click.
-->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import { spa } from "../lib/i18n.js";
  import { capabilities } from "../lib/capabilities.svelte.js";
  import { appHref, routeVisible } from "./router.svelte.js";
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

  const moreItems: DropdownItem[] = $derived([
    {
      type: "link",
      label: c.nav_engines,
      href: appHref("/admin?tab=engines"),
      current: active === "engines",
    },
    {
      type: "link",
      label: c.nav_cache,
      href: appHref("/admin?tab=cache"),
      current: active === "cache",
    },
    {
      type: "link",
      label: c.nav_audit,
      href: appHref("/admin?tab=audit"),
      current: active === "audit",
    },
  ]);
</script>

<header class="site">
  <a class="brand" href={appHref("/")}>{c.brand}</a>
  <nav class="nav-primary" aria-label={c.nav_primary_label}>
    <a href={appHref("/")} aria-current={active === "search" ? "page" : undefined}
      >{c.nav_search}</a
    >
    {#if capabilities.aiEnabled}
      <a href={appHref("/answer")} aria-current={active === "answer" ? "page" : undefined}
        >{c.nav_answer}</a
      >
    {/if}
    <a href={appHref("/history")} aria-current={active === "history" ? "page" : undefined}>{c.nav_history}</a>
    <a href={appHref("/dashboard")} aria-current={active === "dashboard" ? "page" : undefined}>{c.nav_dashboard}</a>
    {#if capabilities.loaded && capabilities.flags.archiving}
      <a href={appHref("/archive")} aria-current={active === "archive" ? "page" : undefined}>{c.nav_archive}</a>
    {/if}
  </nav>
  <div class="nav-right">
    {#if capabilities.loaded && capabilities.flags.adminSurface}
      <nav class="nav-operator" aria-label={c.nav_operator_label}>
        <a href={appHref("/admin?tab=engines")} aria-current={active === "engines" ? "page" : undefined}>{c.nav_engines}</a>
        <a href={appHref("/admin?tab=cache")} aria-current={active === "cache" ? "page" : undefined}>{c.nav_cache}</a>
        <a href={appHref("/admin?tab=audit")} aria-current={active === "audit" ? "page" : undefined}>{c.nav_audit}</a>
      </nav>
    {/if}
    {#if capabilities.loaded && routeVisible("/settings", capabilities.flags.adminSurface, capabilities.flags.archiving, capabilities.flags.allowUserKeys || capabilities.flags.allowUserBaseUrl)}
      <a href={appHref("/settings")} aria-current={active === "settings" ? "page" : undefined}>{c.nav_settings}</a>
    {/if}
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
            <svg
              viewBox="0 0 16 16"
              width="15"
              height="15"
              fill="none"
              stroke="currentColor"
              stroke-width="1.4"
              stroke-linecap="round"
              aria-hidden="true"
            >
              <circle cx="8" cy="8" r="3" />
              <path
                d="M8 1.4v1.7M8 12.9v1.7M1.4 8h1.7M12.9 8h1.7M3.3 3.3l1.2 1.2M11.5 11.5l1.2 1.2M12.7 3.3l-1.2 1.2M4.5 11.5l-1.2 1.2"
              />
            </svg>
          {:else if theme.value === "dark"}
            <svg
              viewBox="0 0 16 16"
              width="15"
              height="15"
              fill="none"
              stroke="currentColor"
              stroke-width="1.4"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M13.4 9.4A5.6 5.6 0 1 1 6.6 2.6a4.5 4.5 0 0 0 6.8 6.8Z" />
            </svg>
          {:else}
            <svg
              viewBox="0 0 16 16"
              width="15"
              height="15"
              fill="none"
              stroke="currentColor"
              stroke-width="1.4"
              aria-hidden="true"
            >
              <circle cx="8" cy="8" r="5.4" />
              <path d="M8 2.6a5.4 5.4 0 0 1 0 10.8Z" fill="currentColor" stroke="none" />
            </svg>
          {/if}
        </UiButton>
      {/snippet}
    </UiTooltip>
    {#if capabilities.loaded && capabilities.flags.adminSurface}
      <span class="nav-more">
        <UiDropdownMenu items={moreItems} ariaLabel={c.nav_operator_label}>
          {#snippet trigger(props)}
            <UiButton
              {...(props as ComponentProps<typeof UiButton>)}
              variant="ghost"
              size="sm">{c.nav_more}</UiButton
            >
          {/snippet}
        </UiDropdownMenu>
      </span>
    {/if}
  </div>
</header>

<style>
  .site {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.35rem 1rem;
    margin-bottom: 1rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
    font-size: 0.9375rem;
    transition: background-color 160ms ease;
  }

  @media (prefers-reduced-motion: reduce) {
    .site {
      transition: none;
    }
  }

  .site a {
    color: var(--fg);
    text-decoration: none;
  }

  .site a:hover {
    text-decoration: underline;
  }

  .brand {
    font-weight: 700;
  }

  .nav-primary,
  .nav-right {
    display: flex;
    align-items: baseline;
    gap: 0.9rem;
  }

  .nav-right {
    margin-left: auto;
    color: var(--muted);
  }

  .nav-right a {
    color: var(--muted);
  }

  .site a[aria-current="page"] {
    color: var(--accent);
  }

  .nav-operator {
    display: none;
    align-items: baseline;
    gap: 0.9rem;
  }

  /* ≥36px hit area on the mobile nav row (plan §1.3). */
  @media (width < 700px) {
    .nav-right :global(.ui-button[data-size="icon"]) {
      width: 2.25rem;
      height: 2.25rem;
      min-height: 2.25rem;
    }
  }

  /* The open menu trigger takes the accent, like the old open
     disclosure did. */
  .nav-more :global(.ui-button[data-state="open"]) {
    color: var(--accent);
  }

  /* CSS-only collapse: operator links move into `more` below 700px. */
  @media (width >= 700px) {
    .nav-operator {
      display: flex;
    }

    .nav-more {
      display: none;
    }
  }
</style>
