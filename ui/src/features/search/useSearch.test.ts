import { describe, expect, test } from "bun:test";
import { metaLine, nextStatus } from "./useSearch";
import type { SearchResponse } from "../../lib/api";
import { cachedAgeOf, isCacheHit, pagerLetters } from "./useSearch";

const payload = (over: Partial<SearchResponse>): SearchResponse => ({
  requestId: "r1",
  results: [{ title: "t", url: "https://example.com" }],
  ...over,
});

describe("isCacheHit", () => {
  test("true when _source is cache", () => {
    expect(isCacheHit(payload({ _source: "cache" }))).toBe(true);
  });
  test("false when network or missing", () => {
    expect(isCacheHit(payload({ _source: "network" }))).toBe(false);
    expect(isCacheHit(payload({}))).toBe(false);
  });
});

describe("cachedAgeOf", () => {
  test("derives age from cached_at epoch seconds", () => {
    const now = 1_000_000_000;
    expect(cachedAgeOf(payload({ _cached_at: now - 120 }), now)).toBe(120);
  });
  test("null when absent or in the future", () => {
    expect(cachedAgeOf(payload({}))).toBe(null);
    expect(cachedAgeOf(payload({ _cached_at: 2_000_000_000 }))).toBe(null);
  });
});

describe("metaLine", () => {
  test("counts results", () => {
    expect(metaLine(payload({}))).toBe("1 result");
  });
  test("pluralizes results", () => {
    const p = payload({ results: [] });
    expect(metaLine(p)).toBe("0 results");
  });
  test("empty payload renders nothing", () => {
    expect(metaLine(null)).toBe("");
  });
});

describe("pagerLetters", () => {
  test("cycles o,x,e letters", () => {
    expect(pagerLetters(5)).toEqual(["o", "x", "e", "o", "x"]);
  });
  test("caps at 10 pages", () => {
    expect(pagerLetters(50)).toHaveLength(10);
  });
  test("zero pages when no results", () => {
    expect(pagerLetters(0)).toEqual([]);
  });
});

describe("nextStatus", () => {
  test("empty result list -> empty", () => {
    expect(nextStatus([])).toBe("empty");
  });
  test("non-empty -> success", () => {
    expect(nextStatus([{ url: "https://example.com" }])).toBe("success");
  });
});
