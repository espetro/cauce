-- W4-02 (#51): the `answers` table — cached grounded AI answers, keyed on
-- sha256(normalized query, model). Rows are written by the answer loop only
-- when the caching rule holds (>= 1 source, confidence >= 4, no error) and
-- expire after the loop's answers TTL (24 h); `get_answer` treats
-- `expires_at <= now` as absent. `payload_json` holds the replayable `done`
-- fields (answer, confidence, related_questions); `sources_json` the cited
-- `sources` list.
--
-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

CREATE TABLE answers (
    key          TEXT PRIMARY KEY,
    query        TEXT NOT NULL,
    model        TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    sources_json TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL
);

CREATE INDEX idx_answers_expires ON answers (expires_at);
