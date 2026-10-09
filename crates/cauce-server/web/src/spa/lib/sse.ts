/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * SSE primitives, ported from `web/src/sse.ts` (the htmx extension half
 * stays behind with the legacy pages). `parseSseFrame` splits one raw
 * `event:`/`data:` frame; `pumpSse` drives it over a `fetch` body for
 * the SSE-over-POST surfaces (`/api/answer`).
 */

/** The `reader.read()` chunk shape the SSE pump consumes. */
export interface SseChunk {
  value?: Uint8Array;
  done: boolean;
}

/** The `res.body.getReader()` slice a streamed fetch must expose. */
export interface SseBody {
  getReader(): { read(): Promise<SseChunk> };
}

/** The `fetch` Response slice the SSE-over-fetch calls consume. */
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
 * Multiple `data:` lines concatenate; frames without data return
 * `data: ""` and are skipped by the caller.
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
 * `onFrame`.
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
