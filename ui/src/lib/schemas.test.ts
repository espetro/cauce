import { describe, expect, test } from "bun:test";
import * as v from "valibot";
import {
  AnswerEventSchema,
  HistoryResponseSchema,
  ModelsResponseSchema,
  SearchResponseSchema,
  SettingsGetSchema,
} from "./schemas";

/** Real cached-hit payload, mirroring the backend's SearchResponse
 * round-trip fixture (tests/test_openapi_fresh.py): all `_`-fields
 * present, `_error`/`_error_kind` as explicit nulls. */
const cachedSearchPayload = {
  requestId: "r",
  searchType: "auto",
  results: [
    {
      title: "t",
      url: "https://a.example",
      id: "h1",
      text: "cached body",
      highlights: ["hl"],
      favicon: null,
      publishedDate: null,
      author: null,
      image: null,
    },
  ],
  costDollars: { total: 0.0 },
  _source: "cache",
  _q_hash: "abc",
  _q: "hello",
  _backend: "ddg",
  _duration_ms: 42,
  _cached_at: 1700000000,
  _error: null,
  _error_kind: null,
  _page: 1,
};

/** Fresh-miss payload: the backend now serializes explicit nulls instead
 * of omitting the keys. */
const freshMissPayload = {
  requestId: null,
  searchType: null,
  results: [],
  costDollars: null,
  _source: null,
  _q_hash: "def",
  _q: "world",
  _backend: null,
  _duration_ms: null,
  _cached_at: null,
  _error: null,
  _error_kind: null,
};

describe("SearchResponseSchema", () => {
  test("parses a real cached-hit payload with explicit nulls", () => {
    const out = v.parse(SearchResponseSchema, cachedSearchPayload);
    expect(out._source).toBe("cache");
    expect(out._cached_at).toBe(1700000000);
    expect(out.results).toHaveLength(1);
    expect(out.results[0].title).toBe("t");
    expect(out.results[0].favicon).toBe(null);
  });

  test("parses a fresh miss (explicit nulls where keys used to be absent)", () => {
    const out = v.parse(SearchResponseSchema, freshMissPayload);
    expect(out._source).toBe(null);
    expect(out._cached_at).toBe(null);
    expect(out.results).toEqual([]);
  });
});

describe("HistoryResponseSchema", () => {
  test("parses merged click + cache items", () => {
    const out = v.parse(HistoryResponseSchema, {
      items: [
        {
          kind: "click",
          clicked_at: 100,
          sort_at: 100,
          query_hash: "h",
          query: "q",
          result_id: "r1",
          url: "https://a.example",
          title: "t",
          source: "web-ui",
        },
        {
          kind: "cache",
          created_at: 200,
          sort_at: 200,
          expires_at: 300,
          query_hash: "h2",
          query: "q2",
          hits: 1,
          size_bytes: 10,
        },
      ],
      clicks: 1,
      cache_rows: 1,
      limit: 50,
      since: "7d",
    });
    expect(out.items[0].kind).toBe("click");
    expect(out.items[1].kind).toBe("cache");
  });
});

describe("ModelsResponseSchema", () => {
  test("parses the /v1/models payload", () => {
    const out = v.parse(ModelsResponseSchema, {
      object: "list",
      data: [{ id: "gpt-x" }],
      ai_available: true,
      error: null,
    });
    expect(out.data[0].id).toBe("gpt-x");
  });
});

describe("SettingsGetSchema", () => {
  test("parses ai section with redacted key absent", () => {
    const out = v.parse(SettingsGetSchema, {
      configured: true,
      config_path: "~/.config/oxe/config.toml",
      ai: {
        provider: "groq",
        model: "llama",
        base_url: null,
        enabled: true,
        api_key_set: true,
        api_key_env: null,
      },
    });
    expect(out.ai?.provider).toBe("groq");
  });
});

describe("AnswerEventSchema", () => {
  test("parses every ai.py event shape", () => {
    const step = v.parse(AnswerEventSchema, {
      type: "step",
      tool: "web_search",
      query: "q",
      label: "Searching...",
    });
    expect(step.type).toBe("step");
    const delta = v.parse(AnswerEventSchema, { type: "delta", text: "x" });
    expect(delta.type).toBe("delta");
    const sources = v.parse(AnswerEventSchema, {
      type: "sources",
      sources: [{ title: "a", url: "https://a.example", favicon: null }],
    });
    expect(sources.type).toBe("sources");
    const done = v.parse(AnswerEventSchema, {
      type: "done",
      answer: "a",
      related_questions: ["q1"],
      confidence: 9,
      model: "m",
      cached: false,
    });
    expect(done.type).toBe("done");
    const doneErr = v.parse(AnswerEventSchema, {
      type: "done",
      answer: "",
      related_questions: [],
      confidence: 0,
      error: "boom",
    });
    if (doneErr.type === "done") expect(doneErr.error).toBe("boom");
  });

  test("done with error: null parses (backend always emits the key)", () => {
    const done = v.parse(AnswerEventSchema, {
      type: "done",
      answer: "ok",
      related_questions: [],
      confidence: 9,
      cached: true,
      error: null,
      sources: [],
    });
    if (done.type === "done") expect(done.error).toBeNull();
  });

  test("rejects unknown event types (malformed frames are skipped by ai.ts)", () => {
    expect(v.is(AnswerEventSchema, { type: "nope" })).toBe(false);
  });
});
