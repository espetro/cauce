"""Empty results vs backend failure at the search-pipeline level.

- all-backends-fail payloads carry `_error` + `_error_kind` and are NOT cached
- empty-but-successful responses have no `_error` keys and follow negative TTL
- HTTP /search sets X-Cache: MISS then HIT for identical queries
"""

from unittest.mock import patch

from fastapi.testclient import TestClient

from oxe import exa_compat
from oxe.cache import TTLCache
from oxe.search import do_search
from oxe.server import make_app


def _req(query="q"):
    return {"query": query, "numResults": 3, "contents": {"text": True}}


def test_all_backends_fail_annotates_and_skips_cache():
    class BoomDDGS:
        def text(self, **kwargs):
            raise RuntimeError("boom")

    with patch.object(exa_compat, "DDGS", BoomDDGS):
        payload = exa_compat.search(_req())
        assert payload["_error"] == "boom"
        assert payload["_error_kind"] == "backend_error"
        assert payload["results"] == []

        cache = TTLCache(":memory:")
        out = do_search(cache, _req())
        assert out["_error"] == "boom"
        assert out["_error_kind"] == "backend_error"
        assert out["_source"] == "network"

    hit = cache.get_with_meta(exa_compat.cache_key(_req() | {"_backend": "ddg"}))
    assert hit is None  # failure must not be cached


def test_rate_limit_maps_to_rate_limited():
    from ddgs.exceptions import RatelimitException

    class RLDDGS:
        def text(self, **kwargs):
            raise RatelimitException("429")

    with patch.object(exa_compat, "DDGS", RLDDGS):
        payload = exa_compat.search(_req())
    assert payload["_error_kind"] == "rate_limited"
    assert payload["results"] == []


def test_timeout_maps_to_timeout():
    from ddgs.exceptions import TimeoutException

    class TDDGS:
        def text(self, **kwargs):
            raise TimeoutException("t/o")

    with patch.object(exa_compat, "DDGS", TDDGS):
        payload = exa_compat.search(_req())
    assert payload["_error_kind"] == "timeout"


def test_empty_success_has_no_error_keys_and_negative_ttl():
    class EmptyDDGS:
        def text(self, **kwargs):
            return []

    cache = TTLCache(":memory:")
    with patch.object(exa_compat, "DDGS", EmptyDDGS):
        payload = exa_compat.search(_req())
        assert "_error" not in payload
        assert "_error_kind" not in payload
        out = do_search(cache, _req())
    assert "_error" not in out
    assert out["results"] == []
    key = exa_compat.cache_key(_req() | {"_backend": "ddg"})
    hit = cache.get_with_meta(key)
    assert hit is not None  # negative-TTL cached


def test_x_cache_header_miss_then_hit():
    class OneDDGS:
        def text(self, **kwargs):
            return [{"title": "t", "href": "https://a.example", "body": "b"}]

    cache = TTLCache(":memory:")
    app = make_app(cache=cache)
    client = TestClient(app)
    with patch.object(exa_compat, "DDGS", OneDDGS):
        r1 = client.post("/search", json=_req())
        assert r1.status_code == 200
        assert r1.headers["x-cache"] == "MISS"
        r2 = client.post("/search", json=_req())
        assert r2.headers["x-cache"] == "HIT"
        assert r2.json()["results"][0]["url"] == "https://a.example"
