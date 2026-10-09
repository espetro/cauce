/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * Theme toggle, ported from `web/src/theme.ts`: cycles
 * system -> light -> dark, persisted under `cauce-theme` (`system`
 * removes the key so `prefers-color-scheme` rules again). The inline
 * script in `index.html` applies the stored choice before first paint;
 * this store owns the button's state word and dataset writes.
 */

import { spa } from "../lib/i18n.js";

export type Theme = "system" | "light" | "dark";
const ORDER: Theme[] = ["system", "light", "dark"];

const THEME_KEY = "cauce-theme";

function stored(): Theme {
  try {
    const s = localStorage.getItem(THEME_KEY);
    return s === "light" || s === "dark" ? s : "system";
  } catch {
    return "system";
  }
}

export const theme = $state({ value: stored() });

/** Write `document.documentElement.dataset.theme` ("" while `system`). */
export function applyTheme(): void {
  document.documentElement.dataset.theme = theme.value === "system" ? "" : theme.value;
}

export function cycleTheme(): void {
  const i = ORDER.indexOf(theme.value);
  theme.value = ORDER[(i + 1) % ORDER.length];
  applyTheme();
  try {
    if (theme.value === "system") {
      localStorage.removeItem(THEME_KEY);
    } else {
      localStorage.setItem(THEME_KEY, theme.value);
    }
  } catch {
    /* private mode: theme stays session-local */
  }
}

/** The state word shown on the toggle (`system`/`light`/`dark` copy). */
export function themeWord(): string {
  const key = ("theme_" + theme.value) as keyof typeof spa.common;
  return spa.common[key] ?? theme.value;
}

/** `aria-label` announcing the active state (`theme: dark`). */
export function themeAria(): string {
  return spa.common.theme_aria_state.replace("{state}", themeWord());
}
