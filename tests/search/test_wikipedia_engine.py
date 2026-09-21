"""Tests for oxe.search.engines.wikipedia.WikipediaEngine.

The blocking ``_fetch`` helper (the only thing that touches ``urlopen``) is
monkeypatched at the module level, so no network call ever happens.
"""

import asyncio
import io
import json
import urllib.error
import urllib.parse
import urllib.request
from email.message import Message

import pytest

from oxe.search.engines import wikipedia as wiki_mod
from oxe.search.engines.protocol import SearchEngine
from oxe.search.errors import BackendError
from oxe.search.model import SearchRequest


class _FakeFetch:
    """Stands in for wikipedia._fetch; records URLs, returns or raises per test."""

    def __init__(self) -> None:
        self.urls: list[str] = []
        self.body: object = ["q", [], [], []]
        self.exc: Exception | None = None

    def __call__(self, url: str, timeout: float) -> object:  # noqa: ARG002
        self.urls.append(url)
        if self.exc is not None:
            raise self.exc
        return self.body


@pytest.fixture
def fake_fetch(monkeypatch: pytest.MonkeyPatch) -> _FakeFetch:
    fake = _FakeFetch()
    monkeypatch.setattr(wiki_mod, "_fetch", fake)
    return fake


def test_search_maps_opensearch_payload(fake_fetch: _FakeFetch) -> None:
    fake_fetch.body = [
        "python",
        ["Python (programming language)", "Pythonidae"],
        ["A language.", "A snake family."],
        ["https://en.wikipedia.org/wiki/Python", "https://en.wikipedia.org/wiki/Pythonidae"],
    ]

    out = asyncio.run(wiki_mod.WikipediaEngine().search(SearchRequest(q="python")))

    assert out.query == "python"
    assert out.number_of_results == 2
    first = out.results[0]
    assert first.title == "Python (programming language)"
    assert first.content == "A language."
    assert first.url == "https://en.wikipedia.org/wiki/Python"
    assert first.engine == "wikipedia-opensearch"
    assert first.engines == ["wikipedia-opensearch"]
    assert out.results[1].title == "Pythonidae"


def test_every_result_engine_is_stamped(fake_fetch: _FakeFetch) -> None:
    fake_fetch.body = ["q", ["A", "B", "C"], ["a", "b", "c"], ["u1", "u2", "u3"]]

    out = asyncio.run(wiki_mod.WikipediaEngine().search(SearchRequest(q="q")))

    assert {r.engine for r in out.results} == {"wikipedia-opensearch"}


def test_empty_results_is_not_an_error(fake_fetch: _FakeFetch) -> None:
    fake_fetch.body = ["zzzz", [], [], []]

    out = asyncio.run(wiki_mod.WikipediaEngine().search(SearchRequest(q="zzzz")))

    assert out.results == []
    assert out.number_of_results == 0


def test_request_has_expected_query_params(fake_fetch: _FakeFetch) -> None:
    asyncio.run(wiki_mod.WikipediaEngine().search(SearchRequest(q="hello world")))

    parsed = urllib.parse.urlparse(fake_fetch.urls[0])
    assert parsed.netloc == "en.wikipedia.org"
    qs = urllib.parse.parse_qs(parsed.query)
    assert qs["action"] == ["opensearch"]
    assert qs["search"] == ["hello world"]
    assert qs["namespace"] == ["0"]
    assert qs["format"] == ["json"]
    assert qs["limit"] == ["10"]


@pytest.mark.parametrize(
    ("language", "host"),
    [
        ("all", "en.wikipedia.org"),
        ("en-US", "en.wikipedia.org"),
        ("fr", "fr.wikipedia.org"),
        ("de_DE", "de.wikipedia.org"),
        ("evil.example/x", "en.wikipedia.org"),
    ],
)
def test_language_selects_subdomain(fake_fetch: _FakeFetch, language: str, host: str) -> None:
    asyncio.run(wiki_mod.WikipediaEngine().search(SearchRequest(q="q", language=language)))

    assert urllib.parse.urlparse(fake_fetch.urls[0]).netloc == host


def test_unsupported_request_fields_are_ignored(fake_fetch: _FakeFetch) -> None:
    fake_fetch.body = ["q", ["A"], ["a"], ["u"]]

    out = asyncio.run(
        wiki_mod.WikipediaEngine().search(
            SearchRequest(q="q", pageno=3, categories=["news"], time_range="week", safesearch=2)
        )
    )

    assert out.number_of_results == 1


def test_http_error_raises_backend_error(fake_fetch: _FakeFetch) -> None:
    fake_fetch.exc = urllib.error.HTTPError(
        "https://en.wikipedia.org", 429, "Too Many Requests", Message(), io.BytesIO(b"")
    )

    with pytest.raises(BackendError, match="429"):
        asyncio.run(wiki_mod.WikipediaEngine().search(SearchRequest(q="q")))


@pytest.mark.parametrize(
    "exc",
    [
        urllib.error.URLError("dns failure"),
        TimeoutError("timed out"),
        ConnectionResetError("reset"),
        json.JSONDecodeError("bad", "doc", 0),
    ],
)
def test_transport_errors_raise_backend_error(fake_fetch: _FakeFetch, exc: Exception) -> None:
    fake_fetch.exc = exc

    with pytest.raises(BackendError):
        asyncio.run(wiki_mod.WikipediaEngine().search(SearchRequest(q="q")))


@pytest.mark.parametrize("body", [{"error": "nope"}, ["only", "two"], "string"])
def test_unexpected_payload_shape_raises_backend_error(
    fake_fetch: _FakeFetch, body: object
) -> None:
    fake_fetch.body = body

    with pytest.raises(BackendError):
        asyncio.run(wiki_mod.WikipediaEngine().search(SearchRequest(q="q")))


def test_engine_satisfies_protocol() -> None:
    engine = wiki_mod.WikipediaEngine()

    assert isinstance(engine, SearchEngine)
    assert engine.name == "wikipedia-opensearch"
    assert engine.timeout > 0


def test_fetch_sends_descriptive_user_agent(monkeypatch: pytest.MonkeyPatch) -> None:
    seen: dict[str, str] = {}

    class _Resp(io.BytesIO):
        def __enter__(self) -> "_Resp":
            return self

    def fake_urlopen(request: urllib.request.Request, timeout: float) -> _Resp:  # noqa: ARG001
        seen.update({k.lower(): v for k, v in request.header_items()})
        return _Resp(b'["q", [], [], []]')

    monkeypatch.setattr(wiki_mod.urllib.request, "urlopen", fake_urlopen)

    body = wiki_mod._fetch(  # noqa: SLF001  # pyright: ignore[reportPrivateUsage]
        "https://en.wikipedia.org/w/api.php", 1.0
    )

    assert body == ["q", [], [], []]
    assert seen["user-agent"].startswith("oxe/")
    assert "github.com/espetro/oxe" in seen["user-agent"]
