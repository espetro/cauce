import { describe, expect, test } from "bun:test";
import { domainOf, fmtDur, truncate } from "./format";

describe("domainOf", () => {
  test("strips scheme, path and www", () => {
    expect(domainOf("https://www.example.com/a/b?q=1")).toBe("example.com");
  });
  test("falls back to input on invalid url", () => {
    expect(domainOf("not a url")).toBe("not a url");
  });
});

describe("fmtDur", () => {
  test("seconds under a minute", () => {
    expect(fmtDur(45)).toBe("45s");
  });
  test("minutes and hours", () => {
    expect(fmtDur(120)).toBe("2m");
    expect(fmtDur(7200)).toBe("2h");
  });
  test("days", () => {
    expect(fmtDur(3 * 86400)).toBe("3d");
  });
  test("null and negative are 0s", () => {
    expect(fmtDur(null)).toBe("0s");
    expect(fmtDur(-5)).toBe("0s");
  });
});

describe("truncate", () => {
  test("keeps short strings", () => {
    expect(truncate("hello", 10)).toBe("hello");
  });
  test("clamps with ellipsis", () => {
    expect(truncate("hello world", 8)).toBe("hello w…");
  });
});
