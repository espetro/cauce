"""HTTP-level tests for ``GET /api/stats`` and ``GET /api/history``.

Mirrors ``tests/test_stats.py`` seeding style (``TTLCache`` + ``log_search`` /
``record_click``) driven through the FastAPI app. Each test gets a fresh cache
db by pointing ``OXE_CACHE_DIR`` at ``tmp_path`` before ``create_app()`` builds
the app against it.
"""

from collections.abc import Iterator
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from oxe.app import create_app
from oxe.cache import SearchLogMeta, TTLCache


@pytest.fixture
def cache(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Iterator[TTLCache]:
    """A fresh per-test cache db; the app under test is built against it."""
    monkeypatch.setenv("OXE_CACHE_DIR", str(tmp_path / "cache"))
    app = create_app()
    yielded = app.state.search_service.cache
    yield yielded
    yielded.close()


@pytest.fixture
def client(cache: TTLCache) -> TestClient:
    """TestClient over the app whose cache is the ``cache`` fixture."""
    del cache
    return TestClient(create_app())


def _seed(cache: TTLCache) -> None:
    cache.set("hash1", {"_q": "python asyncio", "results": [1, 2]}, ttl=3600)
    cache.set("hash2", {"_q": "rust tokio", "results": []}, ttl=3600)
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
    cache.record_click("hash1", "r1", "https://example.com/a", "Example A", source="web-ui")
    cache.record_click("hash2", "r2", "https://example.com/b", "Example B", source="mcp")


def test_api_stats_returns_summary(cache: TTLCache, client: TestClient) -> None:
    _seed(cache)
    r = client.get("/api/stats")
    assert r.status_code == 200
    body = r.json()
    assert body["days"] == 14
    assert body["hit_rate"]["cache_hits"] == 1
    assert body["top_queries"][0]["query"] == "python asyncio"


def test_api_stats_days_param(cache: TTLCache, client: TestClient) -> None:
    _seed(cache)
    assert client.get("/api/stats", params={"days": 7}).json()["days"] == 7
    assert client.get("/api/stats", params={"days": 0}).status_code == 422


def test_api_history_shape(cache: TTLCache, client: TestClient) -> None:
    _seed(cache)
    r = client.get("/api/history")
    assert r.status_code == 200
    body = r.json()
    assert body["stats"]["total"] == 2
    assert body["stats"]["last_24h"] == 2
    assert body["limit"] == 50
    assert body["since"] is None
    assert len(body["items"]) == 2
    assert {i["source"] for i in body["items"]} == {"web-ui", "mcp"}
    assert body["items"][0]["clicked_at"] >= body["items"][1]["clicked_at"]


def test_api_history_filters(cache: TTLCache, client: TestClient) -> None:
    _seed(cache)
    body = client.get("/api/history", params={"q": "rust"}).json()
    assert [i["query"] for i in body["items"]] == ["rust tokio"]
    body = client.get("/api/history", params={"since": "24"}).json()
    assert body["since"] == 24
    assert len(body["items"]) == 2
    # TestClient with raise_server_exceptions=True (default) re-raises the
    # ValueError the handler raises for gap values like 30 -- which is the
    # assertion itself.
    assert client.get("/api/history", params={"since": "30"}).status_code == 422
    assert client.get("/api/history", params={"since": "719"}).status_code == 422
    assert client.get("/api/history", params={"limit": 201}).status_code == 422


def test_api_history_empty(client: TestClient) -> None:
    body = client.get("/api/history").json()
    assert body["items"] == []
    assert body["stats"]["total"] == 0
    assert body["stats"]["oldest"] is None
