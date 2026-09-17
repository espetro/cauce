"""Tests for oxe.search.engines.ddgs.DdgsEngine.

DDGS().text() is monkeypatched at the module level (oxe.search.engines.ddgs.DDGS)
so no network call ever happens.
"""

import asyncio

import pytest

from oxe.search.engines import ddgs as ddgs_mod
from oxe.search.errors import BackendError
from oxe.search.model import SearchRequest


class _FakeDDGS:
    """Stands in for ddgs.DDGS; .text() call count/behavior is set per test."""

    def __init__(self) -> None:
        self.calls: list[dict[str, object]] = []
        self.responses: list[list[dict[str, str]]] = []
        self.exceptions: list[Exception | None] = []

    def text(self, query: str, **kwargs: object) -> list[dict[str, str]]:
        self.calls.append({"query": query, **kwargs})
        idx = len(self.calls) - 1
        if idx < len(self.exceptions) and self.exceptions[idx] is not None:
            exc = self.exceptions[idx]
            assert exc is not None
            raise exc
        return self.responses[idx] if idx < len(self.responses) else []


@pytest.fixture
def fake_ddgs(monkeypatch: pytest.MonkeyPatch) -> _FakeDDGS:
    fake = _FakeDDGS()
    monkeypatch.setattr(ddgs_mod, "DDGS", lambda: fake)
    return fake


def test_search_builds_canonical_results(fake_ddgs: _FakeDDGS) -> None:
    fake_ddgs.responses = [
        [{"title": "T1", "href": "https://a.example/1", "body": "B1"}],
    ]
    engine = ddgs_mod.DdgsEngine("ddg")

    out = asyncio.run(engine.search(SearchRequest(q="python")))

    assert out.query == "python"
    assert out.number_of_results == 1
    r = out.results[0]
    assert r.url == "https://a.example/1"
    assert r.title == "T1"
    assert r.content == "B1"
    assert r.engine == "ddg"
    assert r.engines == ["ddg"]


def test_search_retries_once_then_succeeds(fake_ddgs: _FakeDDGS) -> None:
    fake_ddgs.exceptions = [ddgs_mod.DDGSException("rate limited"), None]
    fake_ddgs.responses = [[], [{"title": "T", "href": "u", "body": "b"}]]
    engine = ddgs_mod.DdgsEngine("ddg")

    out = asyncio.run(engine.search(SearchRequest(q="python")))

    assert len(fake_ddgs.calls) == 2
    assert out.results[0].url == "u"


def test_search_raises_backend_error_after_retry_fails(fake_ddgs: _FakeDDGS) -> None:
    err = ddgs_mod.DDGSException("down")
    fake_ddgs.exceptions = [err, err]
    engine = ddgs_mod.DdgsEngine("ddg")

    with pytest.raises(BackendError):
        asyncio.run(engine.search(SearchRequest(q="python")))
    assert len(fake_ddgs.calls) == 2


def test_unknown_engine_name_raises() -> None:
    with pytest.raises(BackendError):
        ddgs_mod.DdgsEngine("not-a-real-engine")


def test_time_range_maps_to_timelimit(fake_ddgs: _FakeDDGS) -> None:
    fake_ddgs.responses = [[]]
    engine = ddgs_mod.DdgsEngine("ddg")

    asyncio.run(engine.search(SearchRequest(q="python", time_range="week")))

    assert fake_ddgs.calls[0]["timelimit"] == "w"
