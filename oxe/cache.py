import gzip
import json
import os
import sqlite3
import threading
import time
from pathlib import Path

from . import sqlload

# Faster-than-LIKE fuzzy matching for history/cache search; optional dep.
try:
    from rapidfuzz import fuzz, process as rz_process

    _FUZZ = True
except ImportError:
    _FUZZ = False

FUZZ_THRESHOLD = 72


class TTLCache:
    def __init__(self, db_path: str | os.PathLike[str]):
        self.db_path = str(db_path)
        Path(self.db_path).parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._conn = sqlite3.connect(self.db_path, check_same_thread=False, isolation_level=None)
        self._conn.row_factory = sqlite3.Row
        self._conn.executescript(sqlload.schema_sql())
        try:
            self._conn.execute("ALTER TABLE cache ADD COLUMN created_at INTEGER")
        except sqlite3.OperationalError:
            pass  # column already exists
        self._conn.execute("PRAGMA optimize;")
        self._all_queries: list[str] | None = None

    def _q(self, name):
        return getattr(sqlload.queries(), name)

    # -- cache table --------------------------------------------------------

    def get(self, key: str) -> dict | None:
        row = self._q("get_cache")(self._conn, key=key)
        if row is None or row["expires_at"] < int(time.time()):
            return None
        try:
            payload = gzip.decompress(row["response"])
        except (OSError, gzip.BadGzipFile):
            return None
        self._q("hits_bump")(self._conn, key=key)
        return json.loads(payload)

    def set(self, key: str, value: dict, ttl: int) -> None:
        payload = gzip.compress(json.dumps(value, separators=(",", ":")).encode("utf-8"))
        expires_at = int(time.time()) + ttl
        with self._lock:
            self._q("put_cache")(
                self._conn,
                key=key,
                text=value.get("_q", ""),
                response=payload,
                expires_at=expires_at,
                created_at=int(time.time()),
            )
            self._conn.execute("DELETE FROM cache WHERE expires_at < ?", (expires_at - ttl - 1,))

    def get_with_meta(self, key: str) -> tuple[dict, int | None] | None:
        """Like get() but returns (payload, created_at). No hits bump avoided: counts too."""
        row = self._q("get_cache")(self._conn, key=key)
        if row is None or row["expires_at"] < int(time.time()):
            return None
        try:
            payload = gzip.decompress(row["response"])
        except (OSError, gzip.BadGzipFile):
            return None
        self._q("hits_bump")(self._conn, key=key)
        return json.loads(payload), row["created_at"]

    def invalidate(self) -> int:
        with self._lock:
            cur = self._conn.execute("DELETE FROM cache")
            return cur.rowcount

    def _fuzzy_query_ids(self, q: str) -> list[str]:
        """Hashes of queries close to q; rapidfuzz when available, difflib otherwise."""
        if not q:
            return []
        if self._all_queries is None:
            rows = self._conn.execute("SELECT DISTINCT query_text FROM cache").fetchall()
            self._all_queries = [r["query_text"] for r in rows]
        if _FUZZ:
            matches = rz_process.extract(
                q, self._all_queries, scorer=fuzz.WRatio, score_cutoff=FUZZ_THRESHOLD, limit=8
            )
            texts = [text for text, _s, _i in matches]
        else:
            import difflib

            texts = difflib.get_close_matches(q, self._all_queries, n=8, cutoff=0.6)
        if not texts:
            return []
        ids: list[str] = []
        for text in texts:
            for r in self._conn.execute(
                "SELECT query_hash FROM cache WHERE query_text = ? LIMIT 3", (text,)
            ):
                ids.append(r["query_hash"])
        return ids

    def list_rows(
        self,
        q: str | None = None,
        include_expired: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[dict]:
        now = 0 if include_expired else int(time.time())
        fuzzy_ids = None
        if q:
            ids = self._fuzzy_query_ids(q)
            fuzzy_ids = json.dumps(ids) if ids else None
        like = f"%{q}%" if q else None
        rows = self._q("list_rows")(
            self._conn,
            now=now,
            q=like,
            fuzzy_ids=fuzzy_ids,
            limit=limit,
            offset=offset,
        )
        return [
            {
                "hash": r["hash"],
                "query": r["query"],
                "expires_at": r["expires_at"],
                "hits": r["hits"],
                "size_bytes": r["size_bytes"],
                "expired": r["expires_at"] < now if now else r["expires_at"] < int(time.time()),
            }
            for r in rows
        ]

    def peek(self, key: str) -> dict | None:
        return self.get(key)

    def delete(self, key: str) -> bool:
        with self._lock:
            cur = self._conn.execute("DELETE FROM cache WHERE query_hash = ?", (key,))
            self._all_queries = None
            return cur.rowcount > 0

    def stats(self) -> dict:
        row = self._q("cache_stats")(self._conn, now=int(time.time()))
        db_size = os.path.getsize(self.db_path) if os.path.exists(self.db_path) else 0
        return {
            "rows": row["rows"],
            "unexpired_rows": row["unexpired_rows"],
            "db_size_bytes": db_size,
            "total_hits": row["total_hits"],
            "oldest_unexpired": row["oldest_unexpired"],
            "newest": row["newest"],
        }

    def close(self) -> None:
        self._conn.close()

    # -- answers table (completed AI answers) --------------------------------

    def get_answer(self, key: str) -> dict | None:
        row = self._q("get_answer")(self._conn, key=key)
        if row is None or row["expires_at"] < int(time.time()):
            return None
        try:
            payload = gzip.decompress(row["response"])
        except (OSError, gzip.BadGzipFile):
            return None
        self._q("answer_hits_bump")(self._conn, key=key)
        return json.loads(payload)

    def put_answer(self, key: str, query: str, value: dict, ttl: int, model: str = "") -> None:
        payload = gzip.compress(json.dumps(value, separators=(",", ":")).encode("utf-8"))
        now = int(time.time())
        with self._lock:
            self._q("put_answer")(
                self._conn,
                key=key,
                text=(query or "")[:200],
                response=payload,
                model=model,
                created_at=now,
                expires_at=now + ttl,
            )
            self._q("prune_answers")(self._conn, now=now)

    def answer_stats(self) -> dict:
        db_size = os.path.getsize(self.db_path) if os.path.exists(self.db_path) else 0
        try:
            n = self._q("count_answers")(self._conn)["c"]
        except Exception:
            n = 0
        return {"rows": n, "db_size_bytes": db_size}

    # -- clicks -------------------------------------------------------------

    def record_click(
        self,
        query_hash: str,
        result_id: str,
        url: str,
        title: str,
        source: str = "web",
    ) -> int:
        with self._lock:
            cur = self._q("record_click")(
                self._conn,
                hash=query_hash,
                result_id=result_id,
                url=url,
                title=title,
                clicked_at=int(time.time()),
                source=source,
            )
            return cur or 0

    def get_clicks(
        self,
        query_hash: str | None = None,
        query_text: str | None = None,
        limit: int = 50,
        since_hours: int | None = None,
    ) -> list[dict]:
        since = (
            0 if since_hours is None else int(time.time()) - int(since_hours) * 3600
        )
        like = f"%{query_text}%" if query_text else None
        rows = self._q("get_clicks")(
            self._conn,
            hash=query_hash,
            q=like,
            since=since,
            limit=limit,
        )
        return [
            {
                "id": r["id"],
                "query_hash": r["query_hash"],
                "query": r["query"],
                "result_id": r["result_id"],
                "url": r["url"],
                "title": r["title"],
                "clicked_at": r["clicked_at"],
                "source": r["source"],
            }
            for r in rows
        ]

    def click_stats(self) -> dict:
        row = self._q("click_stats")(self._conn, cutoff=int(time.time()) - 86400)
        return {
            "total": row["total"],
            "last_24h": row["last_24h"] or 0,
            "oldest": row["oldest"],
        }

    def prune_clicks(self, retention_days: int) -> int:
        cutoff = int(time.time()) - retention_days * 86400
        with self._lock:
            cur = self._q("prune_clicks")(self._conn, cutoff=cutoff)
            return cur

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
        with self._lock:
            cur = self._q("log_search")(
                self._conn,
                ts=int(time.time()),
                query_text=(query_text or "")[:200],
                query_hash=query_hash,
                source=source,
                backend=backend,
                result_count=result_count,
                duration_ms=duration_ms,
                client=client,
            )
            return cur or 0

    def suggest_queries(self, prefix: str, limit: int = 3) -> list[str]:
        """Recency-then-frequency ranked unique queries starting with prefix."""
        esc = (
            (prefix or "")
            .replace("\\", "\\\\")
            .replace("%", "\\%")
            .replace("_", "\\_")
            .lower()
        )
        rows = self._q("suggest_queries")(self._conn, prefix=f"{esc}%", limit=limit)
        return [r["query_text"] for r in rows]

    def get_search_log(self, limit: int = 100) -> list[dict]:
        rows = self._q("get_search_log")(self._conn, limit=limit)
        return [
            {
                "id": r["id"],
                "ts": r["ts"],
                "query": r["query_text"],
                "query_hash": r["query_hash"],
                "source": r["source"],
                "backend": r["backend"],
                "result_count": r["result_count"],
                "duration_ms": r["duration_ms"],
                "client": r["client"],
            }
            for r in rows
        ]

    def lookup_query_text(self, query_hash: str) -> str | None:
        """Best-effort original query text for a hash, from cache or search_log."""
        row = self._q("lookup_query_text_cache")(self._conn, key=query_hash)
        if row is None:
            row = self._q("lookup_query_text_log")(self._conn, key=query_hash)
        return row["query_text"] if row else None

    def prune_search_log(self, retention_days: int) -> int:
        cutoff = int(time.time()) - retention_days * 86400
        with self._lock:
            cur = self._q("prune_search_log")(self._conn, cutoff=cutoff)
            return cur

    def delete_clicks(self, scope: str) -> int:
        """scope: '24h' deletes last 24h, 'all' deletes everything."""
        with self._lock:
            if scope == "all":
                cur = self._q("delete_clicks_all")(self._conn)
            elif scope == "24h":
                cur = self._q("delete_clicks_since")(
                    self._conn, since=int(time.time()) - 86400
                )
            else:
                return 0
            return cur
