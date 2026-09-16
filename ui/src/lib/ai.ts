/** AI-mode endpoint clients: /v1/models, /settings/test, /answer (SSE).
 * Response schemas + AnswerEvent live in ./schemas.ts (bound to the
 * generated OpenAPI types). */
import * as v from "valibot";
import { ApiError, request } from "./api";
import { devLog } from "./devlog";
import {
  AnswerEventSchema,
  ModelsResponseSchema,
  SettingsTestSchema,
  type AiSource,
  type AnswerEvent,
  type ModelsResponse,
  type SettingsTest,
} from "./schemas";

export type { AiSource, AnswerEvent, ModelsResponse };

export interface TestConnectionBody {
  provider: string;
  model: string;
  base_url?: string;
  api_key?: string;
}

/** Verify AI config with POST /settings/test (provider + key + model). */
export async function testConnection(body: TestConnectionBody): Promise<SettingsTest> {
  return request("/settings/test", SettingsTestSchema, {
    method: "POST",
    body: { ai: body },
  });
}

export async function listModels(signal?: AbortSignal): Promise<ModelsResponse> {
  return request("/v1/models", ModelsResponseSchema, { signal });
}

/** Consume the /answer SSE stream via chunked fetch. Calls `on` per event.
 * Malformed frames are skipped (try/catch), well-formed frames are
 * type-narrowed through AnswerEventSchema. */
export async function streamAnswer(
  query: string,
  on: (ev: AnswerEvent) => void,
  signal: AbortSignal,
): Promise<void> {
  const res = await fetch(`/answer`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "text/event-stream" },
    body: JSON.stringify({ query }),
    signal,
  });
  if (!res.ok || !res.body) {
    let code = "http_error";
    let detail = `HTTP ${res.status}`;
    try {
      const envelope = v.parse(
        v.object({ error: v.object({ code: v.string(), message: v.string() }) }),
        await res.json(),
      );
      code = envelope.error.code;
      detail = envelope.error.message;
    } catch {
      // non-json error body
    }
    throw new ApiError(code, detail, res.status);
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  const DEV = import.meta.env.DEV;
  // dev-only event counters (type counts, never payloads)
  const counts: Record<string, number> = {};
  const t0 = DEV ? performance.now() : 0;
  const tick = (type: string) => {
    if (!DEV) return;
    counts[type] = (counts[type] ?? 0) + 1;
  };
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    let idx: number;
    while ((idx = buf.indexOf("\n\n")) !== -1) {
      const frame = buf.slice(0, idx);
      buf = buf.slice(idx + 2);
      for (const line of frame.split("\n")) {
        if (!line.startsWith("data: ")) continue;
        try {
          const ev = v.parse(AnswerEventSchema, JSON.parse(line.slice(6)));
          tick(ev.type);
          on(ev);
        } catch {
          // skip malformed frames
        }
      }
    }
  }
  if (DEV) {
    devLog("answer.sse", {
      q: query,
      events: counts,
      duration_ms: Math.round(performance.now() - t0),
    });
  }
}
