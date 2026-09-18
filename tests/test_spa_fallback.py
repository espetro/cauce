"""SPA deep-link fallback: unmatched GET paths serve the UI's index.html
(hard refresh on /history, /dashboard, ...) while API routes keep priority
and non-navigation requests still 404 (the content-negotiated JSON contract
must not start returning HTML).
"""

import os
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from oxe.app import create_app


@pytest.fixture
def dist(tmp_path: Path) -> Path:
    d = tmp_path / "dist"
    d.mkdir()
    (d / "index.html").write_text("<html>spa</html>", encoding="utf-8")
    return d


def _client(dist: Path) -> TestClient:
    os.environ["OXE_DIST_DIR"] = str(dist)
    try:
        return TestClient(create_app())
    finally:
        del os.environ["OXE_DIST_DIR"]


@pytest.mark.parametrize("path", ["/history", "/dashboard", "/settings/ui"])
def test_deep_link_serves_index_html(dist: Path, path: str) -> None:
    response = _client(dist).get(path, headers={"accept": "text/html"})
    assert response.status_code == 200
    assert "text/html" in response.headers["content-type"]
    assert "spa" in response.text


def test_api_route_priority_over_fallback(dist: Path) -> None:
    client = _client(dist)
    health = client.get("/health")
    assert health.status_code == 200
    assert health.json() == {"status": "ok"}
    root = client.get("/", headers={"accept": "text/html"})
    assert root.status_code == 200
    assert "spa" in root.text


def test_non_navigation_request_still_404s(dist: Path) -> None:
    response = _client(dist).get("/history")
    assert response.status_code == 404
