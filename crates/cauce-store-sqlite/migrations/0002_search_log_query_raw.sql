-- #89: `search_log.query` holds the normalized query (lowercased,
-- whitespace-collapsed), which is what stats grouping and the history `q`
-- filter want — but history displays lost the user's original casing.
-- `query_raw` keeps the query text as submitted; NULL on rows written
-- before this migration.
--
-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at <https://mozilla.org/MPL/2.0/>.

ALTER TABLE search_log ADD COLUMN query_raw TEXT;
