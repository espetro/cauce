import gzip
import json
import os
import sqlite3
import threading
import time
from pathlib import Path

SCHEMA = """
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
"""


class TTLCache:
    def __init__(self, db_path: str | os.PathLike[str]):
        self.db_path = str(db_path)
        Path(self.db_path).parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._conn = sqlite3.connect(self.db_path, check_same_thread=False, isolation_level=None)
        self._conn.executescript(SCHEMA)
        self._conn.execute("PRAGMA optimize;")

    def get(self, key: str) -> dict | None:
        now = int(time.time())
        with self._lock:
            row = self._conn.execute(
                "SELECT response, expires_at FROM cache WHERE query_hash = ?", (key,)
            ).fetchone()
        if row is None:
            return None
        blob, expires_at = row
        if expires_at < now:
            return None
        try:
            payload = gzip.decompress(blob)
        except (OSError, gzip.BadGzipFile):
            return None
        with self._lock:
            self._conn.execute(
                "UPDATE cache SET hits = hits + 1 WHERE query_hash = ?", (key,)
            )
        return json.loads(payload)

    def set(self, key: str, value: dict, ttl: int) -> None:
        payload = gzip.compress(json.dumps(value, separators=(",", ":")).encode("utf-8"))
        expires_at = int(time.time()) + ttl
        with self._lock:
            self._conn.execute(
                "INSERT OR REPLACE INTO cache (query_hash, query_text, response, expires_at, hits) "
                "VALUES (?, ?, ?, ?, COALESCE((SELECT hits FROM cache WHERE query_hash = ?), 0))",
                (key, value.get("_q", ""), payload, expires_at, key),
            )
            self._conn.execute("DELETE FROM cache WHERE expires_at < ?", (expires_at - ttl - 1,))

    def invalidate(self) -> int:
        with self._lock:
            cur = self._conn.execute("DELETE FROM cache")
            return cur.rowcount

    def list_rows(
        self,
        q: str | None = None,
        include_expired: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[dict]:
        now = int(time.time())
        clauses: list[str] = []
        params: list = []
        if not include_expired:
            clauses.append("expires_at >= ?")
            params.append(now)
        if q:
            clauses.append("query_text LIKE ?")
            params.append(f"%{q}%")
        where = (" WHERE " + " AND ".join(clauses)) if clauses else ""
        sql = (
            "SELECT query_hash, query_text, expires_at, hits, length(response) "
            f"FROM cache{where} ORDER BY expires_at DESC LIMIT ? OFFSET ?"
        )
        params.extend([limit, offset])
        with self._lock:
            rows = self._conn.execute(sql, params).fetchall()
        return [
            {
                "hash": h,
                "query": qtext,
                "expires_at": exp,
                "hits": hits,
                "size_bytes": size,
                "expired": exp < now,
            }
            for h, qtext, exp, hits, size in rows
        ]

    def peek(self, key: str) -> dict | None:
        return self.get(key)

    def delete(self, key: str) -> bool:
        with self._lock:
            cur = self._conn.execute("DELETE FROM cache WHERE query_hash = ?", (key,))
            return cur.rowcount > 0

    def stats(self) -> dict:
        now = int(time.time())
        with self._lock:
            total = self._conn.execute("SELECT COUNT(*) FROM cache").fetchone()[0]
            unexpired = self._conn.execute(
                "SELECT COUNT(*) FROM cache WHERE expires_at >= ?", (now,)
            ).fetchone()[0]
            total_hits = self._conn.execute("SELECT COALESCE(SUM(hits), 0) FROM cache").fetchone()[0]
            oldest = self._conn.execute(
                "SELECT MIN(expires_at) FROM cache WHERE expires_at >= ?", (now,)
            ).fetchone()[0]
            newest_row = self._conn.execute(
                "SELECT MAX(expires_at) FROM cache"
            ).fetchone()[0]
        db_size = os.path.getsize(self.db_path) if os.path.exists(self.db_path) else 0
        return {
            "rows": total,
            "unexpired_rows": unexpired,
            "db_size_bytes": db_size,
            "total_hits": total_hits,
            "oldest_unexpired": oldest,
            "newest": newest_row,
        }

    def close(self) -> None:
        with self._lock:
            self._conn.close()

    # -- click tracking -----------------------------------------------------

    def record_click(
        self,
        query_hash: str,
        result_id: str,
        url: str,
        title: str,
        source: str = "web",
    ) -> int:
        now = int(time.time())
        with self._lock:
            cur = self._conn.execute(
                "INSERT INTO clicks (query_hash, result_id, url, title, clicked_at, source) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                (query_hash, result_id, url, title, now, source),
            )
            return cur.lastrowid or 0

    def get_clicks(
        self,
        query_hash: str | None = None,
        query_text: str | None = None,
        limit: int = 50,
        since_hours: int | None = None,
    ) -> list[dict]:
        clauses: list[str] = []
        params: list = []
        join = " LEFT JOIN cache k ON k.query_hash = c.query_hash "
        if query_hash:
            clauses.append("c.query_hash = ?")
            params.append(query_hash)
        if query_text:
            clauses.append("k.query_text LIKE ?")
            params.append(f"%{query_text}%")
        if since_hours is not None:
            clauses.append("c.clicked_at >= ?")
            params.append(int(time.time()) - since_hours * 3600)
        where = (" WHERE " + " AND ".join(clauses)) if clauses else ""
        sql = (
            "SELECT c.id, c.query_hash, COALESCE(k.query_text, ''), c.result_id, c.url, "
            "c.title, c.clicked_at, c.source "
            f"FROM clicks c{join}{where} ORDER BY c.clicked_at DESC LIMIT ?"
        )
        params.append(limit)
        with self._lock:
            rows = self._conn.execute(sql, params).fetchall()
        return [
            {
                "id": cid,
                "query_hash": qh,
                "query": qt,
                "result_id": rid,
                "url": url,
                "title": title,
                "clicked_at": ts,
                "source": src,
            }
            for cid, qh, qt, rid, url, title, ts, src in rows
        ]

    def click_stats(self) -> dict:
        now = int(time.time())
        with self._lock:
            total = self._conn.execute("SELECT COUNT(*) FROM clicks").fetchone()[0]
            last_24h = self._conn.execute(
                "SELECT COUNT(*) FROM clicks WHERE clicked_at >= ?", (now - 86400,)
            ).fetchone()[0]
            oldest = self._conn.execute("SELECT MIN(clicked_at) FROM clicks").fetchone()[0]
        return {"total": total, "last_24h": last_24h, "oldest": oldest}

    def prune_clicks(self, retention_days: int) -> int:
        cutoff = int(time.time()) - retention_days * 86400
        with self._lock:
            cur = self._conn.execute("DELETE FROM clicks WHERE clicked_at < ?", (cutoff,))
            return cur.rowcount

    # -- search log ---------------------------------------------------------

    def log_search(
        self,
        query_text: str,
        query_hash: str,
        source: str,
        backend: str = "ddg",
        result_count: int = 0,
        duration_ms: int | None = None,
        client: str = "http",
    ) -> int:
        now = int(time.time())
        with self._lock:
            cur = self._conn.execute(
                "INSERT INTO search_log (ts, query_text, query_hash, source, backend, "
                "result_count, duration_ms, client) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    now,
                    (query_text or "")[:200],
                    query_hash,
                    source,
                    backend,
                    result_count,
                    duration_ms,
                    client,
                ),
            )
            return cur.lastrowid or 0

    def get_search_log(self, limit: int = 100) -> list[dict]:
        with self._lock:
            rows = self._conn.execute(
                "SELECT id, ts, query_text, query_hash, source, backend, result_count, "
                "duration_ms, client FROM search_log ORDER BY ts DESC LIMIT ?",
                (limit,),
            ).fetchall()
        return [
            {
                "id": rid,
                "ts": ts,
                "query": qt,
                "query_hash": qh,
                "source": src,
                "backend": be,
                "result_count": rc,
                "duration_ms": dm,
                "client": cl,
            }
            for rid, ts, qt, qh, src, be, rc, dm, cl in rows
        ]

    def lookup_query_text(self, query_hash: str) -> str | None:
        """Best-effort original query text for a hash, from cache or search_log."""
        with self._lock:
            row = self._conn.execute(
                "SELECT query_text FROM cache WHERE query_hash = ?", (query_hash,)
            ).fetchone()
            if row is None:
                row = self._conn.execute(
                    "SELECT query_text FROM search_log WHERE query_hash = ? "
                    "ORDER BY ts DESC LIMIT 1",
                    (query_hash,),
                ).fetchone()
        return row[0] if row else None

    def prune_search_log(self, retention_days: int) -> int:
        cutoff = int(time.time()) - retention_days * 86400
        with self._lock:
            cur = self._conn.execute("DELETE FROM search_log WHERE ts < ?", (cutoff,))
            return cur.rowcount

    def delete_clicks(self, scope: str) -> int:
        """scope: '24h' deletes last 24h, 'all' deletes everything."""
        with self._lock:
            if scope == "all":
                cur = self._conn.execute("DELETE FROM clicks")
            elif scope == "24h":
                cur = self._conn.execute(
                    "DELETE FROM clicks WHERE clicked_at >= ?", (int(time.time()) - 86400,)
                )
            else:
                return 0
            return cur.rowcount
