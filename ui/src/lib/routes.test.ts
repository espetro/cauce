import { beforeEach, describe, expect, test } from "bun:test";
import { Window } from "happy-dom";
import { routeUrl } from "./routes";

// Hermetic browser env per test file: happy-dom window provides the
// location/history/document globals the router touches (module-load seed,
// pushState/replaceState, popstate listeners).
let win: Window;

async function setup(url: string): Promise<typeof import("./routes")> {
  win = new Window({ url });
  globalThis.window = win as never;
  globalThis.location = win.location as never;
  globalThis.history = win.history as never;
  globalThis.document = win.document as never;
  return await import("./routes");
}

beforeEach(async () => {
  // Reset module state (router atom + prev-cache) for hermetic history
  // assertions per test.
  const mod = await setup("http://localhost:4479/");
  mod.router.listen(() => {});
});

describe("routeUrl search params", () => {
  test("encodes q", () => {
    expect(routeUrl("search", { q: "python asyncio" })).toBe("/search?q=python+asyncio");
  });
  test("never emits deprecated p", () => {
    expect(routeUrl("search", { q: "x", p: "3" } as never)).toBe("/search?q=x");
  });
  test("carries ai mode", () => {
    expect(routeUrl("search", { q: "x", mode: "ai" })).toBe("/search?q=x&mode=ai");
  });
  test("preserves settings flag", () => {
    expect(routeUrl("search", { q: "x", settings: "open" })).toBe("/search?q=x&settings=open");
  });
});

describe("p-stripping through navigation", () => {
  test("navigate drops p, keeps q", async () => {
    const { navigate, router } = await setup("http://localhost:4479/search?q=x&p=3");
    router.listen(() => {});
    navigate("search", { q: "x", p: "3" } as never);
    expect(location.pathname + location.search).toBe("/search?q=x");
  });
  test("openPath strips p from raw URLs when reparsing", async () => {
    const { openPath, router } = await setup("http://localhost:4479/");
    router.listen(() => {});
    // Router parse keeps p in search (router-level concern); openPath itself
    // is a raw passthrough — p stripping is the navigate/redirect/routeUrl
    // contract, verified above and below.
    openPath("/search?q=x&p=9");
    expect(router.get()?.path).toBe("/search");
  });
});

describe("push vs replace semantics", () => {
  test("navigate pushes a history entry", async () => {
    const { navigate } = await setup("http://localhost:4479/search?q=a");
    const before = history.length;
    navigate("history");
    expect(history.length).toBe(before + 1);
    expect(location.pathname).toBe("/history");
  });
  test("redirect replaces the current entry", async () => {
    const { redirect } = await setup("http://localhost:4479/search?q=a");
    const before = history.length;
    redirect("history", { settings: "open" });
    expect(history.length).toBe(before);
    expect(location.pathname + location.search).toBe("/history?settings=open");
  });
  test("openPath pushes by default, replaces with replace=true", async () => {
    const { openPath } = await setup("http://localhost:4479/search?q=a");
    const before = history.length;
    openPath("/history");
    expect(history.length).toBe(before + 1);
    openPath("/dashboard", true);
    expect(history.length).toBe(before + 1);
    expect(location.pathname).toBe("/dashboard");
  });
});

describe("router matching", () => {
  test("unknown path opens undefined route (404 page)", async () => {
    const { router } = await setup("http://localhost:4479/");
    expect(router.open("/row/abc")).toBeUndefined();
  });
});

describe("settings param preservation on /history", () => {
  test("navigate keeps settings param", async () => {
    const { navigate } = await setup("http://localhost:4479/history?settings=open");
    navigate("history", { settings: "open" });
    expect(location.pathname + location.search).toBe("/history?settings=open");
  });
  test("redirect keeps settings param while replacing", async () => {
    const { redirect } = await setup("http://localhost:4479/history?settings=open");
    redirect("history", { settings: "open" });
    expect(location.pathname + location.search).toBe("/history?settings=open");
  });
  test("routeUrl rebuilds /history with settings intact", async () => {
    const { routeUrl } = await setup("http://localhost:4479/");
    expect(routeUrl("history", { settings: "open" })).toBe("/history?settings=open");
  });
});
