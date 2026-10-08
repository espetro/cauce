-- #254: AI-mode history — `search_log.origin` labels who originated a
-- search, and `answer_log` durably records every terminal answer run
-- (the `answers` table stays the 24h replay cache; this is the log).
--
-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

-- `user` = typed into the UI (client = 'ui' and not a tool-loop call);
-- `agent` = answer-loop tool calls and all api/mcp/cli clients.
-- Rows written before this column existed can't be proven agent, so
-- 'user' is the honest backfill.
ALTER TABLE search_log ADD COLUMN origin TEXT NOT NULL DEFAULT 'user';

CREATE TABLE answer_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    ts            INTEGER NOT NULL,
    query         TEXT NOT NULL,
    query_raw     TEXT,
    model         TEXT NOT NULL,
    answer        TEXT NOT NULL DEFAULT '',
    confidence    INTEGER,
    sources_json  TEXT NOT NULL,
    related_json  TEXT NOT NULL DEFAULT '[]',
    request_id    TEXT,
    client        TEXT NOT NULL,
    origin        TEXT NOT NULL,
    status        TEXT NOT NULL,           -- 'done' | 'cached' | 'error'
    ungrounded    INTEGER NOT NULL DEFAULT 0,
    error         TEXT
);
CREATE INDEX idx_answer_log_ts ON answer_log (ts);
