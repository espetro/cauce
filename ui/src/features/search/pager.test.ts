import { describe, expect, test } from "bun:test";
import { searchUrl } from "./pager";

describe("searchUrl", () => {
  test("basic query", () => {
    expect(searchUrl({ q: "python asyncio" })).toBe("/search?q=python+asyncio");
  });
  test("page 1 is omitted", () => {
    expect(searchUrl({ q: "x", page: 1 })).toBe("/search?q=x");
  });
  test("page > 1 kept", () => {
    expect(searchUrl({ q: "x", page: 3 })).toBe("/search?q=x&p=3");
  });
  test("ai mode expressed via mode param", () => {
    expect(searchUrl({ q: "x", mode: "ai" })).toBe("/search?q=x&mode=ai");
  });
  test("extra params preserved (settings)", () => {
    expect(searchUrl({ q: "x", extra: { settings: "open" } })).toBe("/search?q=x&settings=open");
  });
});
