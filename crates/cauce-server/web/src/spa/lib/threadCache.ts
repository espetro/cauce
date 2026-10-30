/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * UX-02 client copy of a thread transcript (`localStorage`,
 * `cauce:thread:<id>`). The server row is authoritative; this copy is
 * the fallback when `GET /api/threads/{id}` fails (offline, unauth'd
 * public instance, AI persistence off) so a reload still shows the
 * conversation. Same quota/private-mode stance as `localHistory.ts`:
 * failures degrade to "not persisted".
 */

import type { AnswerTurn } from "../../types/AnswerTurn.js";

const PREFIX = "cauce:thread:";

export function cacheThread(id: string, turns: AnswerTurn[]): void {
  try {
    localStorage.setItem(PREFIX + id, JSON.stringify(turns));
  } catch {
    /* degrade to not persisted */
  }
}

export function cachedThread(id: string): AnswerTurn[] | null {
  try {
    const raw = localStorage.getItem(PREFIX + id);
    if (raw === null) return null;
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as AnswerTurn[]) : null;
  } catch {
    return null;
  }
}
