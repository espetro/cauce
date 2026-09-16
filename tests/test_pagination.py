"""Tests for POST /search pagination (page param).

Strategy: pass `page` through to ddgs (it supports the kwarg) with a single
in-backends retry, because DDG html paging is aggressively rate limited and
frequently raises a transient "No results found." Deep-page results are
cached with a short TTL so a flaky fetch doesn't stick (oxe/search.py).
"""

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


def test_cache_key_page2_differs_from_page3():
    base = {"query": "x", "page": 2}
    assert exa_compat.cache_key(dict(base, page=2)) != exa_compat.cache_key(
        dict(base, page=3)
    )


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


def test_search_page2_retries_once_on_transient_failure():
    """First DDGS call fails, immediate retry succeeds (rate-limit recovery)."""
    calls = {"n": 0}

    class FakeDDGS:
        def text(self, **kwargs):
            calls["n"] += 1
            if calls["n"] == 1:
                raise RuntimeError("No results found.")
            return [{"title": "t", "href": "https://a.example", "body": "b"}]

    with patch.object(exa_compat, "DDGS", FakeDDGS), patch.object(
        exa_compat.time, "sleep"
    ):
        out = exa_compat.search({"query": "q", "numResults": 3, "page": 2})
    assert calls["n"] == 2
    assert len(out["results"]) == 1


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


def test_do_search_page2_empty_cached_briefly(tmp_path):
    """A page-2 fetch that legitimately returns nothing is still cached
    (dedupe), but with the short _PAGE_TTL so it self-heals fast."""
    from oxe import search as search_mod
    from oxe.search import do_search

    cache = TTLCache(tmp_path / "p.db")

    class FakeDDGS:
        def text(self, **kwargs):
            return []

    with patch.object(exa_compat, "DDGS", FakeDDGS):
        out = do_search(cache, {"query": "q", "numResults": 3, "page": 2})
    assert out["results"] == []
    # cached empty page-2 expires within the short page TTL window
    expires = cache._conn.execute(
        "SELECT expires_at FROM cache WHERE query_hash = ?", (out["_q_hash"],)
    ).fetchone()
    assert expires is not None
    import time as _t

    assert expires[0] <= _t.time() + search_mod._PAGE_TTL + 5
