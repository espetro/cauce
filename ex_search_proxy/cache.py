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
