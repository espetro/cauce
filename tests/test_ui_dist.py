"""Tests for optional built-UI (SPA) serving via OXE_UI_DIST / ui/dist."""

import shutil
from pathlib import Path

from fastapi.testclient import TestClient

from oxe.cache import TTLCache
from oxe.server import _ui_dist_dir, make_app


class _StubBackend:
    name = "stub"
    timeout = 10.0

    def search(self, req):
        return {
            "requestId": "r-stub",
            "searchType": req.get("type") or "auto",
            "results": [],
            "costDollars": {"total": 0.0},
        }


def _client(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    app = make_app(cache=c, backend=_StubBackend())
    return c, TestClient(app)


def test_ui_dist_dir_none_by_default(tmp_path, monkeypatch):
    monkeypatch.delenv("OXE_UI_DIST", raising=False)
    monkeypatch.chdir(tmp_path)  # no ./ui/dist here
    assert _ui_dist_dir() is None


def test_ui_dist_env_override(tmp_path, monkeypatch):
    dist = tmp_path / "dist"
    (dist / "assets").mkdir(parents=True)
    (dist / "index.html").write_text("<html>spa</html>")
    monkeypatch.setenv("OXE_UI_DIST", str(dist))
    assert _ui_dist_dir() == dist


def test_ui_dist_env_missing_index_is_none(tmp_path, monkeypatch):
    empty = tmp_path / "notadist"
    empty.mkdir()
    monkeypatch.setenv("OXE_UI_DIST", str(empty))
    assert _ui_dist_dir() is None


def test_root_serves_spa_index(tmp_path, monkeypatch):
    dist = tmp_path / "dist"
    dist.mkdir()
    (dist / "index.html").write_text("<html>spa</html>")
    monkeypatch.setenv("OXE_UI_DIST", str(dist))
    _, client = _client(tmp_path)
    r = client.get("/")
    assert r.status_code == 200
    assert "spa" in r.text
    assert r.headers["content-type"].startswith("text/html")


def test_assets_served_from_dist(tmp_path, monkeypatch):
    dist = tmp_path / "dist"
    (dist / "assets").mkdir(parents=True)
    (dist / "index.html").write_text("<html></html>")
    (dist / "assets" / "index-abc.js").write_text("console.log(1)")
    monkeypatch.setenv("OXE_UI_DIST", str(dist))
    _, client = _client(tmp_path)
    r = client.get("/assets/index-abc.js")
    assert r.status_code == 200
    assert "console.log" in r.text
    # traversal rejected
    assert client.get("/assets/..%2Findex.html").status_code in (400, 404)
    assert client.get("/assets/nope.js").status_code == 404


def test_root_minimal_page_without_dist(tmp_path, monkeypatch):
    monkeypatch.delenv("OXE_UI_DIST", raising=False)
    monkeypatch.chdir(tmp_path)  # no dist anywhere
    _, client = _client(tmp_path)
    r = client.get("/")
    assert r.status_code == 200
    assert "<html" in r.text.lower()
    assert "OXE_UI_DIST" in r.text


def test_dashboard_per_route_shell(tmp_path, monkeypatch):
    dist = tmp_path / "dist"
    dist.mkdir()
    (dist / "index.html").write_text("<html>root</html>")
    (dist / "dashboard").mkdir()
    (dist / "dashboard" / "index.html").write_text("<html>dashboard shell</html>")
    monkeypatch.setenv("OXE_UI_DIST", str(dist))
    _, client = _client(tmp_path)
    r = client.get("/dashboard")
    assert r.status_code == 200
    assert "dashboard shell" in r.text
    assert "root" not in r.text


def test_route_fallback_to_root_shell(tmp_path, monkeypatch):
    dist = tmp_path / "dist"
    dist.mkdir()
    (dist / "index.html").write_text("<html>root</html>")
    monkeypatch.setenv("OXE_UI_DIST", str(dist))
    _, client = _client(tmp_path)
    for route in ("/dashboard", "/history"):
        r = client.get(route)
        assert r.status_code == 200
        assert "root" in r.text
    r = client.get("/search?q=python", headers={"accept": "text/html"})
    assert r.status_code == 200
    assert "root" in r.text


def test_search_html_branch_xcache_with_per_route_shell(tmp_path, monkeypatch):
    dist = tmp_path / "dist"
    (dist / "search").mkdir(parents=True)
    (dist / "index.html").write_text("<html>root</html>")
    (dist / "search" / "index.html").write_text("<html>search shell</html>")
    monkeypatch.setenv("OXE_UI_DIST", str(dist))
    _, client = _client(tmp_path)
    r = client.get("/search?q=python", headers={"accept": "text/html"})
    assert r.status_code == 200
    assert "search shell" in r.text
    assert "x-cache" in {k.lower() for k in r.headers}


def test_search_json_branch_unchanged(tmp_path, monkeypatch):
    dist = tmp_path / "dist"
    (dist / "search").mkdir(parents=True)
    (dist / "index.html").write_text("<html>root</html>")
    (dist / "search" / "index.html").write_text("<html>search shell</html>")
    monkeypatch.setenv("OXE_UI_DIST", str(dist))
    _, client = _client(tmp_path)
    r = client.get("/search?q=python", headers={"accept": "application/json"})
    assert r.status_code == 200
    data = r.json()
    assert "results" in data
    assert "<html>" not in r.text


def test_real_dist_routes_on_tmp_copy(tmp_path, monkeypatch):
    """Per-route shells work against a copy of the real built bundle."""
    repo_dist = Path(__file__).resolve().parent.parent / "ui" / "dist"
    if not (repo_dist / "index.html").is_file():
        import pytest

        pytest.skip("ui/dist not built")
    dist = tmp_path / "dist"
    shutil.copytree(repo_dist, dist)
    monkeypatch.setenv("OXE_UI_DIST", str(dist))
    for route in ("/dashboard", "/history", "/search"):
        shell = dist / route.strip("/") / "index.html"
        _, client = _client(tmp_path)
        r = client.get(f"{route}?q=x" if route == "/search" else route)
        assert r.status_code == 200, route
        if shell.is_file():
            assert r.text == shell.read_text()
        else:
            assert r.text == (dist / "index.html").read_text()
