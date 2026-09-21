"""Tests for oxe.stats.build_json, exercised directly against a TTLCache-backed db.

Legacy's test_api_stats.py drove this through the /api/stats HTTP endpoint
(oxe.server); that route is out of scope for this port (oxe/api/ lands in a
later wave-2 step). This file tests the aggregation itself.
"""

import sqlite3
from pathlib import Path

from oxe.cache import SearchLogMeta, TTLCache
from oxe.stats import build, build_json


def _seed(cache: TTLCache) -> None:
    cache.set("hash1", {"_q": "python asyncio", "results": [1, 2]}, ttl=3600)
    cache.log_search(
        "python asyncio",
        "hash1",
        "cache",
        SearchLogMeta(result_count=2, duration_ms=10, client="mcp"),
    )
    cache.log_search(
        "rust tokio",
        "hash2",
        "network",
        SearchLogMeta(result_count=0, duration_ms=200, client="web-ui"),
    )
    cache.log_search(
        "python asyncio",
        "hash1",
        "cache",
        SearchLogMeta(result_count=2, duration_ms=20, client="http"),
    )


def test_stats_shape(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    _seed(cache)
    summary = build_json(cache.db_path)
    assert summary.days == 14
    cache.close()


def test_stats_values(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    _seed(cache)
    summary = build_json(cache.db_path, days=7)
    assert summary.days == 7
    today = sum(d.total for d in summary.searches_per_day)
    assert today == 3
    assert summary.hit_rate.total == 3
    assert summary.hit_rate.cache_hits == 2
    assert summary.hit_rate.rate == 66.7
    top = summary.top_queries[0]
    assert top.query == "python asyncio"
    assert top.count == 2
    assert summary.zero_result_queries[0].query == "rust tokio"
    assert isinstance(summary.zero_result_queries[0].last_seen, int)
    clients = {c.client: c.count for c in summary.client_split}
    assert clients == {"mcp": 1, "web-ui": 1, "http": 1}
    cache.close()


def test_stats_latency_percentiles(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    _seed(cache)
    summary = build_json(cache.db_path)
    lat = summary.latency_ms
    # durations 10, 20, 200 -> p50 = 20
    assert lat.p50 == 20
    assert lat.p99 is not None
    assert lat.p90 is not None
    assert lat.p50 is not None
    assert lat.p99 >= lat.p90 >= lat.p50
    cache.close()


def test_stats_empty_db(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    summary = build_json(cache.db_path)
    assert summary.hit_rate.rate is None
    assert summary.latency_ms.p50 is None
    assert summary.top_queries == []
    cache.close()


def test_stats_cache_empty_db(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    summary = build_json(cache.db_path)
    assert summary.cache.rows == 0
    assert summary.cache.unexpired == 0
    assert summary.cache.total_hits == 0
    assert summary.cache.newest is None
    assert summary.cache.db_size_bytes > 0
    cache.close()


def test_stats_cache_missing_db(tmp_path: Path) -> None:
    summary = build_json(str(tmp_path / "absent.db"))
    assert summary.cache.model_dump() == {
        "rows": 0,
        "unexpired": 0,
        "total_hits": 0,
        "db_size_bytes": 0,
        "newest": None,
    }
    assert not (tmp_path / "absent.db").exists()


def test_stats_cache_mixed_expiry(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    cache.set("live1", {"_q": "a"}, ttl=3600)
    cache.set("live2", {"_q": "b"}, ttl=3600)
    cache.set("dead", {"_q": "c"}, ttl=3600)
    assert cache.get("live1") is not None
    assert cache.get("live1") is not None
    assert cache.get("live2") is not None
    with sqlite3.connect(cache.db_path) as conn:
        conn.execute("UPDATE cache SET expires_at = 1, created_at = 100 WHERE query_hash = 'dead'")
        conn.execute("UPDATE cache SET created_at = 500 WHERE query_hash = 'live1'")
        conn.execute("UPDATE cache SET created_at = 900 WHERE query_hash = 'live2'")
    summary = build_json(cache.db_path)
    assert summary.cache.rows == 3
    assert summary.cache.unexpired == 2
    assert summary.cache.total_hits == 3
    assert summary.cache.newest == 900
    assert summary.cache.db_size_bytes == Path(cache.db_path).stat().st_size
    cache.close()


def test_build_writes_html(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    _seed(cache)
    out_path = build(cache.db_path, str(tmp_path / "out"))
    cache.close()
    content = Path(out_path).read_text(encoding="utf-8")
    assert "<title>oxe stats</title>" in content
