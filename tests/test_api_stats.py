"""Tests for the GET /api/stats endpoint."""

import pytest
from fastapi.testclient import TestClient

from oxe.cache import TTLCache
from oxe.server import make_app


@pytest.fixture
def cache(tmp_path):
    return TTLCache(str(tmp_path / "db"))


@pytest.fixture
def client(cache):
    app = make_app(cache=cache, backend=object())
    return TestClient(app)


def _seed(cache):
    cache.set("hash1", {"_q": "python asyncio", "results": [1, 2]}, ttl=3600)
    cache.log_search(
        "python asyncio", "hash1", "cache", result_count=2, duration_ms=10,
        client="mcp",
    )
    cache.log_search(
        "rust tokio", "hash2", "network", result_count=0, duration_ms=200,
        client="web-ui",
    )
    cache.log_search("python asyncio", "hash1", "cache", result_count=2,
                     duration_ms=20, client="http")


def test_stats_shape(client, cache):
    _seed(cache)
    res = client.get("/api/stats")
    assert res.status_code == 200
    body = res.json()
    assert body["days"] == 14
    for key in (
        "searches_per_day",
        "hit_rate",
        "latency_ms",
        "top_queries",
        "zero_result_queries",
        "client_split",
        "cache",
    ):
        assert key in body


def test_stats_values(client, cache):
    _seed(cache)
    body = client.get("/api/stats", params={"days": 7}).json()
    assert body["days"] == 7
    today = sum(d["total"] for d in body["searches_per_day"])
    assert today == 3
    assert body["hit_rate"]["total"] == 3
    assert body["hit_rate"]["cache_hits"] == 2
    assert body["hit_rate"]["rate"] == 66.7
    top = body["top_queries"][0]
    assert top == {"query": "python asyncio", "count": 2}
    assert body["zero_result_queries"][0]["query"] == "rust tokio"
    assert isinstance(body["zero_result_queries"][0]["last_seen"], int)
    clients = {c["client"]: c["count"] for c in body["client_split"]}
    assert clients == {"mcp": 1, "web-ui": 1, "http": 1}


def test_stats_latency_percentiles(client, cache):
    _seed(cache)
    body = client.get("/api/stats").json()
    lat = body["latency_ms"]
    # durations 10, 20, 200 -> p50 = 20
    assert lat["p50"] == 20
    assert lat["p99"] >= lat["p90"] >= lat["p50"]


def test_stats_cache_section(client, cache):
    _seed(cache)
    cachec = client.get("/api/stats").json()["cache"]
    assert cachec["rows"] == 1
    assert cachec["unexpired_rows"] == 1
    assert cachec["db_size_bytes"] > 0


def test_stats_empty_db(client):
    body = client.get("/api/stats").json()
    assert body["hit_rate"]["rate"] is None
    assert body["latency_ms"]["p50"] is None
    assert body["top_queries"] == []
    assert body["cache"]["rows"] == 0


def test_stats_days_validation(client):
    assert client.get("/api/stats", params={"days": 0}).status_code == 422
    assert client.get("/api/stats", params={"days": 91}).status_code == 422
