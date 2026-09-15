"""Tests for optional built-UI (SPA) serving via OXE_UI_DIST / ui/dist."""

from fastapi.testclient import TestClient

from oxe.cache import TTLCache
from oxe.server import _ui_dist_dir, make_app


def _client(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    app = make_app(cache=c, backend=object())
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


def test_root_falls_back_to_legacy_template(tmp_path, monkeypatch):
    monkeypatch.delenv("OXE_UI_DIST", raising=False)
    monkeypatch.chdir(tmp_path)  # no dist anywhere
    _, client = _client(tmp_path)
    r = client.get("/")
    assert r.status_code == 200
    assert "<html" in r.text.lower()
