// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it, vi } from "vitest";
import { initAiModePill, initIndexBeacon, initSearchForm } from "../src/search.js";

function clickNoNav(el) {
  el.addEventListener("click", (e) => e.preventDefault(), { once: true });
  el.click();
}

function searchShell({ aiPill = true, indexOnClick = false } = {}) {
  document.body.innerHTML = `
    <body ${indexOnClick ? 'data-index-on-click data-query-hash="hash123"' : ""}>
    <form id="search-form" action="/search" method="get">
      <input type="search" name="q" value="">
      ${
        aiPill
          ? '<button type="button" id="ai-mode" class="mode-pill" aria-pressed="false" data-placeholder="Ask anything" data-submit="Ask">AI mode</button>'
          : ""
      }
      <button type="submit">Search</button>
    </form>
    <div id="results"></div>
    </body>`;
  const form = document.getElementById("search-form");
  return { form, input: form.querySelector('input[name="q"]') };
}

function fakeLocation(origin = "http://localhost:4479") {
  return { origin, assign: vi.fn() };
}

function submit(form) {
  const event = new Event("submit", { cancelable: true, bubbles: true });
  form.dispatchEvent(event);
  return event;
}

describe("initSearchForm", () => {
  it("redirects to /search?q=…&stream=1 on submit", () => {
    const { form, input } = searchShell();
    const loc = fakeLocation();
    initSearchForm(form, loc);
    input.value = "tokyo weather";
    const event = submit(form);
    expect(event.defaultPrevented).toBe(true);
    expect(loc.assign).toHaveBeenCalledWith("/search?q=tokyo+weather&stream=1");
  });

  it("does not intercept an empty query", () => {
    const { form, input } = searchShell();
    const loc = fakeLocation();
    initSearchForm(form, loc);
    input.value = "   ";
    const event = submit(form);
    expect(event.defaultPrevented).toBe(false);
    expect(loc.assign).not.toHaveBeenCalled();
  });
});

describe("initAiModePill — assist context collection", () => {
  it("collects placeholder/submit strings from data attributes and toggles", () => {
    const { form, input } = searchShell();
    initAiModePill(form);
    const pill = document.getElementById("ai-mode");
    const submitBtn = form.querySelector('button[type="submit"]');

    pill.click();
    expect(form.dataset.mode).toBe("ai");
    expect(pill.getAttribute("aria-pressed")).toBe("true");
    expect(input.placeholder).toBe("Ask anything");
    expect(submitBtn.textContent).toBe("Ask");

    pill.click();
    expect(form.dataset.mode).toBe("search");
    expect(pill.getAttribute("aria-pressed")).toBe("false");
    expect(input.placeholder).toBe("");
    expect(submitBtn.textContent).toBe("Search");
  });

  it("routes submit to /answer?q= while AI mode is armed", () => {
    const { form, input } = searchShell();
    const loc = fakeLocation();
    initAiModePill(form);
    initSearchForm(form, loc);
    document.getElementById("ai-mode").click();
    input.value = "why rust";
    submit(form);
    expect(loc.assign).toHaveBeenCalledWith("/answer?q=why%20rust");
  });

  it("is inert without the pill element", () => {
    const { form, input } = searchShell({ aiPill: false });
    const loc = fakeLocation();
    initAiModePill(form);
    initSearchForm(form, loc);
    input.value = "x";
    submit(form);
    expect(loc.assign).toHaveBeenCalledWith("/search?q=x&stream=1");
  });
});

describe("initIndexBeacon", () => {
  it("posts /api/pages with url + query_hash on result clicks", () => {
    const fetchImpl = vi.fn(() => Promise.resolve({ ok: true }));
    document.body.innerHTML = "";
    document.body.setAttribute("data-index-on-click", "");
    document.body.dataset.queryHash = "hash123";
    const results = document.createElement("div");
    results.id = "results";
    results.innerHTML = `<a href="https://example.com/x">t</a>`;
    document.body.appendChild(results);
    initIndexBeacon(document, fetchImpl);
    clickNoNav(results.querySelector("a"));
    expect(fetchImpl).toHaveBeenCalledWith(
      "/api/pages",
      expect.objectContaining({
        method: "POST",
        keepalive: true,
        body: JSON.stringify({ url: "https://example.com/x", query_hash: "hash123" }),
      }),
    );
  });

  it("omits query_hash when empty and ignores non-http links", () => {
    const fetchImpl = vi.fn(() => Promise.resolve({ ok: true }));
    document.body.innerHTML = "";
    document.body.setAttribute("data-index-on-click", "");
    delete document.body.dataset.queryHash;
    const results = document.createElement("div");
    results.id = "results";
    results.innerHTML = `<a href="https://a.com/">a</a><a href="mailto:x@y.z">m</a>`;
    document.body.appendChild(results);
    initIndexBeacon(document, fetchImpl);
    clickNoNav(results.querySelectorAll("a")[1]);
    expect(fetchImpl).not.toHaveBeenCalled();
    clickNoNav(results.querySelectorAll("a")[0]);
    expect(JSON.parse(fetchImpl.mock.calls[0][1].body)).toEqual({ url: "https://a.com/" });
  });

  it("swallows beacon rejections so the click flow never breaks", async () => {
    const fetchImpl = vi.fn(() => Promise.reject(new Error("offline")));
    document.body.innerHTML = "";
    document.body.setAttribute("data-index-on-click", "");
    const results = document.createElement("div");
    results.id = "results";
    results.innerHTML = `<a href="https://a.com/">a</a>`;
    document.body.appendChild(results);
    initIndexBeacon(document, fetchImpl);
    clickNoNav(results.querySelector("a"));
    await vi.waitFor(() => expect(fetchImpl).toHaveBeenCalled()); // no unhandled rejection
  });

  it("is inert without data-index-on-click", () => {
    const fetchImpl = vi.fn();
    document.body.innerHTML = "";
    initIndexBeacon(document, fetchImpl);
    document.body.click();
    expect(fetchImpl).not.toHaveBeenCalled();
  });
});
