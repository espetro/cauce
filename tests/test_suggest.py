"""Tests for /suggest (OpenSearch Suggestions JSON) over search_log."""

from fastapi.testclient import TestClient

from oxe.cache import TTLCache
from oxe.server import make_app


def _app(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    app = make_app(cache=c, backend=object())
    return c, TestClient(app)


def test_suggest_empty_log(tmp_path):
    _, client = _app(tmp_path)
    r = client.get("/suggest", params={"q": "py"})
    assert r.status_code == 200
    assert r.json() == ["py", [], [], []]


def test_suggest_prefix_match_recency_rank(tmp_path):
    c, client = _app(tmp_path)
    c.log_search("python tips", "h1", source="http")
    c.log_search("golang", "h2", source="http")
    c.log_search("python asyncio", "h3", source="http")
    r = client.get("/suggest", params={"q": "py"})
    assert r.json() == ["py", ["python asyncio", "python tips"], [], []]


def test_suggest_case_insensitive_and_dedup(tmp_path):
    c, client = _app(tmp_path)
    c.log_search("Python tips", "h1", source="http")
    c.log_search("python tips", "h1", source="http")  # same hash, dup text
    r = client.get("/suggest", params={"q": "PY"})
    assert r.json() == ["PY", ["Python tips"], [], []]
    c.close()


def test_suggest_frequency_breaks_ties(tmp_path):
    c, client = _app(tmp_path)
    c.log_search("python old", "h1", source="http")
    c.log_search("python frequent", "h2", source="http")
    c.log_search("python frequent", "h2", source="http")
    r = client.get("/suggest", params={"q": "pyth"})
    assert r.json()[1] == ["python frequent", "python old"]
    c.close()


def test_suggest_caps_at_three(tmp_path):
    c, client = _app(tmp_path)
    for i in range(5):
        c.log_search(f"query {i}", f"h{i}", source="http")
    r = client.get("/suggest", params={"q": "query"})
    assert len(r.json()[1]) == 3
    c.close()


def test_suggest_special_chars_no_crash(tmp_path):
    _, client = _app(tmp_path)
    r = client.get("/suggest", params={"q": "py%th_on\\"})
    assert r.status_code == 200
    assert r.json()[1] == []


def test_suggest_empty_q(tmp_path):
    _, client = _app(tmp_path)
    r = client.get("/suggest", params={"q": "  "})
    assert r.status_code == 200
    assert r.json() == ["", [], [], []]
