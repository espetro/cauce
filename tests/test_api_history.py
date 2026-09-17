"""Tests for the GET /api/history endpoint and dual-format /history/delete."""

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
    cache.set("hash1", {"_q": "python asyncio", "results": []}, ttl=3600)
    cache.set("hash2", {"_q": "rust tokio", "results": []}, ttl=3600)
    cache.record_click("hash1", "r1", "https://a.example", "Python A", source="web-ui")
    cache.record_click("hash2", "r2", "https://b.example", "Rust B", source="mcp")


def test_history_merges_clicks_and_cache(client, cache):
    _seed(cache)
    res = client.get("/api/history")
    assert res.status_code == 200
    body = res.json()
    kinds = {i["kind"] for i in body["items"]}
    assert kinds == {"click", "cache"}
    assert body["clicks"] == 2
    assert body["cache_rows"] == 2
    assert body["since"] == "all"
    # sorted newest first
    ts = [
        i.get("clicked_at") or i.get("created_at") for i in body["items"]
    ]
    assert ts == sorted(ts, reverse=True)


def test_history_item_shapes(client, cache):
    _seed(cache)
    items = client.get("/api/history").json()["items"]
    click = next(i for i in items if i.get("result_id") == "r1")
    assert click["query_hash"] == "hash1"
    assert click["query"] == "python asyncio"
    assert click["result_id"] == "r1"
    assert click["url"] == "https://a.example"
    assert click["title"] == "Python A"
    assert click["source"] == "web-ui"
    assert "clicked_at" in click
    crow = next(i for i in items if i["kind"] == "cache")
    assert crow["query"] in ("python asyncio", "rust tokio")
    assert crow["hits"] == 0
    assert "expires_at" in crow and "created_at" in crow


def test_history_kind_filter(client, cache):
    _seed(cache)
    body = client.get("/api/history", params={"kind": "clicks"}).json()
    assert all(i["kind"] == "click" for i in body["items"])
    assert body["cache_rows"] == 0
    body = client.get("/api/history", params={"kind": "cache"}).json()
    assert all(i["kind"] == "cache" for i in body["items"])
    assert body["clicks"] == 0


def test_history_q_substring(client, cache):
    _seed(cache)
    body = client.get("/api/history", params={"q": "python"}).json()
    assert body["items"]
    assert all("python" in i["query"] for i in body["items"])


def test_history_limit(client, cache):
    _seed(cache)
    body = client.get("/api/history", params={"limit": 3}).json()
    assert len(body["items"]) == 3
    assert body["limit"] == 3


def test_history_limit_validation(client):
    assert client.get("/api/history", params={"limit": 201}).status_code == 422


def test_history_since_24h_excludes_old_click(client, cache):
    _seed(cache)
    # backdate one click beyond 24h
    with cache._lock:
        cache._conn.execute(
            "UPDATE clicks SET clicked_at = clicked_at - 90000 WHERE result_id = 'r2'"
        )
        cache._conn.commit()
    body = client.get("/api/history", params={"since": "24h"}).json()
    assert body["clicks"] == 1
    assert all(i["result_id"] != "r2" for i in body["items"] if i["kind"] == "click")


def test_history_bad_params(client):
    assert client.get("/api/history", params={"since": "1y"}).status_code == 422
    assert client.get("/api/history", params={"kind": "bogus"}).status_code == 422


def test_history_delete_json(client, cache):
    _seed(cache)
    res = client.post("/history/delete", json={"scope": "all"})
    assert res.status_code == 200
    assert res.json() == {"ok": True, "deleted": 2}
    assert client.get("/api/history", params={"kind": "clicks"}).json()["clicks"] == 0


def test_history_delete_form_still_redirects(client, cache):
    _seed(cache)
    res = client.post(
        "/history/delete",
        data={"scope": "24h"},
        follow_redirects=False,
    )
    assert res.status_code == 303
    assert res.headers["location"] == "/history"
    assert client.get("/api/history", params={"kind": "clicks"}).json()["clicks"] == 0


def test_history_delete_bad_scope(client):
    assert (
        client.post("/history/delete", json={"scope": "nope"}).status_code == 422
    )
    assert (
        client.post(
            "/history/delete", data={"scope": "nope"}, follow_redirects=False
        ).status_code
        == 422
    )


def test_history_and_dashboard_serve_shell(client):
    for path in ("/history", "/dashboard"):
        res = client.get(path)
        assert res.status_code == 200
        assert "text/html" in res.headers["content-type"]
