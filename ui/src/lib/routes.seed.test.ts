// Own file on purpose: routes.ts seeds the router from location at module
// load, and bun caches the ES module per test file. Isolating this file gives
// the seed a pristine module registry and URL.
import { describe, expect, test } from "bun:test";
import { Window } from "happy-dom";

const url = "http://localhost:4479/search?q=python%20asyncio&mode=ai";
const win = new Window({ url });
globalThis.window = win as never;
globalThis.location = win.location as never;
globalThis.history = win.history as never;
globalThis.document = win.document as never;

const { router } = await import("./routes");

describe("module-load seed (hydration path)", () => {
  test("store is populated before first subscribe, no 404 flash", () => {
    // Mirrors main.tsx: hydrate() renders against router.get() BEFORE any
    // component subscribes (nanostores' own onMount seed runs post-render,
    // too late to match the prerendered shell).
    const page = router.get();
    expect(page?.route).toBe("search");
    expect(page?.search["q"]).toBe("python asyncio");
    expect(page?.search["mode"]).toBe("ai");
  });
  test("seed does not rewrite the address bar", () => {
    // replaceState of the identical URL: same visible URL, no extra entry.
    expect(location.pathname + location.search).toBe("/search?q=python%20asyncio&mode=ai");
    expect(history.length).toBe(1);
  });
});
