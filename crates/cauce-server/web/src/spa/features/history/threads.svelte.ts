/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * UX-07 `Conversations` section state: the owner's threads from
 * `GET /api/threads`, newest first, with per-row delete. A failed list
 * read (AI persistence off, unauthenticated public instance) leaves
 * the section empty and hidden rather than erroring the whole page.
 */

import { deleteThread, fetchThreads } from "../../lib/api.js";
import { fmtTs } from "../../lib/format.js";
import { fmt, spa } from "../../lib/i18n.js";
import { confirm } from "../../ui/confirm.js";

export interface ThreadRowView {
  id: string;
  title: string;
  updated: string;
  turnsLabel: string;
  resumeUrl: string;
  error: string;
}

export interface ThreadsState {
  rows: ThreadRowView[];
}

export function createThreadsSection() {
  const state: ThreadsState = $state({ rows: [] });

  async function load(): Promise<void> {
    try {
      const rows = await fetchThreads();
      state.rows = rows.map((t) => ({
        id: t.id,
        title: t.title,
        updated: fmtTs(t.updated_at),
        turnsLabel: fmt(spa.history.threads_turns, { n: t.turns.length }),
        resumeUrl: "/app/answer?thread=" + encodeURIComponent(t.id),
        error: "",
      }));
    } catch {
      state.rows = [];
    }
  }

  async function remove(row: ThreadRowView): Promise<void> {
    const ok = await confirm({
      title: spa.history.threads_delete_title,
      description: row.title,
      confirmLabel: spa.history.delete,
      danger: true,
    });
    if (!ok) return;
    try {
      await deleteThread(row.id);
      state.rows = state.rows.filter((r) => r.id !== row.id);
    } catch {
      row.error = spa.history.threads_delete_failed;
    }
  }

  return { state, load, remove };
}
