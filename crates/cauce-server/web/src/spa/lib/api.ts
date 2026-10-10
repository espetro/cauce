/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * The SPA's `/api/*` client. Every call carries `X-Cauce-Client: ui` —
 * the header the HTML pages send through htmx — except the EventSource
 * stream, which can't set headers and passes `client=ui` on the query
 * instead (the same substitution `stream_url()` makes server-side).
 */

import type { AnswerBody } from "../../types/AnswerBody.js";
import type { ApiError } from "../../types/ApiError.js";
import type { Capabilities } from "../../types/Capabilities.js";
import type { InstanceInfo } from "../../types/InstanceInfo.js";
import { authHeader } from "./admin.svelte.js";
import { noteApiCall } from "./backend.svelte.js";
import type { ArchiveResponse } from "../../types/ArchiveResponse.js";
import type { AuditRow } from "../../types/AuditRow.js";
import type { CachedSearch } from "../../types/CachedSearch.js";
import type { Config } from "../../types/Config.js";
import type { ConfigPutResponse } from "../../types/ConfigPutResponse.js";
import type { EngineToggleAck } from "../../types/EngineToggleAck.js";
import type { EngineView } from "../../types/EngineView.js";
import type { HistoryItem } from "../../types/HistoryItem.js";
import type { PageRow } from "../../types/PageRow.js";
import type { SearchResponse } from "../../types/SearchResponse.js";
import type { StatsSnapshot } from "../../types/StatsSnapshot.js";

const UI_HEADERS = { "X-Cauce-Client": "ui" } as const;

/**
 * `UI_HEADERS` + the bearer credential when the operator saved one —
 * the only header that can unlock the admin surface on a public
 * instance (FX-07).
 */
function headers(): Record<string, string> {
  return { ...UI_HEADERS, ...authHeader() };
}

/**
 * `fetch` + health bookkeeping: every `/api/*` round-trip reports its
 * outcome to the backend-health store (offline / degraded / ok) so the
 * sonner toaster reflects connectivity without waiting for the next
 * `/health` probe. Beacons stay on raw `fetch` — a fire-and-forget
 * keepalive miss should not flip the health state.
 */
async function apiFetch(input: string, init: RequestInit): Promise<Response> {
  try {
    const res = await fetch(input, init);
    noteApiCall(res);
    return res;
  } catch (err) {
    noteApiCall(null);
    throw err;
  }
}

/** Query-string keys `parse_search_request` accepts. */
const SEARCH_KEYS = ["q", "page", "lang", "time_range", "safesearch", "engines"];

/** Keys the `/app/search` route itself understands (`stream` stays SPA-side). */
export const ROUTE_KEYS = new Set([...SEARCH_KEYS, "stream"]);

/**
 * Whitelist the route's search params down to the API contract —
 * `deny_unknown_fields`-style strictness for the fetch, matching what
 * `stream_url()` forwards server-side (it drops `stream` and anything
 * else it doesn't know).
 */
export function apiParams(params: URLSearchParams): URLSearchParams {
  const out = new URLSearchParams();
  for (const key of SEARCH_KEYS) {
    const value = params.get(key);
    if (value !== null) out.set(key, value);
  }
  return out;
}

/** Route params outside `ROUTE_KEYS` — parity with the page's 400 arm. */
export function unknownRouteKeys(params: URLSearchParams): string[] {
  const extras = new Set<string>();
  for (const key of params.keys()) {
    if (!ROUTE_KEYS.has(key)) extras.add(key);
  }
  return [...extras];
}

/** `GET /api/config` — the capabilities source (`ai`, `archive`). */
export async function fetchConfig(): Promise<Config | null> {
  try {
    const res = await apiFetch("/api/config", {
      headers: { Accept: "application/json", ...headers() },
    });
    return res.ok ? ((await res.json()) as Config) : null;
  } catch {
    return null;
  }
}

/** `GET /api/search` — the non-streaming (SSR-equivalent) arm. */
export async function fetchSearch(
  params: URLSearchParams,
): Promise<SearchResponse> {
  const res = await apiFetch("/api/search?" + params.toString(), {
    headers: { Accept: "application/json", ...headers() },
  });
  if (!res.ok) throw await errorFrom(res);
  return (await res.json()) as SearchResponse;
}

/** The EventSource URL for the streaming arm. */
export function streamUrl(params: URLSearchParams): string {
  const p = new URLSearchParams(params);
  p.set("client", "ui");
  return "/api/search/stream?" + p.toString();
}

/** Read the `ApiError` envelope off a failed response. */
export async function errorFrom(res: Response): Promise<Error> {
  try {
    const env = (await res.json()) as ApiError;
    if (env?.error?.message) return new Error(env.error.message);
  } catch {
    /* fall through to the status line */
  }
  return new Error("HTTP " + res.status);
}

export interface ClickBeaconBody {
  url: string;
  title: string;
  position: number;
  query_hash: string | null;
}

/** `POST /api/click` — keepalive beacon, failure-silent like the HTMX one. */
export function clickBeacon(body: ClickBeaconBody): void {
  fetch("/api/click", {
    method: "POST",
    headers: { "Content-Type": "application/json", ...headers() },
    body: JSON.stringify(body),
    keepalive: true,
  }).catch(() => {});
}

/** `POST /api/pages` — index-on-click beacon (`archive.index_on_click` gate). */
export function indexBeacon(url: string, queryHash: string | null): void {
  const body = queryHash ? { url, query_hash: queryHash } : { url };
  fetch("/api/pages", {
    method: "POST",
    headers: { "Content-Type": "application/json", ...headers() },
    body: JSON.stringify(body),
    keepalive: true,
  }).catch(() => {});
}

/**
 * `POST /api/answer` — the assist/answer SSE stream (read via `pumpSse`).
 * `signal` aborts the stream — the `/app/answer` stop button's wire.
 */
export function postAnswer(
  body: AnswerBody,
  signal?: AbortSignal,
): Promise<Response> {
  return apiFetch("/api/answer", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "text/event-stream",
      ...headers(),
    },
    body: JSON.stringify(body),
    signal,
  });
}

/* ------------------------------------------------------------------ */
/* FX-05 read/admin surfaces — all JSON arms of the shared handlers.   */
/* ------------------------------------------------------------------ */

async function getJson<T>(path: string): Promise<T> {
  const res = await apiFetch(path, {
    headers: { Accept: "application/json", ...headers() },
  });
  if (!res.ok) throw await errorFrom(res);
  return (await res.json()) as T;
}

async function delJson(path: string): Promise<void> {
  const res = await apiFetch(path, { method: "DELETE", headers: headers() });
  if (!res.ok) throw await errorFrom(res);
}

/** `GET /api/stats?days=N` — the dashboard's whole data plane. */
export function fetchStats(days: number): Promise<StatsSnapshot> {
  return getJson("/api/stats?days=" + days);
}

/** `GET /api/history` — `HistoryItem[]` (search | click | answer rows). */
export function fetchHistory(params: URLSearchParams): Promise<HistoryItem[]> {
  return getJson("/api/history?" + params.toString());
}

/** `DELETE /api/history/{id}` — one search_log row. */
export function deleteHistoryRow(id: number): Promise<void> {
  return delJson("/api/history/" + id);
}

/** `DELETE /api/answer-log/{id}` — one answer_log row. */
export function deleteAnswerRow(id: number): Promise<void> {
  return delJson("/api/answer-log/" + id);
}

/** `GET /api/engines` — live health + card fields per engine. */
export function fetchEngines(): Promise<EngineView[]> {
  return getJson("/api/engines");
}

/**
 * `POST /api/engines/{id}/{reset|enable|disable}` — audited ops.
 * Returns the `EngineToggleAck` (`requires_restart` drives the hint).
 */
export async function postEngine(
  id: string,
  op: "reset" | "enable" | "disable",
): Promise<EngineToggleAck> {
  const res = await apiFetch(
    "/api/engines/" + encodeURIComponent(id) + "/" + op,
    { method: "POST", headers: headers() },
  );
  if (!res.ok) throw await errorFrom(res);
  return (await res.json()) as EngineToggleAck;
}

/**
 * `GET /api/cache` — `limit`/`offset`/`q` filtered listing. The endpoint
 * answers the bare `entries` array (`Json(listing.entries)`), so the
 * filter state stays with the caller — same data the HTMX page read.
 */
export function fetchCacheList(
  params: URLSearchParams,
): Promise<CachedSearch[]> {
  return getJson("/api/cache?" + params.toString());
}

/** `GET /api/cache/{key}` — one entry incl. the stored `response` payload. */
export function fetchCacheEntry(key: string): Promise<CachedSearch> {
  return getJson("/api/cache/" + encodeURIComponent(key));
}

/** `DELETE /api/cache/{key}` — one entry. */
export function deleteCacheEntry(key: string): Promise<void> {
  return delJson("/api/cache/" + encodeURIComponent(key));
}

/** `DELETE /api/cache?expired=true` / `?all=true` — bulk sweeps. */
export function deleteCacheBulk(scope: "expired" | "all"): Promise<void> {
  return delJson("/api/cache?" + scope + "=true");
}

/** `GET /api/audit` — `actor`/`action`/`since`/`limit` filtered rows. */
export function fetchAudit(params: URLSearchParams): Promise<AuditRow[]> {
  return getJson("/api/audit?" + params.toString());
}

/** `GET /api/archive` — `q`/`limit`/`offset` search + browse listing. */
export function fetchArchive(
  params: URLSearchParams,
): Promise<ArchiveResponse> {
  return getJson("/api/archive?" + params.toString());
}

/** `GET /api/pages/{url}` — the stored page's markdown. */
export function fetchPage(url: string): Promise<PageRow> {
  return getJson("/api/pages/" + encodeURIComponent(url));
}

/** `DELETE /api/pages/{url}` — drop a page from the archive. */
export function deletePage(url: string): Promise<void> {
  return delJson("/api/pages/" + encodeURIComponent(url));
}

/**
 * `PUT /api/config` — urlencoded dotted-path body (same settled input
 * the HTMX form posts; the TOML arm stays for curl users).
 */
export async function putConfig(
  body: URLSearchParams,
): Promise<ConfigPutResponse> {
  const res = await apiFetch("/api/config", {
    method: "PUT",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      Accept: "application/json",
      ...headers(),
    },
    body: body.toString(),
  });
  if (!res.ok) throw await errorFrom(res);
  return (await res.json()) as ConfigPutResponse;
}

/**
 * `GET /api/capabilities` — the FX-07 bootstrap payload: instance mode,
 * the caller's derived role and the capability flags. Always open.
 */
export async function fetchCapabilities(): Promise<Capabilities | null> {
  try {
    const res = await apiFetch("/api/capabilities", {
      headers: { Accept: "application/json", ...headers() },
    });
    return res.ok ? ((await res.json()) as Capabilities) : null;
  } catch {
    return null;
  }
}

/** `GET /api/instance` — the public instance card + SPA bootstrap knobs. */
export async function fetchInstance(): Promise<InstanceInfo | null> {
  try {
    const res = await apiFetch("/api/instance", {
      headers: { Accept: "application/json", ...headers() },
    });
    return res.ok ? ((await res.json()) as InstanceInfo) : null;
  } catch {
    return null;
  }
}
