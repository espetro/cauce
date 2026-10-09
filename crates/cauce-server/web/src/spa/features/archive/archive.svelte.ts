/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * `/app/archive` feature state (FX-05): `GET /api/archive` browse +
 * `q` search, lazy markdown per row via `GET /api/pages/{url}`, row
 * delete, prev/next pager on the browse arm. Mirrors
 * `src/html/archive.rs` — with two wire degradations noted on #266:
 * `ArchiveRow.snippet` arrives mark-stripped (no `<mark>` highlights)
 * and the `enabled` flag is inferred from the search call failing.
 */

import type { ArchiveRow } from "../../../types/ArchiveRow.js";
import { deletePage, fetchArchive, fetchPage } from "../../lib/api.js";
import { capabilities, loadCapabilities } from "../../lib/capabilities.svelte.js";
import {
  archiveIndexLoad,
  type ArchiveIndexRow,
} from "../../lib/localHistory.js";
import { fmtTs, hostOf } from "../../lib/format.js";
import { spa } from "../../lib/i18n.js";

const A = () => spa.archive;

export interface ArchiveRowView {
  url: string;
  title: string;
  host: string;
  fetched: string;
  snippet: string;
  markdown: string;
  markdownError: string;
  markdownLoading: boolean;
  gone: boolean;
}

export function createArchivePage() {
  const state = $state({
    loading: true,
    /** The fetch pipeline couldn't be built server-side (archive down). */
    disabled: false,
    error: "",
    q: "",
    searching: false,
    offset: 0,
    hasMore: false,
    rows: [] as ArchiveRowView[],
    countLine: "",
    emptyLine: "",
    requestId: "",
  });

  const LIMIT = 50;

  function plural(n: number, one: string, many: string): string {
    return n === 1 ? one : many;
  }

  function countLine(shown: number, searching: boolean): string {
    const word = plural(shown, A().page_one, A().page_many);
    return searching ? `${shown} ${A().matching} ${word}` : `${shown} ${word}`;
  }

  function rowView(r: ArchiveRow): ArchiveRowView {
    return {
      url: r.url,
      title: r.title === "" ? r.url : r.title,
      host: hostOf(r.url),
      fetched: fmtTs(r.fetched_at),
      snippet: r.snippet,
      markdown: "",
      markdownError: "",
      markdownLoading: false,
      gone: false,
    };
  }

  /**
   * FX-07: the per-user archive index on a public instance — the
   * `index_on_click` arm writes browser-local rows, content still
   * resolves from the shared server store (`GET /api/pages/{url}`
   * stays open). No snippets server-side, so a `q` filters url/title.
   */
  function indexRowView(r: ArchiveIndexRow): ArchiveRowView {
    return {
      url: r.url,
      title: r.title === "" ? r.url : r.title,
      host: hostOf(r.url),
      fetched: fmtTs(r.fetched_at),
      snippet: "",
      markdown: "",
      markdownError: "",
      markdownLoading: false,
      gone: false,
    };
  }

  function runLocalIndex(): void {
    const q = state.q.trim().toLowerCase();
    const rows = archiveIndexLoad().filter(
      (r) =>
        q === "" ||
        r.url.toLowerCase().includes(q) ||
        r.title.toLowerCase().includes(q),
    );
    state.searching = q !== "";
    state.hasMore = false;
    state.rows = rows.map(indexRowView);
    state.countLine = countLine(state.rows.length, state.searching);
    state.emptyLine =
      state.rows.length === 0
        ? state.searching
          ? A().empty_filtered.replace("{q}", state.q)
          : A().empty
        : "";
  }

  async function run(params: URLSearchParams): Promise<void> {
    state.loading = true;
    state.error = "";
    state.disabled = false;
    state.q = params.get("q") ?? "";
    state.offset = Math.max(0, Number(params.get("offset") ?? "0") || 0);
    const p = new URLSearchParams();
    if (state.q.trim() !== "") p.set("q", state.q.trim());
    p.set("limit", String(LIMIT));
    p.set("offset", String(state.offset));
    try {
      // FX-07: read real flags, not the local-mode defaults (the
      // bootstrapping `apply()` may still be in flight).
      await loadCapabilities();
      if (!capabilities.flags.archiving) {
        // `archive.enabled = false` — the whole surface is down; same
        // disabled arm the fetch failure produces.
        state.disabled = true;
        return;
      }
      if (!capabilities.flags.adminSurface) {
        // FX-07: per-user index — content stays shared, the listing
        // never leaves this browser.
        runLocalIndex();
        return;
      }
      const data = await fetchArchive(p);
      state.searching = data.query != null && data.query !== "";
      state.hasMore = data.has_more;
      state.requestId = data.request_id;
      state.rows = data.results.map(rowView);
      state.countLine = countLine(state.rows.length, state.searching);
      state.emptyLine =
        state.rows.length === 0
          ? state.searching
            ? A().empty_filtered.replace("{q}", state.q)
            : A().empty
          : "";
    } catch (e) {
      // No fetch pipeline / no archive feature → the disabled notice,
      // same as the HTMX page's `enabled` arm.
      state.disabled = true;
    } finally {
      state.loading = false;
    }
  }

  async function loadMarkdown(row: ArchiveRowView): Promise<void> {
    if (row.markdown !== "" || row.markdownLoading) return;
    row.markdownLoading = true;
    row.markdownError = "";
    try {
      const page = await fetchPage(row.url);
      row.markdown = page.markdown;
    } catch (e) {
      row.markdownError =
        e instanceof Error ? e.message : A().markdown_failed;
    } finally {
      row.markdownLoading = false;
    }
  }

  async function remove(row: ArchiveRowView): Promise<void> {
    if (!window.confirm(A().confirm_row)) return;
    row.markdownError = "";
    try {
      await deletePage(row.url);
      row.gone = true;
    } catch (e) {
      row.markdownError = e instanceof Error ? e.message : String(e);
    }
  }

  function submit(navigate: (to: string) => void): void {
    const q = state.q.trim();
    navigate("/app/archive" + (q === "" ? "" : "?q=" + encodeURIComponent(q)));
  }

  function pagerUrl(offset: number): string {
    return "/app/archive?offset=" + Math.max(0, offset);
  }

  /** `DELETE /api/pages` is an admin surface — the view hides it. */
  function canDelete(): boolean {
    return capabilities.loaded && capabilities.flags.adminSurface;
  }

  return { state, run, loadMarkdown, remove, canDelete, submit, pagerUrl, LIMIT };
}
