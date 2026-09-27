/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * The htmx `sse` extension: elements carrying `sse-connect` get an
 * `EventSource` whose named frames (`results`, `meta`, `error`) are
 * re-dispatched as `cauce:sse` DOM events on the element. `meta`/`error`
 * are terminal — the source closes itself. Factored as a factory so tests
 * can inject a fake `htmx` and `EventSource`.
 */

/** The `EventSource` slice the extension parks on elements. */
export interface SseSource {
  addEventListener(name: string, listener: (event: Event) => void): void;
  close(): void;
}

/** The `reader.read()` chunk shape the SSE pump consumes. */
export interface SseChunk {
  value?: Uint8Array;
  done: boolean;
}

/** The `res.body.getReader()` slice a streamed fetch must expose. */
export interface SseBody {
  getReader(): { read(): Promise<SseChunk> };
}

/** The `fetch` Response slice the SSE-over-fetch pages consume. */
export interface SseResponse {
  ok: boolean;
  status: number;
  json(): Promise<unknown>;
  body: SseBody | null;
}

/** The `fetch` slice used for SSE-over-POST (`/api/answer`). */
export interface SseFetch {
  (input: string, init: RequestInit): Promise<SseResponse>;
}

/** One parsed SSE frame: the `event:` name and joined `data:` payload. */
export interface SseFrame {
  name: string;
  data: string;
}

/**
 * Split one raw SSE frame (`event:`/`data:` lines) into `{name, data}`.
 * Multiple `data:` lines concatenate (matching the historical inline
 * behavior); frames without data return `data: ""` and are skipped by
 * the caller.
 */
export function parseSseFrame(raw: string): SseFrame {
  let name = "message";
  let data = "";
  raw.split("\n").forEach((line) => {
    if (line.indexOf("event:") === 0) name = line.slice(6).trim();
    else if (line.indexOf("data:") === 0) data += line.slice(5).trim();
  });
  return { name, data };
}

/**
 * Read `res.body` to end, dispatching each `\n\n`-delimited raw frame to
 * `onFrame`. Shared by `answer`/`assist`, whose `EventSource`-less
 * SSE-over-POST streams need the same chunk-boundary handling.
 */
export async function pumpSse(
  res: SseResponse,
  onFrame: (raw: string) => void,
): Promise<void> {
  if (!res.body) return;
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  for (;;) {
    const chunk = await reader.read();
    buffer += decoder.decode(chunk.value, { stream: !chunk.done });
    let i;
    while ((i = buffer.indexOf("\n\n")) >= 0) {
      onFrame(buffer.slice(0, i));
      buffer = buffer.slice(i + 2);
    }
    if (chunk.done) break;
  }
  if (buffer.trim()) onFrame(buffer);
}

export function sseExtension(
  htmx: Pick<Htmx, "trigger">,
  EventSourceImpl: new (url: string) => SseSource,
): HtmxExtension {
  return {
    getSelectors() {
      return ["[sse-connect]"];
    },
    onEvent(name, event) {
      const target = event.target instanceof Element ? event.target : undefined;
      const detailElt = (event as HtmxEventLike).detail?.elt;
      const element =
        target ?? (detailElt instanceof Element ? detailElt : undefined);
      if (!element) return;

      if (name === "htmx:beforeCleanupElement") {
        if (element.cauceEventSource) element.cauceEventSource.close();
        return;
      }
      if (name !== "htmx:afterProcessNode") return;
      const url = element.getAttribute("sse-connect");
      if (url === null || element.cauceEventSource) return;

      const source = new EventSourceImpl(url);
      element.cauceEventSource = source;
      ["results", "meta", "error"].forEach((eventName) => {
        source.addEventListener(eventName, (message) => {
          if (!("data" in message) || typeof message.data !== "string") return;
          htmx.trigger(element, "cauce:sse", {
            name: eventName,
            data: message.data,
          });
          if (eventName === "meta" || eventName === "error") source.close();
        });
      });
    },
  };
}

/**
 * Register the extension on the page's `htmx` (inlined ahead of the bundle).
 * Pages without htmx (dashboard, audit) simply skip it.
 */
export function registerSseExtension(
  htmx: Pick<Htmx, "defineExtension" | "trigger"> | undefined = window.htmx,
  EventSourceImpl: new (url: string) => SseSource = window.EventSource,
): void {
  if (!htmx || !EventSourceImpl) return;
  htmx.defineExtension("sse", sseExtension(htmx, EventSourceImpl));
}
