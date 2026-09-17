"""Tests for ``GET /api/history`` (``oxe/api/history.py``)."""

from pathlib import Path

from fastapi.testclient import TestClient

from oxe.app import create_app
from oxe.cache import TTLCache


def _seed(cache: TTLCache) -> None:
    cache.set("hash1", {"_q": "python asyncio"}, ttl=3600)
    cache.set("hash2", {"_q": "rust tokio"}, ttl=3600)
    cache.record_click(
        "hash1", "r1", "https://realpython.com/asyncio", "Understand asyncio", "web-ui"
    )
    cache.record_click("hash2", "r1", "https://tokio.rs/tutorial", "Tokio tutorial", "mcp")


def test_history_empty_returns_empty_rows_and_zeroed_stats(tmp_path: Path) -> None:
    app = create_app()
    app.state.cache = TTLCache(tmp_path / "db")
    client = TestClient(app)

    response = client.get("/api/history")

    assert response.status_code == 200
    body = response.json()
    assert body["rows"] == []
    assert body["stats"] == {"total": 0, "last_24h": 0, "oldest": None}


def test_history_unfiltered_returns_newest_first(tmp_path: Path) -> None:
    app = create_app()
    cache = TTLCache(tmp_path / "db")
    _seed(cache)
    app.state.cache = cache
    client = TestClient(app)

    response = client.get("/api/history")

    assert response.status_code == 200
    body = response.json()
    assert len(body["rows"]) == 2
    assert body["stats"]["total"] == 2
    urls = [row["url"] for row in body["rows"]]
    assert "https://tokio.rs/tutorial" in urls
    assert "https://realpython.com/asyncio" in urls


def test_history_query_text_filter_narrows_rows(tmp_path: Path) -> None:
    app = create_app()
    cache = TTLCache(tmp_path / "db")
    _seed(cache)
    app.state.cache = cache
    client = TestClient(app)

    response = client.get("/api/history", params={"q": "asyncio"})

    assert response.status_code == 200
    body = response.json()
    assert len(body["rows"]) == 1
    assert body["rows"][0]["url"] == "https://realpython.com/asyncio"


def test_history_since_filter_excludes_old_clicks(tmp_path: Path) -> None:
    app = create_app()
    cache = TTLCache(tmp_path / "db")
    _seed(cache)
    app.state.cache = cache
    client = TestClient(app)

    response = client.get("/api/history", params={"since": 24})

    assert response.status_code == 200
    body = response.json()
    # Both clicks were just recorded, so both fall within the last 24h.
    assert len(body["rows"]) == 2


def test_history_since_rejects_non_positive_value(tmp_path: Path) -> None:
    app = create_app()
    app.state.cache = TTLCache(tmp_path / "db")
    client = TestClient(app)

    response = client.get("/api/history", params={"since": 0})

    assert response.status_code == 422
