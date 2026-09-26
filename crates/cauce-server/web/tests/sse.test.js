// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { registerSseExtension, sseExtension } from "../src/sse.js";

class FakeEventSource {
  static instances = [];
  constructor(url) {
    this.url = url;
    this.listeners = new Map();
    this.closed = false;
    FakeEventSource.instances.push(this);
  }
  addEventListener(name, fn) {
    if (!this.listeners.has(name)) this.listeners.set(name, []);
    this.listeners.get(name).push(fn);
  }
  emit(name, message) {
    for (const fn of this.listeners.get(name) || []) fn(message);
  }
  close() {
    this.closed = true;
  }
}

function fakeHtmx() {
  return {
    defineExtension: vi.fn(),
    trigger: vi.fn(),
  };
}

function connect(htmx, { sseConnect = "/api/search/stream?q=x" } = {}) {
  const ext = sseExtension(htmx, FakeEventSource);
  const element = document.createElement("div");
  if (sseConnect !== null) element.setAttribute("sse-connect", sseConnect);
  ext.onEvent("htmx:afterProcessNode", { target: element });
  return { ext, element, source: FakeEventSource.instances.at(-1) };
}

beforeEach(() => {
  FakeEventSource.instances = [];
});

describe("sseExtension", () => {
  it("exposes the [sse-connect] selector", () => {
    expect(sseExtension(fakeHtmx(), FakeEventSource).getSelectors()).toEqual(["[sse-connect]"]);
  });

  it("opens an EventSource on sse-connect once", () => {
    const htmx = fakeHtmx();
    const { ext, element, source } = connect(htmx);
    expect(source.url).toBe("/api/search/stream?q=x");
    expect(element.cauceEventSource).toBe(source);
    ext.onEvent("htmx:afterProcessNode", { target: element });
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it("ignores elements without sse-connect and non-matching events", () => {
    const htmx = fakeHtmx();
    const { source: connected } = connect(htmx, { sseConnect: null });
    expect(connected).toBeUndefined();
    const element = document.createElement("div");
    sseExtension(htmx, FakeEventSource).onEvent("htmx:load", { target: element });
    expect(FakeEventSource.instances).toHaveLength(0);
  });

  it("re-dispatches results frames as cauce:sse", () => {
    const htmx = fakeHtmx();
    const { element, source } = connect(htmx);
    source.emit("results", { data: '{"results":[]}' });
    expect(htmx.trigger).toHaveBeenCalledWith(element, "cauce:sse", {
      name: "results",
      data: '{"results":[]}',
    });
    expect(source.closed).toBe(false);
  });

  it("meta frames dispatch then close the stream", () => {
    const htmx = fakeHtmx();
    const { element, source } = connect(htmx);
    source.emit("meta", { data: '{"request_id":"abc"}' });
    expect(htmx.trigger).toHaveBeenCalledWith(element, "cauce:sse", {
      name: "meta",
      data: '{"request_id":"abc"}',
    });
    expect(source.closed).toBe(true);
  });

  it("error frames dispatch then close the stream", () => {
    const htmx = fakeHtmx();
    const { source } = connect(htmx);
    source.emit("error", { data: '{"error":{"message":"boom"}}' });
    expect(source.closed).toBe(true);
  });

  it("drops messages without data", () => {
    const htmx = fakeHtmx();
    const { source } = connect(htmx);
    source.emit("results", {});
    expect(htmx.trigger).not.toHaveBeenCalled();
  });

  it("closes the source on htmx:beforeCleanupElement", () => {
    const htmx = fakeHtmx();
    const { ext, element, source } = connect(htmx);
    ext.onEvent("htmx:beforeCleanupElement", { target: element });
    expect(source.closed).toBe(true);
  });

  it("resolves the element from event.detail.elt when target is absent", () => {
    const htmx = fakeHtmx();
    const ext = sseExtension(htmx, FakeEventSource);
    const elt = document.createElement("div");
    elt.setAttribute("sse-connect", "/s");
    ext.onEvent("htmx:afterProcessNode", { detail: { elt } });
    expect(elt.cauceEventSource).toBeDefined();
  });
});

describe("registerSseExtension", () => {
  it("registers the extension under the name htmx expects (hx-ext=\"sse\")", () => {
    const htmx = fakeHtmx();
    registerSseExtension(htmx, FakeEventSource);
    expect(htmx.defineExtension).toHaveBeenCalledWith("sse", expect.any(Object));
  });

  it("no-ops without htmx", () => {
    expect(() => registerSseExtension(undefined, FakeEventSource)).not.toThrow();
  });
});
