/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/app/admin?tab=cache` state (FX-05): `GET /api/cache` listing with
 * the same `limit+1` next-page probe the HTMX page does server-side
 * (the SPA asks for `PAGE_LIMIT + 1`, shows `PAGE_LIMIT`), `q` lexical
 * filter, per-row lazy payload from `GET /api/cache/{key}`, row +
 * `expired`/`all` bulk deletes.
 */

import type { CachedSearch } from "../../../types/CachedSearch.js";
import {
  deleteCacheBulk,
  deleteCacheEntry,
  fetchCacheEntry,
  fetchCacheList,
} from "../../lib/api.js";
import { fmtTs, humanBytes, humanSeconds } from "../../lib/format.js";
import { fmt, spa } from "../../lib/i18n.js";

const C = () => spa.cache;

export const PAGE_LIMIT = 50;

export interface CacheRow {
  key: string;
  query: string;
  created: string;
  expires: string;
  expired: boolean;
  hitsLabel: string;
  engines: string;
  size: string;
  payload: string;
  payloadError: string;
  payloadLoading: boolean;
  gone: boolean;
}

function rowView(e: CachedSearch): CacheRow {
  const now = Date.now();
  const exp = new Date(e.expires_at).getTime();
  const expired = exp <= now;
  return {
    key: e.key,
    query: e.query,
    created: fmtTs(e.created_at),
    expires: expired
      ? fmt(C().expired_ago, { rel: humanSeconds((now - exp) / 1000) })
      : fmt(C().expires_in, { rel: humanSeconds((exp - now) / 1000) }),
    expired,
    hitsLabel:
      e.hits +
      " " +
      (e.hits === 1 ? C().hit_one : C().hit_many),
    engines: e.engines.join(", "),
    // The page's `size` is the stored payload's JSON length — the wire
    // hands us the parsed value, so this is the same number ± whitespace.
    size: humanBytes(JSON.stringify(e.response).length),
    payload: "",
    payloadError: "",
    payloadLoading: false,
    gone: false,
  };
}

export function createCacheTab() {
  const state = $state({
    loading: true,
    error: "",
    q: "",
    searching: false,
    offset: 0,
    rows: [] as CacheRow[],
    hasNext: false,
    countLine: "",
    emptyLine: "",
    filteredCap: "",
    bulkError: "",
  });

  function countLine(shown: number, searching: boolean): string {
    const word = shown === 1 ? C().entry_one : C().entry_many;
    return searching
      ? `${shown} ${C().matching} ${word}`
      : `${shown} ${word}`;
  }

  async function refresh(): Promise<void> {
    state.loading = true;
    state.error = "";
    state.bulkError = "";
    const p = new URLSearchParams();
    if (state.q.trim() !== "") p.set("q", state.q.trim());
    p.set("limit", String(state.searching ? PAGE_LIMIT : PAGE_LIMIT + 1));
    p.set("offset", String(state.offset));
    try {
      // `GET /api/cache` answers the bare `entries` array — the filter
      // state lives here (same params the HTMX page re-rendered).
      const entries = await fetchCacheList(p);
      state.searching = state.q.trim() !== "";
      state.hasNext = !state.searching && entries.length > PAGE_LIMIT;
      const shown = entries.slice(0, PAGE_LIMIT);
      state.rows = shown.map(rowView);
      state.countLine = countLine(shown.length, state.searching);
      state.filteredCap = state.searching
        ? fmt(C().filtered_cap, { n: PAGE_LIMIT })
        : "";
      state.emptyLine =
        state.rows.length === 0
          ? state.searching
            ? C().empty_filtered.replace("{q}", state.q)
            : C().empty
          : "";
    } catch (e) {
      state.error = e instanceof Error ? e.message : String(e);
    } finally {
      state.loading = false;
    }
  }

  async function run(params: URLSearchParams): Promise<void> {
    state.q = params.get("q") ?? "";
    state.offset = Math.max(0, Number(params.get("offset") ?? "0") || 0);
    await refresh();
  }

  async function loadPayload(row: CacheRow): Promise<void> {
    if (row.payload !== "" || row.payloadLoading) return;
    row.payloadLoading = true;
    row.payloadError = "";
    try {
      const entry = await fetchCacheEntry(row.key);
      row.payload = JSON.stringify(entry.response, null, 2);
    } catch (e) {
      row.payloadError = e instanceof Error ? e.message : String(e);
    } finally {
      row.payloadLoading = false;
    }
  }

  async function remove(row: CacheRow): Promise<void> {
    if (!window.confirm(C().confirm_row)) return;
    row.payloadError = "";
    try {
      await deleteCacheEntry(row.key);
      row.gone = true;
      // The count line counts shown rows — drop it by one, same as the
      // page's inline JS decrement.
      const n = state.rows.filter((r) => !r.gone).length;
      state.countLine = countLine(n, state.searching);
    } catch (e) {
      row.payloadError = e instanceof Error ? e.message : String(e);
    }
  }

  async function bulk(scope: "expired" | "all"): Promise<void> {
    const msg = scope === "expired" ? C().confirm_expired : C().confirm_all;
    if (!window.confirm(msg)) return;
    state.bulkError = "";
    try {
      await deleteCacheBulk(scope);
      await refresh();
    } catch (e) {
      state.bulkError = e instanceof Error ? e.message : String(e);
    }
  }

  function submit(navigate: (to: string) => void): void {
    const q = state.q.trim();
    navigate(
      "/app/admin?tab=cache" + (q === "" ? "" : "&q=" + encodeURIComponent(q)),
    );
  }

  function pagerUrl(offset: number): string {
    return "/app/admin?tab=cache&offset=" + Math.max(0, offset);
  }

  return {
    state,
    run,
    refresh,
    loadPayload,
    remove,
    bulk,
    submit,
    pagerUrl,
  };
}
