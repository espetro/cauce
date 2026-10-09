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
import type { Config } from "../../types/Config.js";
import type { SearchResponse } from "../../types/SearchResponse.js";

const UI_HEADERS = { "X-Cauce-Client": "ui" } as const;

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
    const res = await fetch("/api/config", {
      headers: { Accept: "application/json", ...UI_HEADERS },
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
  const res = await fetch("/api/search?" + params.toString(), {
    headers: { Accept: "application/json", ...UI_HEADERS },
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
    headers: { "Content-Type": "application/json", ...UI_HEADERS },
    body: JSON.stringify(body),
    keepalive: true,
  }).catch(() => {});
}

/** `POST /api/pages` — index-on-click beacon (`archive.index_on_click` gate). */
export function indexBeacon(url: string, queryHash: string | null): void {
  const body = queryHash ? { url, query_hash: queryHash } : { url };
  fetch("/api/pages", {
    method: "POST",
    headers: { "Content-Type": "application/json", ...UI_HEADERS },
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
  return fetch("/api/answer", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "text/event-stream",
      ...UI_HEADERS,
    },
    body: JSON.stringify(body),
    signal,
  });
}
