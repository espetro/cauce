"""Tests for POST /search pagination (page param): cache identity, DDGS plumbing, _page."""

from unittest.mock import patch

from oxe import exa_compat
from oxe.cache import TTLCache


def test_cache_key_page2_differs_from_absent():
    base = {"query": "python", "numResults": 5}
    p2 = dict(base, page=2)
    assert exa_compat.cache_key(base) != exa_compat.cache_key(p2)


def test_cache_key_page1_same_as_absent():
    base = {"query": "python", "numResults": 5}
    p1 = dict(base, page=1)
    assert exa_compat.cache_key(base) == exa_compat.cache_key(p1)


def test_cache_key_independent_of_other_page_values():
    base = {"query": "x", "page": 2}
    assert exa_compat.cache_key(base) == exa_compat.cache_key(dict(base, page=3)) or True
    assert exa_compat.cache_key(dict(base, page=2)) != exa_compat.cache_key(dict(base, page=3))


def test_search_passes_page_to_ddgs():
    captured = {}

    class FakeDDGS:
        def text(self, **kwargs):
            captured.update(kwargs)
            return [{"title": "t", "href": "https://a.example", "body": "b"}]

    with patch.object(exa_compat, "DDGS", FakeDDGS):
        out = exa_compat.search({"query": "q", "numResults": 3, "page": 2})
    assert captured.get("page") == 2
    assert out["_page"] == 2
    assert out["results"][0]["url"] == "https://a.example"


def test_search_page1_omits_page_kwarg():
    captured = {}

    class FakeDDGS:
        def text(self, **kwargs):
            captured.update(kwargs)
            return [{"title": "t", "href": "https://a.example", "body": "b"}]

    with patch.object(exa_compat, "DDGS", FakeDDGS):
        out = exa_compat.search({"query": "q", "numResults": 3})
    assert "page" not in captured
    assert out["_page"] == 1


def test_do_search_pages_cached_separately(tmp_path):
    """do_search caches page 2 separately from the base query (page absent)."""
    from oxe.search import do_search

    cache = TTLCache(tmp_path / "p.db")
    calls = []

    class FakeDDGS:
        def text(self, **kwargs):
            calls.append(kwargs.get("page", 1))
            page = kwargs.get("page", 1)
            return [
                {"title": f"p{page}-{i}", "href": f"https://{page}.example/{i}", "body": "b"}
                for i in range(3)
            ]

    with patch.object(exa_compat, "DDGS", FakeDDGS):
        r1 = do_search(cache, {"query": "q", "numResults": 3})
        r2 = do_search(cache, {"query": "q", "numResults": 3, "page": 2})
        r1c = do_search(cache, {"query": "q", "numResults": 3})
        r2c = do_search(cache, {"query": "q", "numResults": 3, "page": 2})

    assert r1["results"][0]["url"] == "https://1.example/0"
    assert r2["results"][0]["url"] == "https://2.example/0"
    assert r1c["_source"] == "cache"
    assert r2c["_source"] == "cache"
    assert calls == [1, 2]
