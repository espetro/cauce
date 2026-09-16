import { describe, expect, test } from "bun:test";
import { searchUrl } from "./pager";

describe("searchUrl", () => {
  test("builds /search?q=... with encoded query", () => {
    expect(searchUrl({ q: "python asyncio" })).toBe("/search?q=python+asyncio");
  });
  test("p is deprecated: no page param is ever generated", () => {
    expect(searchUrl({ q: "x" })).toBe("/search?q=x");
  });
  test("ai mode appended", () => {
    expect(searchUrl({ q: "x", mode: "ai" })).toBe("/search?q=x&mode=ai");
  });
  test("extra params preserved", () => {
    expect(searchUrl({ q: "x", extra: { settings: "open" } })).toBe("/search?q=x&settings=open");
  });
});
