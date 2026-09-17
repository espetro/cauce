"""Tests for ``GET /api/stats`` (``oxe/api/dashboard.py``)."""

from pathlib import Path

from fastapi.testclient import TestClient

from oxe.app import create_app
from oxe.cache import SearchLogMeta, TTLCache


def test_dashboard_empty_cache_has_no_hit_rate(tmp_path: Path) -> None:
    app = create_app()
    app.state.cache = TTLCache(tmp_path / "db")
    client = TestClient(app)

    response = client.get("/api/stats")

    assert response.status_code == 200
    body = response.json()
    assert body["cache"]["rows"] == 0
    assert body["hit_rate_pct"] is None
    assert body["log"]["top_queries"] == []


def test_dashboard_reflects_cache_rows_and_hit_rate(tmp_path: Path) -> None:
    app = create_app()
    cache = TTLCache(tmp_path / "db")
    cache.set("hash1", {"_q": "python asyncio"}, ttl=3600)
    cache.set("hash2", {"_q": "rust tokio"}, ttl=3600)
    app.state.cache = cache
    client = TestClient(app)

    response = client.get("/api/stats")

    assert response.status_code == 200
    body = response.json()
    assert body["cache"]["rows"] == 2
    assert body["cache"]["unexpired_rows"] == 2
    assert body["hit_rate_pct"] == 100.0


def test_dashboard_includes_search_log_aggregates(tmp_path: Path) -> None:
    app = create_app()
    cache = TTLCache(tmp_path / "db")
    cache.log_search("python asyncio", "hash1", "cache", SearchLogMeta(result_count=2))
    app.state.cache = cache
    client = TestClient(app)

    response = client.get("/api/stats")

    assert response.status_code == 200
    body = response.json()
    assert body["log"]["hit_rate"]["total"] == 1
    assert body["log"]["hit_rate"]["cache_hits"] == 1
