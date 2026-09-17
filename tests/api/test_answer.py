"""Tests for the wave-4 AI answer backend: parse_final_answer, the tool
loop cap, the always-sent ``tools`` kwarg, force fixtures, and answer
caching. No live provider is ever contacted -- provider calls go through
``oxe.ai._chat_completion``, whose ``urlopen`` the tests stub at the
boundary (asserting the exact wire payload, which is how the ``tools``
kwarg presence is pinned).
"""

import asyncio
import json
import urllib.error
from collections.abc import AsyncGenerator, Coroutine
from pathlib import Path
from typing import Any, cast

import pytest
from fastapi.testclient import TestClient
from pydantic import TypeAdapter, ValidationError

from oxe.ai import (
    TOOLS_SPEC,
    _chat_completion,  # pyright: ignore[reportPrivateUsage]
    answer_cache_key,
    build_toolset,
    parse_final_answer,
    stream_answer,
)
from oxe.api.ai_frames import AnswerFrame, DoneFrame, ErrorFrame, SourcesFrame
from oxe.api.answer import (  # pyright: ignore[reportPrivateUsage]
    AI_OFF_MESSAGE,
    AnswerRequest,
    CachedAnswer,
    _answer_frames,  # pyright: ignore[reportPrivateUsage]
)
from oxe.app import create_app
from oxe.cache import TTLCache
from oxe.config import CACHE_MIN_CONFIDENCE, MAX_ITERATIONS, AIConfig
from oxe.jsontypes import JSONDict
from oxe.search.model import SearchRequest, SearchResult, SearxResponse
from oxe.search.service import SearchService

_FRAME_ADAPTER: TypeAdapter[AnswerFrame] = TypeAdapter(cast(Any, AnswerFrame))


# -- helpers -----------------------------------------------------------------


class _FakeUrlopen:
    """Replaces ``oxe.ai.urllib.request.urlopen``; replies from a script."""

    def __init__(self, messages: list[JSONDict]) -> None:
        # Per-call list of provider message payloads.
        self._messages = messages
        self.payloads: list[JSONDict] = []

    def __call__(self, request: object, timeout: float) -> "_FakeResponse":
        del timeout
        assert isinstance(request, object)
        payload = json.loads(request.data.decode())  # type: ignore[attr-defined]
        self.payloads.append(payload)
        idx = min(len(self.payloads) - 1, len(self._messages) - 1)
        return _FakeResponse(self._messages[idx])

    def tools_payload(self) -> JSONDict:
        return self.payloads[0]


class _FakeResponse:
    def __init__(self, message: JSONDict) -> None:
        self._message = message

    def __enter__(self) -> "_FakeResponse":
        return self

    def __exit__(self, *args: object) -> None:
        pass

    def read(self) -> bytes:
        return json.dumps({"choices": [{"message": self._message}]}).encode()


def _tool_call_message(call_id: str = "t1", query: str = "cats") -> JSONDict:
    return {
        "content": None,
        "tool_calls": [
            {
                "id": call_id,
                "type": "function",
                "function": {"name": "web_search", "arguments": json.dumps({"query": query})},
            }
        ],
    }


def _final_message(confidence: int = 9, answer: str = "Cats are great.") -> JSONDict:
    tail = json.dumps({"confidence": confidence, "related_questions": ["r1"]})
    return {"content": f"{answer}\n{tail}"}


class _FakeEngine:
    name = "fake"
    timeout = 10.0

    async def search(self, req: SearchRequest) -> SearxResponse:
        r = SearchResult(url="https://example.com/a", title="A", content="text", engine="fake")
        return SearxResponse(query=req.q, number_of_results=1, results=[r])


def _tools(cache: TTLCache) -> dict[str, Any]:
    return dict(build_toolset(SearchService(_FakeEngine(), cache), cache))


async def _collect(gen: AsyncGenerator[Any, None]) -> list[Any]:
    return [frame async for frame in gen]


def _run(coro: Coroutine[Any, Any, list[Any]]) -> list[Any]:
    return asyncio.new_event_loop().run_until_complete(coro)


# -- parse_final_answer ------------------------------------------------------


def test_parse_final_answer_trailing_json() -> None:
    body, conf, related = parse_final_answer(
        'The answer is 4.\n{"confidence": 9, "related_questions": ["a", "b"]}'
    )
    assert body == "The answer is 4."
    assert conf == 9
    assert related == ["a", "b"]


def test_parse_final_answer_pretty_printed_tail() -> None:
    """Legacy defect 1: a JSON tail spread over multiple lines is found."""
    text = 'Answer.\n{\n  "confidence": 7,\n  "related_questions": ["q"]\n}'
    body, conf, related = parse_final_answer(text)
    assert body == "Answer."
    assert conf == 7
    assert related == ["q"]


def test_parse_final_answer_garbled_tail_yields_confidence_zero() -> None:
    """Legacy defect 2: a garbled tail yields confidence 0 rather than
    raising or emitting a bogus value; the body keeps the raw text
    (parse is tolerant, cleanup is the model's problem)."""
    body, conf, related = parse_final_answer('Answer.\n{"confidence": ')
    assert conf == 0
    assert related == []
    assert body.startswith("Answer.")


def test_parse_final_answer_no_tail() -> None:
    body, conf, related = parse_final_answer("Just an answer.")
    assert body == "Just an answer."
    assert conf == 0
    assert related == []


@pytest.mark.parametrize("conf_in", [-5, 0, 4, 9, 10, 11, 99])
def test_parse_final_answer_confidence_clamped_and_tail_stripped(conf_in: int) -> None:
    body, conf, related = parse_final_answer(
        f'ans\n{{"confidence": {conf_in}, "related_questions": ["r"]}}'
    )
    assert 0 <= conf <= 10
    assert len(related) <= 5
    assert "confidence" not in body


# -- tools kwarg (plan step 20) ----------------------------------------------


def test_tools_kwarg_always_sent(monkeypatch: pytest.MonkeyPatch) -> None:
    """The provider request MUST carry the tools kwarg; legacy's aisuite
    client silently dropped it (retro finding, plan step 20)."""
    fake = _FakeUrlopen([{"content": "hi"}])
    monkeypatch.setattr("oxe.ai.urllib.request.urlopen", fake)
    cfg = AIConfig(provider="openai", model="gpt-test", api_key="sk-x")
    _chat_completion(cfg, [{"role": "user", "content": "q"}])
    tools_value = fake.tools_payload()["tools"]
    assert tools_value == TOOLS_SPEC
    assert isinstance(tools_value, list)
    names: list[str] = []
    for tool in tools_value:
        if not isinstance(tool, dict):
            continue
        fn = tool.get("function")
        if isinstance(fn, dict):
            names.append(str(fn.get("name")))
    assert names == ["web_search", "user_history"]


# -- stream_answer / tool loop ----------------------------------------------


def test_stream_answer_tool_loop_cap(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """A model that only ever requests tools is cut off after
    MAX_ITERATIONS rounds with the iterations-exhausted done frame."""
    fake = _FakeUrlopen([_tool_call_message()])
    monkeypatch.setattr("oxe.ai.urllib.request.urlopen", fake)
    cfg = AIConfig(provider="openai", model="m", api_key="k")
    cache = TTLCache(tmp_path / "db")
    frames = _run(_collect(stream_answer("q", cfg, _tools(cache))))
    cache.close()
    steps = [f for f in frames if f.type == "step"]
    done = [f for f in frames if f.type == "done"]
    assert len(steps) == MAX_ITERATIONS
    assert len(done) == 1
    assert isinstance(done[0], DoneFrame)
    assert done[0].error is not None
    assert "max iterations" in done[0].error


def test_stream_answer_frame_order_after_tool_round(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """First round: tool call; second round: final answer. Asserts frame
    order step -> delta -> sources -> done and that the tool result URL
    landed in sources."""
    fake = _FakeUrlopen([_tool_call_message(), _final_message(9)])
    monkeypatch.setattr("oxe.ai.urllib.request.urlopen", fake)
    cfg = AIConfig(provider="openai", model="m", api_key="k")
    cache = TTLCache(tmp_path / "db")
    frames = _run(_collect(stream_answer("cats", cfg, _tools(cache))))
    cache.close()
    assert [f.type for f in frames] == ["step", "delta", "sources", "done"]
    assert isinstance(frames[0].query, str)
    assert frames[0].query == "cats"
    sources = frames[2]
    assert isinstance(sources, SourcesFrame)
    assert sources.sources[0].url == "https://example.com/a"
    done = frames[3]
    assert isinstance(done, DoneFrame)
    assert done.answer == "Cats are great."
    assert done.confidence == 9
    assert done.error is None


# -- route: fixtures, ai-off, request model ----------------------------------


@pytest.fixture
def client(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> TestClient:
    monkeypatch.setenv("OXE_CACHE_DIR", str(tmp_path / "cache"))
    return TestClient(create_app())


def _post(client: TestClient, body: JSONDict) -> list[AnswerFrame]:
    r = client.post("/answer", json=body)
    assert r.status_code == 200
    assert r.headers["content-type"].startswith("text/event-stream")
    frames: list[AnswerFrame] = []
    for chunk in r.text.split("\n\n"):
        data = chunk.strip()
        if data.startswith("data: "):
            frames.append(_FRAME_ADAPTER.validate_json(data[len("data: ") :]))
    return frames


def test_force_ai_off_fixture(client: TestClient) -> None:
    frames = _post(client, {"query": "q", "force": "ai-off"})
    assert len(frames) == 1
    assert isinstance(frames[0], ErrorFrame)
    assert frames[0].message == AI_OFF_MESSAGE


def test_force_error_fixture(client: TestClient) -> None:
    frames = _post(client, {"query": "q", "force": "error"})
    assert len(frames) == 1
    assert isinstance(frames[0], ErrorFrame)
    assert frames[0].message != AI_OFF_MESSAGE


def test_force_empty_fixture(client: TestClient) -> None:
    frames = _post(client, {"query": "q", "force": "empty"})
    assert [f.type for f in frames] == ["sources", "done"]
    assert isinstance(frames[0], SourcesFrame)
    assert frames[0].sources == []
    assert isinstance(frames[1], DoneFrame)
    assert frames[1].confidence == 0


def test_ai_off_when_unconfigured(client: TestClient, monkeypatch: pytest.MonkeyPatch) -> None:
    """No config file at all -> the ai-off ErrorFrame, not an HTTP error."""
    monkeypatch.setenv("OXE_CONFIG_DIR", "/nonexistent-oxe-config")
    frames = _post(client, {"query": "q"})
    assert len(frames) == 1
    assert isinstance(frames[0], ErrorFrame)
    assert frames[0].message == AI_OFF_MESSAGE


def test_answer_request_body_extra_forbidden() -> None:
    with pytest.raises(ValidationError):
        AnswerRequest(query="q", unexpected=1)  # type: ignore[call-arg]


# -- answer caching ----------------------------------------------------------


def test_answer_cache_hit_roundtrip(tmp_path: Path) -> None:
    """A cached >=CACHE_MIN_CONFIDENCE answer replays
    DoneFrame(cached=True) then SourcesFrame, with no provider call."""
    done = DoneFrame(
        answer="A", related_questions=["r"], confidence=9, model="openai:m", cached=False
    )
    sources = SourcesFrame(sources=[])
    key = answer_cache_key("q", "openai:m")
    cache = TTLCache(tmp_path / "db")
    cache.put_answer(
        key,
        "q",
        cast("JSONDict", CachedAnswer(done=done, sources=sources).model_dump(mode="json")),
        3600,
        "openai:m",
    )
    cfg = AIConfig(provider="openai", model="m")
    frames = _run(_collect(_answer_frames(AnswerRequest(query="q"), cfg, None, cache)))  # type: ignore[arg-type]
    cache.close()
    assert [f.type for f in frames] == ["done", "sources"]
    assert frames[0].cached is True
    assert frames[0].answer == "A"


def test_answer_low_confidence_not_cached(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    fake = _FakeUrlopen([_final_message(CACHE_MIN_CONFIDENCE - 1)])
    monkeypatch.setattr("oxe.ai.urllib.request.urlopen", fake)
    cfg = AIConfig(provider="openai", model="m", api_key="k")
    cache = TTLCache(tmp_path / "db")
    frames = _run(
        _collect(
            _answer_frames(
                AnswerRequest(query="q"), cfg, SearchService(_FakeEngine(), cache), cache
            )
        )
    )
    assert frames[-1].confidence == CACHE_MIN_CONFIDENCE - 1
    assert cache.get_answer(answer_cache_key("q", "openai:m")) is None
    cache.close()


def test_answer_high_confidence_cached(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    fake = _FakeUrlopen([_final_message(9)])
    monkeypatch.setattr("oxe.ai.urllib.request.urlopen", fake)
    cfg = AIConfig(provider="openai", model="m", api_key="k")
    cache = TTLCache(tmp_path / "db")
    _run(
        _collect(
            _answer_frames(
                AnswerRequest(query="q"), cfg, SearchService(_FakeEngine(), cache), cache
            )
        )
    )
    cached = cache.get_answer(answer_cache_key("q", "openai:m"))
    assert cached is not None
    CachedAnswer.model_validate(cached)
    cache.close()


def test_answer_error_done_not_cached(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """A done frame carrying an error (provider failure) is never cached."""
    cfg = AIConfig(provider="openai", model="m", api_key="k")
    cache = TTLCache(tmp_path / "db")

    def boom(request: object, timeout: float) -> object:
        del request, timeout
        msg = "connection refused"
        raise urllib.error.URLError(msg)

    monkeypatch.setattr("oxe.ai.urllib.request.urlopen", boom)
    frames = _run(
        _collect(
            _answer_frames(
                AnswerRequest(query="q"), cfg, SearchService(_FakeEngine(), cache), cache
            )
        )
    )
    assert len(frames) == 1
    done = frames[0]
    assert isinstance(done, DoneFrame)
    assert done.error is not None
    assert cache.get_answer(answer_cache_key("q", "openai:m")) is None
    cache.close()
