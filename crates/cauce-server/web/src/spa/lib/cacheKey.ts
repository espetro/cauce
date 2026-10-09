/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `CacheKey` (`cauce-core/src/cache.rs`) ported so the SPA can send the
 * same `query_hash` the server-rendered pages attach to `/api/click`
 * and `/api/pages` beacons: sha256 over the canonical request preimage —
 * `push_str(normalize_query(q))`, `page` u8, `lang` opt-str,
 * `time_range` u8, `safesearch` u8, then `engines` (0 | 1 + count +
 * sorted, dedup'd `push_str` each). Field order and encodings mirror
 * `impl From<&SearchRequest> for CacheKey`; keep them in lockstep.
 *
 * `normalize_query` = unicode lowercase + whitespace runs collapsed +
 * edges trimmed — `split_whitespace().join(" ").to_lowercase()` in
 * Rust; `\s` covers the same whitespace classes closely enough for the
 * beacon join.
 *
 * Returns `null` when `crypto.subtle` is unavailable (non-secure
 * context) — `ClickRow.query_hash` is nullable and the beacons degrade
 * exactly like a page that never carried the hash.
 */

const encoder = new TextEncoder();

export function normalizeQuery(q: string): string {
  return q
    .split(/\s+/)
    .filter((t) => t.length > 0)
    .join(" ")
    .toLowerCase();
}

export interface CacheKeyInput {
  q: string;
  /** 1-based; `default_page` on the wire. */
  page: number;
  lang?: string;
  /** Wire value: `day|week|month|year`; absent maps to byte 0. */
  timeRange?: string;
  /** Wire value: `off|moderate|strict`; absent defaults to `moderate`. */
  safesearch?: string;
  /** Engine ids; absent/empty pins nothing (preimage byte 0). */
  engines?: string[];
}

const TIME_RANGE_BYTE: Record<string, number> = {
  day: 1,
  week: 2,
  month: 3,
  year: 4,
};

const SAFESEARCH_BYTE: Record<string, number> = {
  off: 0,
  moderate: 1,
  strict: 2,
};

function pushU32(buf: number[], n: number): void {
  buf.push(n & 0xff, (n >> 8) & 0xff, (n >> 16) & 0xff, (n >> 24) & 0xff);
}

function pushStr(buf: number[], s: string): void {
  const bytes = encoder.encode(s);
  pushU32(buf, bytes.length);
  for (const b of bytes) buf.push(b);
}

function pushOptStr(buf: number[], s: string | undefined): void {
  if (s === undefined) {
    buf.push(0);
  } else {
    buf.push(1);
    pushStr(buf, s);
  }
}

/** sha256-hex of the canonical `SearchRequest` preimage, or `null`. */
export async function queryHash(input: CacheKeyInput): Promise<string | null> {
  try {
    const buf: number[] = [];
    pushStr(buf, normalizeQuery(input.q));
    buf.push(input.page & 0xff);
    pushOptStr(buf, input.lang);
    buf.push(input.timeRange ? (TIME_RANGE_BYTE[input.timeRange] ?? 0) : 0);
    // `SafeSearch::default` is Moderate — absent param hashes as 1.
    buf.push(SAFESEARCH_BYTE[input.safesearch ?? "moderate"] ?? 1);
    const engines = input.engines?.length ? [...new Set(input.engines)].sort() : null;
    if (!engines) {
      buf.push(0);
    } else {
      buf.push(1);
      pushU32(buf, engines.length);
      for (const id of engines) pushStr(buf, id);
    }
    const digest = await crypto.subtle.digest("SHA-256", new Uint8Array(buf));
    return Array.from(new Uint8Array(digest), (b) =>
      b.toString(16).padStart(2, "0"),
    ).join("");
  } catch {
    return null;
  }
}
