/** AI-mode client types + endpoint clients: /v1/models, /answer (SSE). */

export interface ModelsResponse {
  object: "list";
  data: Array<{ id: string; object: "model"; owned_by?: string | null }>;
  ai_available: boolean;
  /** Backend hint when model listing failed (auth, base_url, ...). */
  error?: string | null;
}

export interface AiSource {
  title: string;
  url: string;
  favicon?: string | null;
}

export type AnswerEvent =
  | { type: "step"; tool: string; query: string; label: string }
  | { type: "delta"; text: string }
  | { type: "sources"; sources: AiSource[] }
  | {
      type: "done";
      answer: string;
      related_questions: string[];
      confidence: number;
      model?: string;
      cached: boolean;
      error?: string;
    };

export async function listModels(signal?: AbortSignal): Promise<ModelsResponse> {
  const res = await fetch(`/v1/models`, { signal });
  if (!res.ok)
    return { object: "list", data: [], ai_available: false, error: `HTTP ${res.status}` };
  return (await res.json()) as ModelsResponse;
}

/** Consume the /answer SSE stream via chunked fetch. Calls `on` per event. */
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
    let detail = `${res.status}`;
    try {
      const j = (await res.json()) as { detail?: string };
      if (j?.detail) detail = j.detail;
    } catch {
      // non-json error body
    }
    throw new Error(detail);
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
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
          on(JSON.parse(line.slice(6)) as AnswerEvent);
        } catch {
          // skip malformed frames
        }
      }
    }
  }
}
