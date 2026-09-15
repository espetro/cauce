import { describe, expect, test } from "bun:test";
import { applyAnswerEvent, INITIAL, type AnswerState } from "./useAnswer";

const step = (label: string) => ({ type: "step" as const, tool: "search", query: "q", label });

describe("applyAnswerEvent", () => {
  test("steps append labels in order", () => {
    let s: AnswerState = INITIAL;
    s = applyAnswerEvent(s, step("searching"));
    s = applyAnswerEvent(s, step("reading"));
    expect(s.steps).toEqual(["searching", "reading"]);
  });

  test("deltas concatenate text and mark streaming", () => {
    let s: AnswerState = applyAnswerEvent(INITIAL, { type: "delta", text: "hello " });
    s = applyAnswerEvent(s, { type: "delta", text: "world" });
    expect(s.text).toBe("hello world");
    expect(s.status).toBe("streaming");
  });

  test("sources replace the list", () => {
    const s = applyAnswerEvent(INITIAL, {
      type: "sources",
      sources: [{ title: "a", url: "https://a.example" }],
    });
    expect(s.sources).toHaveLength(1);
  });

  test("done finalizes with answer metadata", () => {
    const s = applyAnswerEvent(INITIAL, {
      type: "done",
      answer: "final",
      related_questions: ["q2"],
      confidence: 0.9,
      cached: true,
    });
    expect(s.status).toBe("done");
    expect(s.text).toBe("final");
    expect(s.cached).toBe(true);
    expect(s.confidence).toBe(0.9);
    expect(s.relatedQuestions).toEqual(["q2"]);
  });

  test("done with error sets status error", () => {
    const s = applyAnswerEvent(INITIAL, {
      type: "done",
      answer: "",
      related_questions: [],
      confidence: 0,
      cached: false,
      error: "boom",
    });
    expect(s.status).toBe("error");
    expect(s.error).toBe("boom");
  });

  test("steps transition idle to streaming", () => {
    const s = applyAnswerEvent(INITIAL, step("searching"));
    expect(s.status).toBe("streaming");
  });

  test("done keeps streamed text when answer empty", () => {
    let s = applyAnswerEvent(INITIAL, { type: "delta", text: "partial" });
    s = applyAnswerEvent(s, {
      type: "done",
      answer: "",
      related_questions: [],
      confidence: 0,
      cached: false,
    });
    expect(s.text).toBe("partial");
  });
});
