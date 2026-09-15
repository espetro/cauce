PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -32000;
PRAGMA temp_store = MEMORY;
PRAGMA busy_timeout = 5000;

CREATE TABLE IF NOT EXISTS cache (
  query_hash  TEXT PRIMARY KEY,
  query_text  TEXT NOT NULL,
  response    BLOB NOT NULL,
  expires_at  INTEGER NOT NULL,
  hits        INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS expires_idx ON cache(expires_at);

CREATE TABLE IF NOT EXISTS clicks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  query_hash TEXT    NOT NULL,
  result_id  TEXT    NOT NULL,
  url        TEXT    NOT NULL,
  title      TEXT    NOT NULL,
  clicked_at INTEGER NOT NULL,
  source     TEXT    NOT NULL DEFAULT 'web'
);
CREATE INDEX IF NOT EXISTS clicks_query_idx  ON clicks(query_hash);
CREATE INDEX IF NOT EXISTS clicks_recent_idx ON clicks(clicked_at);

CREATE TABLE IF NOT EXISTS search_log (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  ts           INTEGER NOT NULL,
  query_text   TEXT    NOT NULL,
  query_hash   TEXT    NOT NULL,
  source       TEXT    NOT NULL,
  backend      TEXT    NOT NULL DEFAULT 'ddg',
  result_count INTEGER NOT NULL DEFAULT 0,
  duration_ms  INTEGER,
  client       TEXT    NOT NULL DEFAULT 'http'
);
CREATE INDEX IF NOT EXISTS search_log_ts_idx   ON search_log(ts);
CREATE INDEX IF NOT EXISTS search_log_query_idx ON search_log(query_hash);
