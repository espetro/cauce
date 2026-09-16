/** Typed fetch client for the oxe backend.
 * Endpoints: POST /search, POST /click, GET /suggest, GET /ac,
 * GET /api/history, POST /history/delete, GET /api/stats, GET /cache/stats.
 *
 * One seam: every JSON call goes through `request()`, which parses the
 * backend error envelope {error: {code, message}} into `ApiError` and
 * validates success payloads with valibot schemas bound to the generated
 * OpenAPI types (see ../generated/types.gen.ts + ./schemas.ts). */

const BASE = "";

import * as v from "valibot";
import { devLog, devTimed } from "./devlog";
import {
  ApiStatsSchema,
  CacheStatsSchema,
  HistoryResponseSchema,
  SearchResponseSchema,
  type ApiStats,
  type CacheStats,
  type HistoryResponse,
  type SearchResponse,
} from "./schemas";

/** Typed error from the backend's unified envelope (and network/parse
 * failures): {code, message, status}. Render `message`; branch on `code`
 * (never status numbers). */
export class ApiError extends Error {
  code: string;
  status: number;
  constructor(code: string, message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = status;
  }
}

const ErrorEnvelopeSchema = v.object({
  error: v.object({ code: v.string(), message: v.string() }),
});

export interface RequestOptions {
  method?: string;
  body?: unknown;
  signal?: AbortSignal;
  accept?: string;
}

export async function request<S extends v.GenericSchema>(
  url: string,
  schema: S,
  opts: RequestOptions = {},
): Promise<v.InferOutput<S>> {
  let res: Response;
  try {
    res = await fetch(`${BASE}${url}`, {
      method: opts.method ?? "GET",
      headers: {
        ...(opts.body !== undefined ? { "Content-Type": "application/json" } : {}),
        Accept: opts.accept ?? "application/json",
      },
      ...(opts.body !== undefined ? { body: JSON.stringify(opts.body) } : {}),
      signal: opts.signal,
    });
  } catch (e) {
    if ((e as Error)?.name === "AbortError") throw e;
    throw new ApiError("network_error", (e as Error)?.message ?? "network error", 0);
  }
  if (!res.ok) {
    let code = "http_error";
    let message = `HTTP ${res.status}`;
    try {
      const envelope = v.parse(ErrorEnvelopeSchema, await res.json());
      code = envelope.error.code;
      message = envelope.error.message;
    } catch {
      // non-JSON or legacy error body: keep fallbacks
    }
    throw new ApiError(code, message, res.status);
  }
  return v.parse(schema, await res.json());
}

export interface SearchRequest {
  query: string;
  numResults?: number;
  /** 1-based result page (backend support lands separately; sent when > 1). */
  page?: number;
  type?: string;
  includeDomains?: string[];
  excludeDomains?: string[];
  category?: string;
  contents?: { text?: boolean; highlights?: boolean };
}

export interface ClickPayload {
  query_hash: string;
  result_id: string;
  url: string;
  title: string;
  source?: string;
}

export async function search(req: SearchRequest, signal?: AbortSignal): Promise<SearchResponse> {
  const out = await devTimed("search", { q: req.query, page: req.page ?? 1 }, () =>
    request("/search", SearchResponseSchema, {
      method: "POST",
      body: {
        numResults: 10,
        contents: { text: true, highlights: true },
        ...req,
        ...(req.page != null && req.page > 1 ? { page: req.page } : {}),
      },
      signal,
    }),
  );
  devLog("search.response", {
    q: req.query,
    source: out._source,
    results: out.results?.length ?? 0,
    server_ms: out._duration_ms,
  });
  return out;
}

export async function recordClick(payload: ClickPayload): Promise<void> {
  try {
    const body = JSON.stringify({ source: "web-ui", ...payload });
    if (navigator.sendBeacon) {
      navigator.sendBeacon(`${BASE}/click`, new Blob([body], { type: "application/json" }));
      return;
    }
    await fetch(`${BASE}/click`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body,
      keepalive: true,
    });
  } catch {
    // best effort
  }
}

/** DDG autocomplete via the backend proxy: JSON array of phrases, max 6. */
export async function ddgAc(q: string, signal?: AbortSignal): Promise<string[]> {
  const res = await devTimed("ac", { q }, () =>
    fetch(`${BASE}/ac?q=${encodeURIComponent(q)}`, { signal }),
  );
  if (!res.ok) return [];
  const out = (await res.json().catch(() => [])) as string[];
  const items = Array.isArray(out) ? out : [];
  devLog("ac", { q, results: items.length });
  return items;
}

/** OpenSearch suggestions: ["prefix", ["s1", ...], [], []] */
export async function suggest(q: string, signal?: AbortSignal): Promise<string[]> {
  const res = await devTimed("suggest", { q }, () =>
    fetch(`${BASE}/suggest?q=${encodeURIComponent(q)}`, { signal }),
  );
  if (!res.ok) return [];
  const data = (await res.json().catch(() => null)) as [string, string[]?] | null;
  const out = data?.[1] ?? [];
  devLog("suggest", { q, results: out.length });
  return out;
}

/** Delete a single cache row by its query hash (POST /row/{key}/delete).
 * Note: the backend route redirects (303) on success; we only check status. */
export async function deleteCacheRow(key: string): Promise<boolean> {
  try {
    const res = await fetch(`${BASE}/row/${encodeURIComponent(key)}/delete`, {
      method: "POST",
      headers: { Accept: "application/json" },
    });
    return res.ok;
  } catch {
    return false;
  }
}

/** GET /cache/stats (unvalidated: dashboard garnish, budget wins). */
export function cacheStats(signal?: AbortSignal): Promise<CacheStats> {
  return request("/cache/stats", CacheStatsSchema, { signal });
}

/** GET /api/history: merged clicks + cache-rows feed, newest first.
 * Params: since (hours or "all"), q (substring), limit (1-200), kind. */
export type HistoryScope = "24" | "168" | "720" | "all";

/** URL-contract since values map to the backend's vocabulary (24h|7d|30d|all). */
const SINCE_TO_BACKEND: Record<HistoryScope, string> = {
  "24": "24h",
  "168": "7d",
  "720": "30d",
  all: "all",
};
export type DeleteScope = "24h" | "7d" | "30d" | "all";

export async function fetchApiHistory(
  since: HistoryScope,
  qText: string,
  signal?: AbortSignal,
): Promise<HistoryResponse> {
  const p = new URLSearchParams({ since: SINCE_TO_BACKEND[since] });
  if (qText) p.set("q", qText);
  return request(`/api/history?${p.toString()}`, HistoryResponseSchema, { signal });
}

/** POST /history/delete: prune click history by scope.
 * Backend replies {"ok": true, "deleted": n} for JSON clients. */
export async function deleteHistory(scope: DeleteScope): Promise<number> {
  const out = await request(
    "/history/delete",
    v.object({ ok: v.optional(v.boolean()), deleted: v.number() }),
    { method: "POST", body: { scope } },
  );
  return out.deleted;
}

/** GET /api/stats: search-log aggregates for the dashboard plus cache
 * stats. Params: days=1..90 (default 14). */
export function apiStats(signal?: AbortSignal): Promise<ApiStats> {
  return request("/api/stats", ApiStatsSchema, { signal });
}
