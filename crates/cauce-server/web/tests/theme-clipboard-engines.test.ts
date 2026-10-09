// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it, vi } from "vitest";
import { initThemeToggle, nextTheme, storedTheme } from "../src/theme.js";

function clickNoNav(el: HTMLElement) {
  el.addEventListener("click", (e) => e.preventDefault(), { once: true });
  el.click();
}

function fakeStorage() {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => (map.has(k) ? map.get(k)! : null),
    setItem: (k: string, v: string) => {
      map.set(k, String(v));
    },
    removeItem: (k: string) => {
      map.delete(k);
    },
    map,
  };
}

describe("theme toggle", () => {
  it("cycles system -> light -> dark -> system and persists", () => {
    document.documentElement.innerHTML = "";
    document.body.innerHTML = `<button id="theme-toggle"
      data-label-system="theme" data-label-light="light" data-label-dark="dark"
      data-aria-state="theme: {state}">theme</button>`;
    const storage = fakeStorage();
    initThemeToggle(document, storage);
    const btn = document.getElementById("theme-toggle")!;

    expect(storedTheme(storage)).toBe("system");
    expect(btn.textContent).toBe("theme");
    expect(btn.getAttribute("aria-label")).toBe("theme: theme");

    btn.click();
    expect(storage.getItem("cauce-theme")).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(btn.textContent).toBe("light");

    btn.click();
    expect(document.documentElement.dataset.theme).toBe("dark");

    btn.click(); // back to system: key removed, data-theme cleared
    expect(storage.getItem("cauce-theme")).toBeNull();
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("nextTheme order", () => {
    expect(nextTheme("system")).toBe("light");
    expect(nextTheme("light")).toBe("dark");
    expect(nextTheme("dark")).toBe("system");
  });
});

