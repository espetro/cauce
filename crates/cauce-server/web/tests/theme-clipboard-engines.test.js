// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it, vi } from "vitest";
import { initThemeToggle, nextTheme, storedTheme } from "../src/theme.js";
import { initEngineErrors } from "../src/engines.js";
import { initHashDetails, initHistoryCopy } from "../src/clipboard.js";

function clickNoNav(el) {
  el.addEventListener("click", (e) => e.preventDefault(), { once: true });
  el.click();
}

function fakeStorage() {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
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
    const btn = document.getElementById("theme-toggle");

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

describe("engines responseError slot", () => {
  it("writes the status into the card's .test-results slot", () => {
    document.body.innerHTML = "";
    document.body.dataset.i18nTestFailed = "fetch failed ({status})";
    document.body.dataset.i18nRequestFailed = "request failed ({status})";
    const card = document.createElement("article");
    card.className = "engine-card";
    card.innerHTML = `<div class="test-results"></div><form></form><button>x</button>`;
    document.body.appendChild(card);
    initEngineErrors(document);

    const fire = (elt, status) =>
      document.body.dispatchEvent(
        new CustomEvent("htmx:responseError", {
          bubbles: true,
          detail: { elt, xhr: { status } },
        }),
      );
    fire(card.querySelector("form"), 502);
    expect(card.querySelector(".test-results").textContent).toBe("fetch failed (502)");
    fire(card.querySelector("button"), 404);
    expect(card.querySelector(".test-results").textContent).toBe("request failed (404)");
  });

  it("no-ops on pages without the i18n attributes", () => {
    document.body.innerHTML = "";
    delete document.body.dataset.i18nTestFailed;
    expect(() => initEngineErrors(document)).not.toThrow();
  });
});

describe("history copy-json", () => {
  it("copies the fetched payload and flashes the label", async () => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
    document.body.dataset.page = "history";
    document.body.dataset.copiedLabel = "copied";
    const td = document.createElement("td");
    td.innerHTML = `<a href="/api/search?q=x" class="copy-json">copy json</a><code class="copy-fallback" hidden></code>`;
    document.body.appendChild(td);
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.resolve({ ok: true, text: () => Promise.resolve('{"a":1}') })),
    );
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(window.navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    initHistoryCopy(document);

    clickNoNav(td.querySelector("a"));
    await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('{"a":1}'));
    const a = td.querySelector("a");
    expect(a.textContent).toBe("copied");
    vi.advanceTimersByTime(1600);
    expect(a.textContent).toBe("copy json");
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("reveals the .copy-fallback code with the URL on failure", async () => {
    document.body.innerHTML = "";
    document.body.dataset.page = "history";
    document.body.dataset.copiedLabel = "copied";
    const td = document.createElement("td");
    td.innerHTML = `<a href="/api/search?q=x" class="copy-json">copy json</a><code class="copy-fallback" hidden></code>`;
    document.body.appendChild(td);
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve({ ok: false, status: 500 })));
    Object.defineProperty(window.navigator, "clipboard", {
      value: { writeText: vi.fn() },
      configurable: true,
    });
    initHistoryCopy(document);

    clickNoNav(td.querySelector("a"));
    const code = td.querySelector(".copy-fallback");
    await vi.waitFor(() => expect(code.hidden).toBe(false));
    expect(code.textContent).toContain("/api/search?q=x");
    vi.unstubAllGlobals();
  });
});

describe("cache hash details", () => {
  it("opens the <details> named by location.hash on load", () => {
    document.body.innerHTML = "";
    document.body.className = "cache-page";
    document.body.innerHTML = `<details id="row-1"><summary>s</summary></details>`;
    const win = {
      location: { hash: "#row-1" },
      listeners: new Map(),
      addEventListener(n, f) {
        this.listeners.set(n, f);
      },
    };
    initHashDetails(win, document);
    win.listeners.get("load")();
    expect(document.getElementById("row-1").open).toBe(true);
  });

  it("no-ops off the cache page", () => {
    document.body.innerHTML = "";
    document.body.className = "";
    const win = { addEventListener: vi.fn(), location: { hash: "#x" } };
    initHashDetails(win, document);
    expect(win.addEventListener).not.toHaveBeenCalled();
  });
});
