"""Tests for oxe.stats.build_json, exercised directly against a TTLCache-backed db.

Legacy's test_api_stats.py drove this through the /api/stats HTTP endpoint
(oxe.server); that route is out of scope for this port (oxe/api/ lands in a
later wave-2 step). This file tests the aggregation itself.
"""

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


def test_build_writes_html(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    _seed(cache)
    out_path = build(cache.db_path, str(tmp_path / "out"))
    cache.close()
    content = Path(out_path).read_text(encoding="utf-8")
    assert "<title>oxe stats</title>" in content
