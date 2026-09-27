/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * W2-08 theme toggle: cycles system -> light -> dark, persisted in
 * localStorage under `cauce-theme` (`system` removes the key so the
 * prefers-color-scheme media query rules again). The head script in
 * theme_head.html applies the stored choice before first paint; this
 * module only wires the button and its state word. Without JS the
 * button inertly shows `theme` and the media query keeps working.
 */
export const THEME_KEY = "cauce-theme";

export type Theme = "system" | "light" | "dark";
const ORDER: Theme[] = ["system", "light", "dark"];

/** The Storage slice the toggle needs (tests pass a fake). */
type ThemeStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

export function storedTheme(storage: ThemeStorage): Theme {
  try {
    const s = storage.getItem(THEME_KEY);
    return s === "light" || s === "dark" ? s : "system";
  } catch {
    return "system";
  }
}

export function nextTheme(state: Theme): Theme {
  return ORDER[(ORDER.indexOf(state) + 1) % ORDER.length];
}

export function initThemeToggle(
  doc: Document = document,
  storage: ThemeStorage = window.localStorage,
): void {
  const btn = doc.getElementById("theme-toggle");
  if (!btn) return;
  const root = doc.documentElement;

  const render = () => {
    const s = storedTheme(storage);
    const label = btn.getAttribute(`data-label-${s}`) || s;
    btn.textContent = label;
    // The accessible name tracks the active state (`theme: dark`);
    // the static aria-label only covers the no-JS render.
    const tpl = btn.getAttribute("data-aria-state");
    if (tpl) btn.setAttribute("aria-label", tpl.replace("{state}", label));
  };

  btn.addEventListener("click", () => {
    const next = nextTheme(storedTheme(storage));
    if (next === "system") {
      delete root.dataset.theme;
      try {
        storage.removeItem(THEME_KEY);
      } catch {
        /* storage may be unavailable */
      }
    } else {
      root.dataset.theme = next;
      try {
        storage.setItem(THEME_KEY, next);
      } catch {
        /* storage may be unavailable */
      }
    }
    render();
  });
  render();
}
