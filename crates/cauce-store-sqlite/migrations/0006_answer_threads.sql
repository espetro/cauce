-- UX-01: server-authoritative answer threads (`answer_thread`).
--
-- One row per conversation: the SPA keeps the live transcript in memory and
-- sends the full `history[]` on every turn; the server upserts the completed
-- turn on the terminal `done` frame. `turns_json` is the completed
-- `AnswerTurn[]` list (strict user/assistant pairs, oldest first) — the
-- same shape `POST /api/answer` replays, so hydration and the next replay
-- read one field.
--
-- `owner` is the isolation key: `"local"` in local mode (the single local
-- user), a SHA-256 hash of the bearer identity in public mode — never the
-- raw token. Every read filters by owner, so a public instance never
-- serves another user's thread.
--
-- This table is the conversation; `answer_log` stays the durable
-- single-answer artifact (per-run rows, no owner column).
--
-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

CREATE TABLE answer_thread (
    id          TEXT PRIMARY KEY,        -- UUIDv4, minted by the client's first turn
    owner       TEXT NOT NULL,
    title       TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    turns_json  TEXT NOT NULL
);
CREATE INDEX idx_answer_thread_owner_updated
    ON answer_thread (owner, updated_at DESC);
