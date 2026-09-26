// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { describe, expect, it, vi } from "vitest";
import { createStreamRenderer, initSearchStream } from "../src/search.js";

function clickNoNav(el) {
  el.addEventListener("click", (e) => e.preventDefault(), { once: true });
  el.click();
}

const S = {
  results: "results",
  no_results: "No results",
  waiting: "Waiting for engines...",
  complete: "Done",
  invalid_stream: "invalid stream",
  new_above: "{n} new results above",
  live_badge: "live · {ms} ms",
  cached: "cached",
  stale_badge: "stale",
  engine_failed: "{engine} failed ({kind})",
  engine_skipped: "{engine} skipped",
  err_rate_limited: "rate limited",
  err_blocked: "blocked",
  err_timeout: "timeout",
  err_parse: "parse",
  err_transport: "transport",
  err_no_results: "no results",
  err_unknown: "error",
};

/** The DOM shell page.html renders while `is_streaming`. */
function streamShell() {
  document.body.innerHTML = `
    <span id="result-count"></span>
    <span id="search-meta"></span>
    <span id="request-id" class="request-id"></span>
    <div id="search-stream" data-query-hash="hash123" aria-busy="true">
      <span id="stream-status"></span>
    </div>
    <p id="new-results-above" hidden></p>
    <div id="results"></div>`;
  return {
    stream: document.getElementById("search-stream"),
    results: document.getElementById("results"),
    status: document.getElementById("stream-status"),
    count: document.getElementById("result-count"),
    metaLine: document.getElementById("search-meta"),
    requestId: document.getElementById("request-id"),
    above: document.getElementById("new-results-above"),
  };
}

function renderer() {
  const refs = streamShell();
  const fetchImpl = vi.fn(() => Promise.resolve({ ok: true }));
  const r = createStreamRenderer(refs, S, { fetchImpl });
  return { refs, fetchImpl, ...r };
}

const resultsFrame = (results) => ({
  name: "results",
  data: JSON.stringify({ results }),
});

const metaFrame = (meta) => ({ name: "meta", data: JSON.stringify(meta) });

describe("createStreamRenderer — results frames", () => {
  it("appends an article per result with icon, link, host and snippet", () => {
    const { refs, handleMessage } = renderer();
    handleMessage(
      resultsFrame([
        {
          key: "k1",
          url: "https://example.com/a",
          title: "Example A",
          snippet: "snippet a",
        },
      ]),
    );
    const article = refs.results.querySelector("article");
    expect(article.dataset.key).toBe("k1");
    expect(article.querySelector("img").src).toBe(
      "https://icons.duckduckgo.com/ip3/example.com.ico",
    );
    const a = article.querySelector("a");
    expect(a.href).toBe("https://example.com/a");
    expect(a.target).toBe("_blank");
    expect(article.querySelector(".host").textContent).toBe("example.com");
    expect(article.querySelector(".snippet").textContent).toBe("snippet a");
    expect(refs.count.textContent).toBe("1 results");
  });

  it("dedupes on the normalized key", () => {
    const { refs, handleMessage } = renderer();
    handleMessage(resultsFrame([{ key: "k", url: "https://a.com/", title: "t", snippet: "s" }]));
    handleMessage(resultsFrame([{ key: "k", url: "https://a.com/x", title: "t2", snippet: "s2" }]));
    expect(refs.results.querySelectorAll("article")).toHaveLength(1);
  });

  it("posts the click beacon with position and query hash", () => {
    const { refs, fetchImpl, handleMessage } = renderer();
    handleMessage(
      resultsFrame([
        { key: "k1", url: "https://a.com/1", title: "t1", snippet: "s1" },
        { key: "k2", url: "https://a.com/2", title: "t2", snippet: "s2" },
      ]),
    );
    clickNoNav(refs.results.querySelectorAll("article a")[1]);
    expect(fetchImpl).toHaveBeenCalledWith(
      "/api/click",
      expect.objectContaining({
        method: "POST",
        keepalive: true,
        body: JSON.stringify({
          url: "https://a.com/2",
          title: "t2",
          position: 1,
          query_hash: "hash123",
        }),
      }),
    );
  });
});

describe("createStreamRenderer — meta frame wiring", () => {
  const meta = {
    engines_used: [
      { engine: "bing", status: "ok" },
      { engine: "brave", status: { failed: "blocked" } },
    ],
    engines_skipped: ["ddgs"],
    source: "network",
    elapsed_ms: 42,
    request_id: "0123456789abcdef",
    order: ["k2", "k1"],
  };

  it("renders the badge, engine statuses, request id and completes", () => {
    const { refs, handleMessage } = renderer();
    handleMessage(metaFrame(meta));
    expect(refs.metaLine.textContent).toBe(
      "live · 42 ms · bing · brave failed (blocked) · ddgs skipped",
    );
    expect(refs.requestId.title).toBe("0123456789abcdef");
    expect(refs.requestId.textContent).toBe("01234567");
    expect(refs.status.textContent).toBe("Done");
    expect(refs.stream.getAttribute("aria-busy")).toBe("false");
  });

  it("arms the assist card with the final merged order", () => {
    const refs = streamShell();
    const assist = { setContext: vi.fn() };
    const { handleMessage } = createStreamRenderer(refs, S, {
      fetchImpl: vi.fn(),
      assist,
    });
    handleMessage(
      resultsFrame([
        { key: "k1", url: "https://a.com/1", title: "t1", snippet: "s" },
        { key: "k2", url: "https://a.com/2", title: "t2", snippet: "s" },
        { key: "k3", url: "https://a.com/3", title: "t3", snippet: "s" },
      ]),
    );
    handleMessage(metaFrame(meta));
    // meta.order is k2,k1 — the final merge dropped k3; Assist gets the
    // rows in merged order, not arrival order.
    expect(assist.setContext).toHaveBeenCalledWith([
      expect.objectContaining({ key: "k2" }),
      expect.objectContaining({ key: "k1" }),
    ]);
  });

  it("hides arrivals the final merge dropped and counts outranked results", () => {
    const { refs, handleMessage } = renderer();
    handleMessage(
      resultsFrame([
        { key: "k1", url: "https://a.com/1", title: "t1", snippet: "s" },
        { key: "k2", url: "https://a.com/2", title: "t2", snippet: "s" },
        { key: "k3", url: "https://a.com/3", title: "t3", snippet: "s" },
      ]),
    );
    handleMessage(metaFrame({ ...meta, order: ["k2", "k1"] }));
    const articles = [...refs.results.querySelectorAll("article")];
    expect(articles.find((a) => a.dataset.key === "k3").hidden).toBe(true);
    // k2 (rank 0) arrived after k1 (rank 1) -> one outranked arrival.
    expect(refs.above.hidden).toBe(false);
    expect(refs.above.textContent).toBe("1 new results above");
  });

  it("renders the empty-state line when no results arrived", () => {
    const { refs, handleMessage } = renderer();
    handleMessage(metaFrame({ ...meta, order: [] }));
    expect(refs.results.querySelector("p").textContent).toBe(
      "No results · bing · brave failed (blocked) · ddgs skipped",
    );
  });

  it("uses the stale badge for a stale cache source", () => {
    const { refs, handleMessage } = renderer();
    handleMessage(
      metaFrame({ ...meta, source: { cache: { stale: true } }, engines_used: [], engines_skipped: [] }),
    );
    expect(refs.metaLine.textContent).toBe("stale");
  });

  it("maps error objects and strings to their kind strings", () => {
    const { refs, handleMessage } = renderer();
    handleMessage(
      metaFrame({
        ...meta,
        engines_used: [
          { engine: "a", status: { failed: "timeout" } },
          { engine: "b", status: { failed: "mystery" } },
        ],
        engines_skipped: [],
      }),
    );
    expect(refs.metaLine.textContent).toContain("a failed (timeout)");
    expect(refs.metaLine.textContent).toContain("b failed (error)");
  });
});

describe("createStreamRenderer — error and malformed frames", () => {
  it("surfaces the payload message on error frames", () => {
    const { refs, handleMessage } = renderer();
    handleMessage({ name: "error", data: '{"error":{"message":"engine exploded"}}' });
    expect(refs.status.textContent).toBe("engine exploded");
    expect(refs.stream.getAttribute("aria-busy")).toBe("false");
  });

  it("reports invalid JSON as an invalid stream", () => {
    const { refs, handleMessage } = renderer();
    handleMessage({ name: "results", data: "{not json" });
    expect(refs.status.textContent).toBe("invalid stream");
  });
});

describe("initSearchStream — cauce:sse DOM wiring", () => {
  it("binds the body listener and routes frames to the renderer", () => {
    const refs = streamShell();
    initSearchStream(document, S);
    document.body.dispatchEvent(
      new CustomEvent("cauce:sse", {
        detail: {
          name: "meta",
          data: JSON.stringify({
            engines_used: [],
            engines_skipped: [],
            source: "network",
            elapsed_ms: 5,
            request_id: "ffffffffffffffff",
            order: [],
          }),
        },
      }),
    );
    expect(refs.status.textContent).toBe("Done");
    expect(refs.stream.getAttribute("aria-busy")).toBe("false");
  });

  it("is a no-op without the stream shell or the S bundle", () => {
    document.body.innerHTML = `<div id="search-stream"></div>`;
    initSearchStream(document, undefined); // no S
    document.body.innerHTML = `<div id="results"></div>`; // no #search-stream
    initSearchStream(document, S);
    // Neither call threw and nothing rendered.
    expect(document.getElementById("stream-status")).toBeNull();
  });
});
