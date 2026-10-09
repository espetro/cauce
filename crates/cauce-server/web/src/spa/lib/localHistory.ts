/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * FX-07 per-user storage for a `public` instance (§7.4): `serverHistory`
 * is off, so searches and clicks never reach the server DB — they live
 * in `localStorage` under `cauce:history`, newest-first, capped at 200
 * rows. The archive *index* is likewise per-user (`cauce:archive-index`);
 * the shared content store stays server-side keyed by URL.
 *
 * Rows carry the server wire shapes (`HistoryItem`) so the history view
 * folds local and server rows through the same code path.
 */

import type { ClickRow } from "../../types/ClickRow.js";
import type { HistoryItem } from "../../types/HistoryItem.js";

const HISTORY_KEY = "cauce:history";
const INDEX_KEY = "cauce:archive-index";
const LIMIT = 200;

function loadJson<T>(key: string): T[] {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as T[]) : [];
  } catch {
    return [];
  }
}

function saveJson(key: string, rows: unknown[]): void {
  try {
    localStorage.setItem(key, JSON.stringify(rows));
  } catch {
    /* quota/private-mode failures degrade to "not persisted" */
  }
}

/** Monotonic-enough local id: timestamp + counter (never server ids). */
let seq = 0;
export function localId(): number {
  seq += 1;
  return Date.now() * 1000 + (seq % 1000);
}

/** The browser-local history feed, newest-first. */
export function historyLoad(): HistoryItem[] {
  return loadJson<HistoryItem>(HISTORY_KEY);
}

/** Prepend a row, keep the newest `LIMIT`. */
export function historyRecord(item: HistoryItem): void {
  const rows = historyLoad();
  rows.unshift(item);
  saveJson(HISTORY_KEY, rows.slice(0, LIMIT));
}

/** Drop one row by local id (`row.gone` hides it in the view). */
export function historyRemove(id: number): void {
  saveJson(
    HISTORY_KEY,
    historyLoad().filter((i) => i.id !== id),
  );
}

/** What `historyRecord` needs for a click line (server fills the rest). */
export function clickItem(
  row: Pick<ClickRow, "url" | "title" | "position" | "query_hash">,
): HistoryItem {
  return {
    kind: "click",
    id: localId(),
    ts: new Date().toISOString(),
    client: "ui",
    ...row,
  };
}

/** One entry of the per-user archive index. */
export interface ArchiveIndexRow {
  url: string;
  title: string;
  /** ISO timestamp of the local record (`ArchiveRow.fetched_at` twin). */
  fetched_at: string;
}

/** The per-user archive index, newest-first. */
export function archiveIndexLoad(): ArchiveIndexRow[] {
  return loadJson<ArchiveIndexRow>(INDEX_KEY);
}

/** Record (or refresh) a URL in the index — the index_on_click arm. */
export function archiveIndexRecord(url: string, title: string): void {
  const rows = archiveIndexLoad().filter((r) => r.url !== url);
  rows.unshift({ url, title, fetched_at: new Date().toISOString() });
  saveJson(INDEX_KEY, rows.slice(0, LIMIT));
}
