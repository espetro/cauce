"""Tests for oxe.search.engines.compose (FallbackEngine, FanoutEngine, run_one).

Adapted from legacy tests/test_backends.py's coverage of FallbackBackend /
FanoutBackend, retyped against the canonical async SearchEngine protocol.
"""

import asyncio

import pytest

from oxe.search.engines.compose import FallbackEngine, FanoutEngine, run_one
from oxe.search.errors import BackendError
from oxe.search.model import SearchRequest, SearchResult, SearxResponse


class Fake:
    def __init__(
        self, name: str, results: list[str] | None = None, error: str | None = None
    ) -> None:
        self.name = name
        self.timeout = 10.0
        self._results = results or []
        self._error = error

    async def search(self, req: SearchRequest) -> SearxResponse:
        if self._error:
            raise BackendError(self._error)
        return SearxResponse(
            query=req.q,
            number_of_results=len(self._results),
            results=[
                SearchResult(url=u, title=u, engine=self.name, engines=[self.name])
                for u in self._results
            ],
        )


class Slow:
    name = "slow"
    timeout = 0.1

    async def search(self, req: SearchRequest) -> SearxResponse:
        await asyncio.sleep(5)
        return SearxResponse(query=req.q)


REQ = SearchRequest(q="test")


def test_fallback_first_non_empty() -> None:
    fb = FallbackEngine([Fake("a"), Fake("b", results=["u1"])])
    out = asyncio.run(fb.search(REQ))
    assert out.results[0].url == "u1"
    assert out.results[0].engine == "b"


def test_fallback_empty_fallthrough() -> None:
    fb = FallbackEngine([Fake("a"), Fake("c", results=["u2"])])
    out = asyncio.run(fb.search(REQ))
    assert out.results[0].url == "u2"


def test_fallback_exception_fallthrough() -> None:
    fb = FallbackEngine([Fake("a", error="boom"), Fake("b", results=["u1"])])
    out = asyncio.run(fb.search(REQ))
    assert out.results[0].url == "u1"
    assert out.unresponsive_engines[0].engine == "a"


def test_fallback_all_fail_raises() -> None:
    fb = FallbackEngine([Fake("a", error="x"), Fake("b", error="y")])
    with pytest.raises(BackendError):
        asyncio.run(fb.search(REQ))


def test_fallback_all_empty_returns_empty() -> None:
    fb = FallbackEngine([Fake("a"), Fake("b")])
    out = asyncio.run(fb.search(REQ))
    assert out.results == []
    assert out.unresponsive_engines == []


def test_fallback_no_engines_raises() -> None:
    fb = FallbackEngine([])
    with pytest.raises(BackendError):
        asyncio.run(fb.search(REQ))


def test_fanout_dedup_and_order() -> None:
    a = Fake("a", results=["u1", "u2"])
    b = Fake("b", results=["u2", "u3"])
    fo = FanoutEngine([a, b])
    out = asyncio.run(fo.search(REQ))
    urls = [r.url for r in out.results]
    assert urls == ["u1", "u2", "u3"], urls
    engines = [r.engine for r in out.results]
    assert engines == ["a", "a", "b"]


def test_fanout_failed_provider_skipped() -> None:
    a = Fake("a", results=["u1"])
    b = Fake("b", error="boom")
    fo = FanoutEngine([a, b])
    out = asyncio.run(fo.search(REQ))
    assert [r.url for r in out.results] == ["u1"]
    assert out.unresponsive_engines[0].engine == "b"


def test_fanout_no_engines_returns_empty() -> None:
    fo = FanoutEngine([])
    out = asyncio.run(fo.search(REQ))
    assert out.results == []


def test_run_one_timeout() -> None:
    with pytest.raises(BackendError, match="timed out"):
        asyncio.run(run_one(Slow(), REQ, timeout=0.1))
