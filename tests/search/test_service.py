"""Tests for oxe.search.service.SearchService.

No network: engines are faked with a small class implementing the
SearchEngine protocol (search() is async but does no I/O), matching the
style of legacy tests/test_backends.py's ``Fake`` backend.

Async methods are driven with ``asyncio.run()`` directly rather than adding
an asyncio pytest plugin dependency for three tests.
"""

import asyncio
from pathlib import Path

import pytest

from oxe.cache import TTLCache
from oxe.search.errors import BackendError
from oxe.search.model import SearchRequest, SearchResult, SearxResponse, UnresponsiveEngine
from oxe.search.service import SearchService, cache_key


class FakeEngine:
    """A SearchEngine that returns a canned response and counts calls."""

    def __init__(self, response: SearxResponse, *, name: str = "fake") -> None:
        self.name = name
        self.timeout = 10.0
        self._response = response
        self.calls = 0

    async def search(self, req: SearchRequest) -> SearxResponse:
        del req  # canned response, doesn't depend on the request
        self.calls += 1
        return self._response


def _result(url: str) -> SearchResult:
    return SearchResult(url=url, title=url, engine="fake")


def test_successful_search_returns_well_typed_response(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    response = SearxResponse(query="python", number_of_results=1, results=[_result("u1")])
    engine = FakeEngine(response)
    service = SearchService(engine, cache)

    out = asyncio.run(service.search(SearchRequest(q="python")))

    assert isinstance(out, SearxResponse)
    assert out.query == "python"
    assert out.results == [_result("u1")]
    assert engine.calls == 1


def test_cache_is_hit_on_second_identical_query(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    response = SearxResponse(query="rust", number_of_results=1, results=[_result("u1")])
    engine = FakeEngine(response)
    service = SearchService(engine, cache)
    req = SearchRequest(q="rust")

    first = asyncio.run(service.search(req))
    second = asyncio.run(service.search(req))

    assert engine.calls == 1
    assert first == second


def test_cache_miss_on_different_query(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    response = SearxResponse(query="q", number_of_results=1, results=[_result("u1")])
    engine = FakeEngine(response)
    service = SearchService(engine, cache)

    asyncio.run(service.search(SearchRequest(q="rust")))
    asyncio.run(service.search(SearchRequest(q="python")))

    assert engine.calls == 2


def test_unresponsive_engines_with_empty_results_raises_backend_error(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    response = SearxResponse(
        query="python",
        results=[],
        unresponsive_engines=[UnresponsiveEngine(engine="ddg", error="timed out")],
    )
    engine = FakeEngine(response)
    service = SearchService(engine, cache)

    with pytest.raises(BackendError):
        asyncio.run(service.search(SearchRequest(q="python")))


def test_unresponsive_engines_with_nonempty_results_does_not_raise(tmp_path: Path) -> None:
    cache = TTLCache(tmp_path / "db")
    response = SearxResponse(
        query="python",
        results=[_result("u1")],
        unresponsive_engines=[UnresponsiveEngine(engine="ddg", error="timed out")],
    )
    engine = FakeEngine(response)
    service = SearchService(engine, cache)

    out = asyncio.run(service.search(SearchRequest(q="python")))

    assert out.results == [_result("u1")]
    assert out.unresponsive_engines[0].engine == "ddg"


def test_cache_key_depends_on_query_shaping_fields() -> None:
    base = SearchRequest(q="python")
    same_but_case = SearchRequest(q="  Python  ")
    different_page = SearchRequest(q="python", pageno=2)

    assert cache_key(base) == cache_key(same_but_case)
    assert cache_key(base) != cache_key(different_page)


def test_cache_key_ignores_category_order() -> None:
    a = SearchRequest(q="python", categories=["general", "news"])
    b = SearchRequest(q="python", categories=["news", "general"])

    assert cache_key(a) == cache_key(b)
