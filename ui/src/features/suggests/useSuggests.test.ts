import { describe, expect, test } from "bun:test";
import { mergeSuggests } from "./useSuggests";

describe("mergeSuggests", () => {
  test("history first, deduped case-insensitively", () => {
    const items = mergeSuggests(["Python asyncio", "python asyncio", "rust tokio"]);
    expect(items.map((i) => i.text)).toEqual(["Python asyncio", "rust tokio"]);
    expect(items.every((i) => i.group === "history")).toBe(true);
  });
  test("web fills after history, deduped against it", () => {
    const items = mergeSuggests(["python asyncio"], ["python asyncio", "web ac"]);
    expect(items.map((i) => i.group)).toEqual(["history", "web"]);
  });
  test("caps history at 3 and web at 4", () => {
    expect(mergeSuggests(["a", "b", "c", "d"])).toHaveLength(3);
    expect(mergeSuggests([], ["1", "2", "3", "4", "5"])).toHaveLength(4);
  });
  test("blank strings are skipped", () => {
    expect(mergeSuggests(["", "  "])).toHaveLength(0);
    expect(mergeSuggests([], [""])).toHaveLength(0);
  });
});
