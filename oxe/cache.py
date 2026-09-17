"""SQLite-backed TTL cache: search results, AI answers, clicks, search log.

Ported from legacy ``oxe/cache.py``, tightened to the data and error ladders
in ``oxe/AGENTS.md``:

- The migration that used to swallow every ``sqlite3.OperationalError`` now
  checks ``PRAGMA table_info`` explicitly before altering the table, so a
  genuinely broken migration surfaces instead of being silently discarded.
- Boundary return values (rows, stats) are ``NamedTuple``s, not bare ``dict``.
- ``aiosql``'s dynamically-generated query object is untyped; ``cast()`` is
  used at the single point each query result is consumed, converting it into
  a concretely typed value immediately rather than letting ``Any`` leak.
"""

import gzip
import json
import os
import sqlite3
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import NamedTuple, cast

from rapidfuzz import fuzz
from rapidfuzz import process as rz_process

from . import sqlload
from .jsontypes import JSONDict

FUZZ_THRESHOLD = 72


class CacheRow(NamedTuple):
    hash: str
    query: str
    expires_at: int
    created_at: int | None
    hits: int
    size_bytes: int
    expired: bool


class CacheStats(NamedTuple):
    rows: int
    unexpired_rows: int
    db_size_bytes: int
    total_hits: int
    oldest_unexpired: int | None
    newest: int | None


class AnswerStats(NamedTuple):
    rows: int
    db_size_bytes: int


class ClickRow(NamedTuple):
    id: int
    query_hash: str
    query: str
    result_id: str
    url: str
    title: str
    clicked_at: int
    source: str


class ClickStats(NamedTuple):
    total: int
    last_24h: int
    oldest: int | None


@dataclass(frozen=True)
class SearchLogMeta:
    """Optional log_search fields, grouped to keep the call signature under
    the max-args cap rather than growing one keyword parameter at a time."""

    backend: str = "ddg"
    result_count: int = 0
    duration_ms: int | None = None
    client: str = "http"


class SearchLogRow(NamedTuple):
    id: int
    ts: int
    query: str
    query_hash: str
    source: str
    backend: str
    result_count: int
    duration_ms: int | None
    client: str


def _has_created_at_column(conn: sqlite3.Connection) -> bool:
    """True if ``cache.created_at`` already exists.

    Replaces the legacy ``except sqlite3.OperationalError: pass`` migration
    guard: that pattern also swallowed genuine schema-corruption errors, not
    just "column already exists". Checking ``PRAGMA table_info`` explicitly
    means only the one expected case is handled, and anything else raises.
    """
    rows = conn.execute("PRAGMA table_info(cache)").fetchall()
    return any(cast(str, row[1]) == "created_at" for row in rows)


class TTLCache:
    def __init__(self, db_path: str | os.PathLike[str]) -> None:
        self.db_path = str(db_path)
        Path(self.db_path).parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._conn = sqlite3.connect(self.db_path, check_same_thread=False, isolation_level=None)
        self._conn.row_factory = sqlite3.Row
        self._conn.executescript(sqlload.schema_sql())
        if not _has_created_at_column(self._conn):
            self._conn.execute("ALTER TABLE cache ADD COLUMN created_at INTEGER")
        self._conn.execute("PRAGMA optimize;")
        self._all_queries: list[str] | None = None

    # -- cache table --------------------------------------------------------

    def get(self, key: str) -> JSONDict | None:
        row = cast(sqlite3.Row | None, sqlload.query("get_cache")(self._conn, key=key))
        if row is None or cast(int, row["expires_at"]) < int(time.time()):
            return None
        try:
            payload = gzip.decompress(cast(bytes, row["response"]))
        except (OSError, gzip.BadGzipFile):
            return None
        sqlload.query("hits_bump")(self._conn, key=key)
        return cast(JSONDict, json.loads(payload))

    def set(self, key: str, value: JSONDict, ttl: int) -> None:
        payload = gzip.compress(json.dumps(value, separators=(",", ":")).encode("utf-8"))
        expires_at = int(time.time()) + ttl
        text = value.get("_q", "")
        text_str = text if isinstance(text, str) else ""
        with self._lock:
            sqlload.query("put_cache")(
                self._conn,
                key=key,
                text=text_str,
                response=payload,
                expires_at=expires_at,
                created_at=int(time.time()),
            )
            self._conn.execute("DELETE FROM cache WHERE expires_at < ?", (expires_at - ttl - 1,))

    def get_with_meta(self, key: str) -> tuple[JSONDict, int | None] | None:
        """Like get() but returns (payload, created_at). Still bumps hits."""
        row = cast(sqlite3.Row | None, sqlload.query("get_cache")(self._conn, key=key))
        if row is None or cast(int, row["expires_at"]) < int(time.time()):
            return None
        try:
            payload = gzip.decompress(cast(bytes, row["response"]))
        except (OSError, gzip.BadGzipFile):
            return None
        sqlload.query("hits_bump")(self._conn, key=key)
        return cast(JSONDict, json.loads(payload)), cast(int | None, row["created_at"])

    def invalidate(self) -> int:
        with self._lock:
            n = self._conn.execute("DELETE FROM cache").rowcount
            n += self._conn.execute("DELETE FROM answers").rowcount
            return n

    def _fuzzy_query_ids(self, q: str) -> list[str]:
        """Hashes of queries close to q, via rapidfuzz."""
        if not q:
            return []
        if self._all_queries is None:
            rows = self._conn.execute("SELECT DISTINCT query_text FROM cache").fetchall()
            self._all_queries = [cast(str, r["query_text"]) for r in rows]
        matches = rz_process.extract(
            q, self._all_queries, scorer=fuzz.WRatio, score_cutoff=FUZZ_THRESHOLD, limit=8
        )
        texts = [text for text, _score, _idx in matches]
        if not texts:
            return []
        ids: list[str] = []
        for text in texts:
            for r in self._conn.execute(
                "SELECT query_hash FROM cache WHERE query_text = ? LIMIT 3", (text,)
            ):
                ids.append(cast(str, r["query_hash"]))
        return ids

    def list_rows(
        self,
        *,
        q: str | None = None,
        include_expired: bool = False,
        limit: int = 100,
        offset: int = 0,
    ) -> list[CacheRow]:
        now = 0 if include_expired else int(time.time())
        fuzzy_ids = None
        if q:
            ids = self._fuzzy_query_ids(q)
            fuzzy_ids = json.dumps(ids) if ids else None
        like = f"%{q}%" if q else None
        rows = cast(
            list[sqlite3.Row],
            sqlload.query("list_rows")(
                self._conn, now=now, q=like, fuzzy_ids=fuzzy_ids, limit=limit, offset=offset
            ),
        )
        cutoff = now if now else int(time.time())
        return [
            CacheRow(
                hash=cast(str, r["hash"]),
                query=cast(str, r["query"]),
                expires_at=cast(int, r["expires_at"]),
                created_at=cast(int | None, r["created_at"]),
                hits=cast(int, r["hits"]),
                size_bytes=cast(int, r["size_bytes"]),
                expired=cast(int, r["expires_at"]) < cutoff,
            )
            for r in rows
        ]

    def peek(self, key: str) -> JSONDict | None:
        return self.get(key)

    def delete(self, key: str) -> bool:
        with self._lock:
            cur = self._conn.execute("DELETE FROM cache WHERE query_hash = ?", (key,))
            self._all_queries = None
            return cur.rowcount > 0

    def stats(self) -> CacheStats:
        row = cast(sqlite3.Row, sqlload.query("cache_stats")(self._conn, now=int(time.time())))
        db_file = Path(self.db_path)
        db_size = db_file.stat().st_size if db_file.exists() else 0
        return CacheStats(
            rows=cast(int, row["rows"]),
            unexpired_rows=cast(int, row["unexpired_rows"]),
            db_size_bytes=db_size,
            total_hits=cast(int, row["total_hits"]),
            oldest_unexpired=cast(int | None, row["oldest_unexpired"]),
            newest=cast(int | None, row["newest"]),
        )

    def close(self) -> None:
        self._conn.close()

    # -- answers table (completed AI answers) --------------------------------

    def get_answer(self, key: str) -> JSONDict | None:
        row = cast(sqlite3.Row | None, sqlload.query("get_answer")(self._conn, key=key))
        if row is None or cast(int, row["expires_at"]) < int(time.time()):
            return None
        try:
            payload = gzip.decompress(cast(bytes, row["response"]))
        except (OSError, gzip.BadGzipFile):
            return None
        sqlload.query("answer_hits_bump")(self._conn, key=key)
        return cast(JSONDict, json.loads(payload))

    def put_answer(self, key: str, query: str, value: JSONDict, ttl: int, model: str = "") -> None:
        payload = gzip.compress(json.dumps(value, separators=(",", ":")).encode("utf-8"))
        now = int(time.time())
        with self._lock:
            sqlload.query("put_answer")(
                self._conn,
                key=key,
                text=(query or "")[:200],
                response=payload,
                model=model,
                created_at=now,
                expires_at=now + ttl,
            )
            sqlload.query("prune_answers")(self._conn, now=now)

    def answer_stats(self) -> AnswerStats:
        db_file = Path(self.db_path)
        db_size = db_file.stat().st_size if db_file.exists() else 0
        n = cast(int, self._conn.execute("SELECT COUNT(*) FROM answers").fetchone()[0])
        return AnswerStats(rows=n, db_size_bytes=db_size)

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
            cur = cast(
                int | None,
                sqlload.query("record_click")(
                    self._conn,
                    hash=query_hash,
                    result_id=result_id,
                    url=url,
                    title=title,
                    clicked_at=int(time.time()),
                    source=source,
                ),
            )
            return cur or 0

    def get_clicks(
        self,
        query_hash: str | None = None,
        query_text: str | None = None,
        limit: int = 50,
        since_hours: int | None = None,
    ) -> list[ClickRow]:
        since = 0 if since_hours is None else int(time.time()) - int(since_hours) * 3600
        like = f"%{query_text}%" if query_text else None
        rows = cast(
            list[sqlite3.Row],
            sqlload.query("get_clicks")(
                self._conn, hash=query_hash, q=like, since=since, limit=limit
            ),
        )
        return [
            ClickRow(
                id=cast(int, r["id"]),
                query_hash=cast(str, r["query_hash"]),
                query=cast(str, r["query"]),
                result_id=cast(str, r["result_id"]),
                url=cast(str, r["url"]),
                title=cast(str, r["title"]),
                clicked_at=cast(int, r["clicked_at"]),
                source=cast(str, r["source"]),
            )
            for r in rows
        ]

    def click_stats(self) -> ClickStats:
        row = cast(
            sqlite3.Row, sqlload.query("click_stats")(self._conn, cutoff=int(time.time()) - 86400)
        )
        return ClickStats(
            total=cast(int, row["total"]),
            last_24h=cast(int | None, row["last_24h"]) or 0,
            oldest=cast(int | None, row["oldest"]),
        )

    def prune_clicks(self, retention_days: int) -> int:
        cutoff = int(time.time()) - retention_days * 86400
        with self._lock:
            return cast(int, sqlload.query("prune_clicks")(self._conn, cutoff=cutoff))

    # -- search log ---------------------------------------------------------

    def log_search(
        self,
        query_text: str,
        query_hash: str,
        source: str,
        meta: SearchLogMeta | None = None,
    ) -> int:
        m = meta or SearchLogMeta()
        with self._lock:
            cur = cast(
                int | None,
                sqlload.query("log_search")(
                    self._conn,
                    ts=int(time.time()),
                    query_text=(query_text or "")[:200],
                    query_hash=query_hash,
                    source=source,
                    backend=m.backend,
                    result_count=m.result_count,
                    duration_ms=m.duration_ms,
                    client=m.client,
                ),
            )
            return cur or 0

    def suggest_queries(self, prefix: str, limit: int = 3) -> list[str]:
        """Recency-then-frequency ranked unique queries starting with prefix."""
        esc = (prefix or "").replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_").lower()
        rows = cast(
            list[sqlite3.Row],
            sqlload.query("suggest_queries")(self._conn, prefix=f"{esc}%", limit=limit),
        )
        return [cast(str, r["query_text"]) for r in rows]

    def get_search_log(self, limit: int = 100) -> list[SearchLogRow]:
        rows = cast(list[sqlite3.Row], sqlload.query("get_search_log")(self._conn, limit=limit))
        return [
            SearchLogRow(
                id=cast(int, r["id"]),
                ts=cast(int, r["ts"]),
                query=cast(str, r["query_text"]),
                query_hash=cast(str, r["query_hash"]),
                source=cast(str, r["source"]),
                backend=cast(str, r["backend"]),
                result_count=cast(int, r["result_count"]),
                duration_ms=cast(int | None, r["duration_ms"]),
                client=cast(str, r["client"]),
            )
            for r in rows
        ]

    def lookup_query_text(self, query_hash: str) -> str | None:
        """Best-effort original query text for a hash, from cache or search_log."""
        row = cast(
            sqlite3.Row | None,
            sqlload.query("lookup_query_text_cache")(self._conn, key=query_hash),
        )
        if row is None:
            row = cast(
                sqlite3.Row | None,
                sqlload.query("lookup_query_text_log")(self._conn, key=query_hash),
            )
        return cast(str, row["query_text"]) if row is not None else None

    def prune_search_log(self, retention_days: int) -> int:
        cutoff = int(time.time()) - retention_days * 86400
        with self._lock:
            return cast(int, sqlload.query("prune_search_log")(self._conn, cutoff=cutoff))

    def delete_clicks(self, scope: str) -> int:
        """scope: '24h' deletes last 24h, 'all' deletes everything."""
        with self._lock:
            if scope == "all":
                return cast(int, sqlload.query("delete_clicks_all")(self._conn))
            if scope == "24h":
                return cast(
                    int,
                    sqlload.query("delete_clicks_since")(
                        self._conn, since=int(time.time()) - 86400
                    ),
                )
            return 0
