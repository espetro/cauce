/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/app/search` state — the `createStreamRenderer`/`initSearchForm`/
 * `initIndexBeacon` logic of `web/src/search.ts` re-expressed as a
 * Svelte store. The wire contract is unchanged: `stream=1` pages open an
 * `EventSource` on `/api/search/stream?...&client=ui`, others fetch
 * `GET /api/search` (the SSR-page equivalent), `More` pages fetch page
 * N+1 and replace the row list (the `hx-swap="outerHTML"` behavior), and
 * clicks fire the `/api/click` (+ `/api/pages`, when armed) beacons.
 */

import {
  apiParams,
  clickBeacon,
  fetchSearch,
  indexBeacon,
  streamUrl,
  unknownRouteKeys,
} from "../../lib/api.js";
import { queryHash } from "../../lib/cacheKey.js";
import { capabilities, loadCapabilities } from "../../lib/capabilities.svelte.js";
import { hostOf } from "../../lib/format.js";
import { S, spa, fmt } from "../../lib/i18n.js";
import type { AnswerSource } from "../../../types/AnswerSource.js";
import type { ApiError } from "../../../types/ApiError.js";
import type { EngineError } from "../../../types/EngineError.js";
import type { ResultsFrame } from "../../../types/ResultsFrame.js";
import type { SearchMeta } from "../../../types/SearchMeta.js";
import type { SearchResponse } from "../../../types/SearchResponse.js";
import type { StreamMeta } from "../../../types/StreamMeta.js";
import type { StreamResult } from "../../../types/StreamResult.js";

const ERR_KINDS: Record<string, string> = {
  rate_limited: S.err_rate_limited,
  blocked: S.err_blocked,
  timeout: S.err_timeout,
  parse: S.err_parse,
  transport: S.err_transport,
  no_results: S.err_no_results,
};

export function errorKind(value: EngineError): string {
  const kind = typeof value === "string" ? value : Object.keys(value)[0];
  return ERR_KINDS[kind] || S.err_unknown;
}

/** `badge + engine statuses` — the line `renderMeta`/`badge()` compose. */
export function metaLineText(meta: SearchMeta, fullCacheBadge: boolean): string {
  const statuses = engineStatuses(meta);
  const base =
    meta.source === "network"
      ? fmt(S.live_badge, { ms: meta.elapsed_ms })
      : meta.source.cache.stale
        ? S.stale_badge
        : // The stream only sees `source != network` ("cached"); the JSON
          // arm carries age/ttl for the server-rendered badge text.
          fullCacheBadge
          ? fmt(S.cached_badge, { age: meta.source.cache.age_s, ttl: meta.source.cache.ttl_s })
          : S.cached;
  return statuses.length ? base + " · " + statuses.join(" · ") : base;
}

/** `engine_statuses()`: every engine report, ok names included. */
function engineStatuses(meta: SearchMeta): string[] {
  const statuses = meta.engines_used.map((report) =>
    report.status === "ok"
      ? report.engine
      : fmt(S.engine_failed, { engine: report.engine, kind: errorKind(report.status.failed) }),
  );
  for (const engine of meta.engines_skipped) {
    statuses.push(fmt(S.engine_skipped, { engine }));
  }
  return statuses;
}

export function statusesText(meta: SearchMeta): string {
  return engineStatuses(meta).join(" · ");
}

/** Stream rows carry `key`; the JSON/more payloads don't — dedupe on it, else url. */
interface RowLike {
  key?: string;
  url: string;
  title: string;
  snippet: string;
}

export interface SearchRow {
  /** Dedupe key (`key || url`) — `meta.order` entries match it. */
  key: string;
  url: string;
  title: string;
  snippet: string;
  host: string;
  /** 0-based arrival index — what `/api/click` records. */
  position: number;
  hidden: boolean;
}

export class SearchPageState {
  q = $state("");
  /** AI-mode segment of the omnibox (submit -> `/answer?q=`). */
  aiMode = $state(false);
  rows = $state<SearchRow[]>([]);
  hasSearched = $state(false);
  streaming = $state(false);
  loading = $state(false);
  statusText = $state("");
  countText = $state("");
  metaText = $state("");
  requestId = $state("");
  requestIdFull = $state("");
  newAbove = $state("");
  emptyText = $state("");
  errorText = $state("");
  moreVisible = $state(false);
  moreBusy = $state(false);
  /** Top rows (final order when known) armed into the Assist POST. */
  assistContext = $state<AnswerSource[]>([]);
  /** The `CacheKey` this search's beacons join on (`null` = unknown). */
  hash = $state<string | null>(null);

  #source: EventSource | null = null;
  #arrivals: string[] = [];
  #seen = new Set<string>();
  #byKey = new Map<string, StreamResult>();
  #params = new URLSearchParams();

  /** Run the route's `params` — the whole `/search?q=...&stream=1` job. */
  async run(params: URLSearchParams): Promise<void> {
    this.dispose();
    this.#reset();
    this.#params = new URLSearchParams(params);
    const q = params.get("q") ?? "";
    this.q = q;
    // The page's 400 arms, client-side, in the server's order
    // (`required("q")` -> `stream` -> `allow` -> typed params) — the
    // stream endpoint can't surface them legibly through EventSource.
    if (!q.trim()) {
      this.hasSearched = true;
      this.errorText = 'missing required parameter "q"';
      return;
    }
    const stream = params.get("stream");
    if (stream !== null && stream !== "1") {
      this.hasSearched = true;
      this.errorText = "stream must be 1 when present";
      return;
    }
    const extras = unknownRouteKeys(params);
    if (extras.length) {
      this.hasSearched = true;
      this.errorText = `unknown query parameter "${extras[0]}"`;
      return;
    }
    const pageRaw = params.get("page");
    if (pageRaw !== null && !(/^\d+$/.test(pageRaw) && +pageRaw >= 1 && +pageRaw <= 255)) {
      this.hasSearched = true;
      this.errorText = `invalid page "${pageRaw}"`;
      return;
    }
    const timeRange = params.get("time_range");
    if (timeRange !== null && !["day", "week", "month", "year"].includes(timeRange.toLowerCase())) {
      this.hasSearched = true;
      this.errorText = `unknown time_range: ${timeRange}`;
      return;
    }
    const safesearch = params.get("safesearch");
    if (
      safesearch !== null &&
      !["off", "0", "moderate", "1", "strict", "2"].includes(safesearch.toLowerCase())
    ) {
      this.hasSearched = true;
      this.errorText = `unknown safesearch level: ${safesearch}`;
      return;
    }
    const pinned = (params.get("engines") ?? "")
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (pinned.length) {
      // `validate_pin` parity (`search_error` maps it to 400): the SSR
      // page checks the pin before rendering the shell, so wait for the
      // engine-id set rather than letting EventSource swallow the 400.
      await loadCapabilities();
      const unknown = [...new Set(pinned.filter((id) => !capabilities.engineIds.includes(id)))];
      if (unknown.length) {
        this.hasSearched = true;
        this.errorText = `unknown engine ids: ${unknown.join(", ")}; configured engines: ${capabilities.engineIds.join(", ")}`;
        return;
      }
    }
    this.hasSearched = true;
    const api = apiParams(params);
    void this.#armHash(params);
    if (stream === "1") {
      this.#stream(api);
    } else {
      await this.#fetch(api);
    }
  }

  /** `More` — fetch page N+1 and replace the rows (outerHTML swap parity). */
  async loadMore(): Promise<void> {
    const p = new URLSearchParams(this.#params);
    const page = Number.parseInt(p.get("page") ?? "1", 10) || 1;
    p.set("page", String(page + 1));
    this.moreBusy = true;
    try {
      const resp = await fetchSearch(apiParams(p));
      this.#params = p;
      this.rows = resp.results.map((r, i) => this.#toRow(r, i));
      this.moreVisible = resp.results.length > 0;
      if (!resp.results.length) {
        const statuses = statusesText(resp.meta);
        this.emptyText = statuses ? S.no_results + " · " + statuses : S.no_results;
      }
      // The meta line is outside the swapped fragment — untouched.
    } catch {
      /* a failed page swap leaves the rows in place (htmx parity) */
    } finally {
      this.moreBusy = false;
    }
  }

  /** Omnibox submit: `/app/answer?q=` in AI mode, `/app/search?…&stream=1` else. */
  submit(navigate: (to: string) => void): void {
    const value = this.q.trim();
    if (!value) return;
    if (this.aiMode) {
      navigate("/app/answer?q=" + encodeURIComponent(value));
      return;
    }
    navigate("/app/search?q=" + encodeURIComponent(value) + "&stream=1");
  }

  /** Result-row click: click beacon always, index beacon when armed. */
  clickRow(row: SearchRow): void {
    clickBeacon({
      url: row.url,
      title: row.title,
      position: row.position,
      query_hash: this.hash,
    });
    if (capabilities.indexOnClick) indexBeacon(row.url, this.hash);
  }

  /** Close the EventSource on route change/unmount. */
  dispose(): void {
    this.#source?.close();
    this.#source = null;
  }

  #reset(): void {
    this.rows = [];
    this.#arrivals = [];
    this.#seen = new Set();
    this.#byKey = new Map();
    this.hasSearched = false;
    this.streaming = false;
    this.loading = false;
    this.statusText = "";
    this.countText = "";
    this.metaText = "";
    this.requestId = "";
    this.requestIdFull = "";
    this.newAbove = "";
    this.emptyText = "";
    this.errorText = "";
    this.moreVisible = false;
    this.moreBusy = false;
    this.assistContext = [];
    this.hash = null;
  }

  async #armHash(params: URLSearchParams): Promise<void> {
    const engines = (params.get("engines") ?? "")
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    this.hash = await queryHash({
      q: params.get("q") ?? "",
      page: Number.parseInt(params.get("page") ?? "1", 10) || 1,
      lang: params.get("lang") ?? undefined,
      timeRange: params.get("time_range") ?? undefined,
      safesearch: params.get("safesearch") ?? undefined,
      engines: engines.length ? engines : undefined,
    });
  }

  #stream(api: URLSearchParams): void {
    this.streaming = true;
    this.statusText = S.waiting;
    // The SSR shell fills the badge slot with "searching..." until
    // `meta` replaces it with the real badge.
    this.metaText = spa.search.searching;
    const source = new EventSource(streamUrl(api));
    this.#source = source;
    source.addEventListener("results", (event) => {
      const data = (event as MessageEvent).data;
      if (typeof data === "string") this.#onResults(data);
    });
    source.addEventListener("meta", (event) => {
      const data = (event as MessageEvent).data;
      if (typeof data === "string") this.#onMeta(data);
      source.close();
      this.#source = null;
    });
    source.addEventListener("error", (event) => {
      // Named `event: error` frames carry data; transport errors don't
      // (the source retries them itself — same as the htmx extension).
      const data = (event as MessageEvent).data;
      if (typeof data !== "string") return;
      try {
        const payload = JSON.parse(data) as ApiError;
        this.statusText = payload.error?.message ?? S.err_unknown;
      } catch {
        this.statusText = S.invalid_stream;
      }
      this.streaming = false;
      source.close();
      this.#source = null;
    });
  }

  async #fetch(api: URLSearchParams): Promise<void> {
    this.loading = true;
    try {
      const resp = await fetchSearch(api);
      this.#renderJson(resp);
    } catch (e) {
      this.errorText = e instanceof Error ? e.message : S.err_unknown;
    } finally {
      this.loading = false;
    }
  }

  #onResults(data: string): void {
    let payload: ResultsFrame;
    try {
      payload = JSON.parse(data) as ResultsFrame;
    } catch {
      this.statusText = S.invalid_stream;
      return;
    }
    for (const r of payload.results) {
      // Dedupe on the normalized key so variant spellings render once.
      const key = r.key || r.url;
      if (this.#seen.has(key)) continue;
      this.#seen.add(key);
      this.#arrivals.push(key);
      this.#byKey.set(key, r);
      this.rows.push(this.#toRow(r, this.#arrivals.length - 1));
    }
    this.countText = this.#arrivals.length + " " + S.results;
  }

  #onMeta(data: string): void {
    let meta: StreamMeta;
    try {
      meta = JSON.parse(data) as StreamMeta;
    } catch {
      this.statusText = S.invalid_stream;
      this.streaming = false;
      return;
    }
    this.metaText = metaLineText(meta, false);
    this.requestIdFull = meta.request_id;
    this.requestId = meta.request_id.slice(0, 8);
    this.statusText = S.complete;
    this.streaming = false;

    // Arm Assist with the top rows in final order (cap 10).
    this.assistContext = meta.order
      .map((key) => this.#byKey.get(key))
      .filter((r): r is StreamResult => r !== undefined)
      .slice(0, 10)
      .map((r) => ({ url: r.url, title: r.title, snippet: r.snippet, engine: r.engine }));

    // Hide anything the final merge did not keep.
    const rank = new Map(meta.order.map((key, i) => [key, i]));
    for (const row of this.rows) {
      if (!rank.has(row.key)) row.hidden = true;
    }
    const prevRanks: number[] = [];
    let outranking = 0;
    for (const key of this.#arrivals) {
      const current = rank.get(key) ?? Infinity;
      if (prevRanks.some((p) => current < p)) outranking += 1;
      prevRanks.push(current);
    }
    if (outranking > 0) this.newAbove = fmt(S.new_above, { n: outranking });
    if (!this.#arrivals.length) {
      const statuses = statusesText(meta);
      this.emptyText = statuses ? S.no_results + " · " + statuses : S.no_results;
    }
  }

  #renderJson(resp: SearchResponse): void {
    this.rows = resp.results.map((r, i) => this.#toRow(r, i));
    this.countText = resp.results.length + " " + S.results;
    this.metaText = metaLineText(resp.meta, true);
    this.requestIdFull = resp.meta.request_id;
    this.requestId = resp.meta.request_id.slice(0, 8);
    this.assistContext = resp.results.slice(0, 10).map((r) => ({
      url: r.url,
      title: r.title,
      snippet: r.snippet,
      engine: r.engine,
    }));
    this.moreVisible = resp.results.length > 0;
    if (!resp.results.length) {
      const statuses = statusesText(resp.meta);
      this.emptyText = statuses ? S.no_results + " · " + statuses : S.no_results;
    }
  }

  #toRow(r: RowLike, position: number): SearchRow {
    return {
      key: r.key || r.url,
      url: r.url,
      title: r.title,
      snippet: r.snippet,
      host: hostOf(r.url),
      position,
      hidden: false,
    };
  }
}

export function createSearchPage(): SearchPageState {
  return new SearchPageState();
}
