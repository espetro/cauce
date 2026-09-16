import { describe, expect, test } from "bun:test";
import { metaLine, nextStatus } from "./useSearch";
import type { SearchResponse } from "../../lib/api";
import { cachedAgeOf, isCacheHit } from "./useSearch";

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
    expect(cachedAgeOf(payload({ _cached_at: 2_000_000_000 }), 1_000_000_000)).toBe(null);
  });
});

describe("metaLine", () => {
  test("counts accumulated results", () => {
    expect(metaLine(payload({}), 1)).toBe("1 result");
  });
  test("pluralizes results", () => {
    expect(metaLine(payload({}), 0)).toBe("0 results");
    expect(metaLine(payload({}), 23)).toBe("23 results");
  });
  test("no payload and no results renders nothing", () => {
    expect(metaLine(null, 0)).toBe("");
  });
});

describe("nextStatus", () => {
  test("empty result list -> empty", () => {
    expect(nextStatus(payload({ results: [] }))).toBe("empty");
  });
  test("non-empty -> success", () => {
    expect(nextStatus(payload({}))).toBe("success");
  });
  test("empty + _error -> error (backend failure, not clean empty)", () => {
    expect(
      nextStatus(payload({ results: [], _error: "rate limited", _error_kind: "rate_limited" })),
    ).toBe("error");
  });
});
