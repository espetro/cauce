"""AI pipeline tests. No network; aisuite is never imported (mocked provider).

The loop is driven through stream_answer with a fake aisuite client injected
by monkeypatching oxe.ai._aisuite_client, so the tests run with or without
aisuite installed.
"""

import asyncio
import json
from types import SimpleNamespace

import pytest

from oxe import ai as ai_mod
from oxe.cache import TTLCache
from oxe.config import AIConfig, ConfigError, load_config

CFG = AIConfig(provider="openai", model="gpt-test", api_key="sk-test")


# -- config loader ------------------------------------------------------------


def test_load_config_missing_file(tmp_path):
    assert load_config(tmp_path / "nope.toml") is None


def _write(tmp_path, body):
    p = tmp_path / "config.toml"
    p.write_text(body)
    return p


def test_load_config_minimal(tmp_path):
    cfg = load_config(_write(tmp_path, '[ai]\nprovider = "openai"\nmodel = "gpt-4o-mini"\n'))
    assert cfg == AIConfig(provider="openai", model="gpt-4o-mini")


def test_load_config_disabled_means_off(tmp_path):
    cfg = load_config(_write(tmp_path, '[ai]\nprovider = "openai"\nmodel = "m"\nenabled = false\n'))
    assert cfg is None


def test_load_config_malformed_toml(tmp_path):
    with pytest.raises(ConfigError, match="invalid TOML"):
        load_config(_write(tmp_path, "[ai\nprovider ="))


def test_load_config_missing_model(tmp_path):
    with pytest.raises(ConfigError, match="model"):
        load_config(_write(tmp_path, '[ai]\nprovider = "openai"\n'))


def test_load_config_unknown_keys(tmp_path):
    with pytest.raises(ConfigError, match="unknown"):
        load_config(_write(tmp_path, '[ai]\nprovider = "openai"\nmodel = "m"\nbogus = 1\n'))


def test_load_config_bad_provider(tmp_path):
    with pytest.raises(ConfigError, match="provider"):
        load_config(_write(tmp_path, '[ai]\nprovider = "nope"\nmodel = "m"\n'))


def test_load_config_env_key_resolution(tmp_path, monkeypatch):
    monkeypatch.setenv("MY_KEY", "sk-123")
    cfg = load_config(
        _write(tmp_path, '[ai]\nprovider = "openai"\nmodel = "m"\napi_key_env = "MY_KEY"\n')
    )
    assert cfg.resolve_api_key() == "sk-123"


# -- answer parsing -----------------------------------------------------------


def test_parse_final_answer_with_json_line():
    text = 'Answer here.\n{"confidence": 9, "related_questions": ["a", "b"]}'
    body, conf, related = ai_mod.parse_final_answer(text)
    assert body == "Answer here."
    assert conf == 9
    assert related == ["a", "b"]


def test_parse_final_answer_without_json():
    body, conf, related = ai_mod.parse_final_answer("Just text, no JSON.")
    assert body == "Just text, no JSON."
    assert conf == 0
    assert related == []


def test_parse_final_answer_garbled_confidence_only():
    body, conf, _ = ai_mod.parse_final_answer('Body. [1]\n{"confidence": 7}')
    assert body == 'Body. [1]\n{"confidence": 7}'
    assert conf == 0


# -- loop behaviour -----------------------------------------------------------


def _fake_client(responses):
    """Build a fake aisuite client whose create() returns scripted turns.

    Each response is (streamed_text_chunks, tool_calls) where tool_calls is a
    list of {id, name, arguments} dicts (or None for a final text answer).
    """
    turns = list(responses)

    class FakeStream:
        def __init__(self, chunks, tool_calls):
            self._chunks = chunks
            self.tool_calls_acc = list(tool_calls) if tool_calls else None

        def __iter__(self):
            return iter(self._chunks)

    def create(model=None, messages=None, tools=None, stream=None, **kw):
        text, tool_calls = turns.pop(0)
        chunks = [
            SimpleNamespace(
                choices=[SimpleNamespace(delta=SimpleNamespace(content=c, tool_calls=None))]
            )
            for c in text
        ]
        if tool_calls:
            tc_parts = [
                SimpleNamespace(
                    id=t["id"],
                    index=i,
                    function=SimpleNamespace(name=t["name"], arguments=t["arguments"]),
                )
                for i, t in enumerate(tool_calls)
            ]
            chunks.append(
                SimpleNamespace(
                    choices=[
                        SimpleNamespace(delta=SimpleNamespace(content=None, tool_calls=tc_parts))
                    ]
                )
            )
        return FakeStream(chunks, tool_calls)

    return SimpleNamespace(chat=SimpleNamespace(completions=SimpleNamespace(create=create)))


async def _collect(gen):
    return [e async for e in gen]


def _run(coro):
    return asyncio.run(coro)


def test_loop_confidence_stop_and_event_sequence(monkeypatch):
    client = _fake_client(
        [
            (
                ["Here is", ' the answer.\n{"confidence": 9, "related_questions": ["q1"]}'],
                None,
            )
        ]
    )
    monkeypatch.setattr(ai_mod, "_sdk_client", lambda cfg: (client, "openai:gpt-test", "openai"))
    events = _run(_collect(ai_mod.stream_answer("q", CFG)))
    types = [e["type"] for e in events]
    assert types == ["delta", "sources", "done"]
    done = events[-1]
    assert done["confidence"] == 9
    assert done["answer"] == "Here is the answer."
    assert done["related_questions"] == ["q1"]
    assert done["model"] == "openai:gpt-test"
    assert done["cached"] is False


def test_loop_tool_execution_and_max_iterations(monkeypatch):
    calls = {"n": 0}

    def web_search(query, source="web", num_results=5):
        calls["n"] += 1
        return {"results": [{"title": "T", "url": "https://x", "text": "body"}]}

    tool_call = [{"id": "t1", "name": "web_search", "arguments": '{"query": "q"}'}]
    never_confident = (["thinking..."], tool_call)
    client = _fake_client([never_confident] * 5)
    monkeypatch.setattr(ai_mod, "_sdk_client", lambda cfg: (client, "openai:gpt-test", "openai"))
    monkeypatch.setattr(ai_mod, "build_toolset", lambda *a: {"web_search": web_search})
    events = _run(_collect(ai_mod.stream_answer("q", CFG, max_iterations=5)))
    # one step event per search across 5 iterations, then exhausted done
    assert calls["n"] == 5
    assert [e["type"] for e in events].count("step") == 5
    done = events[-1]
    assert done["type"] == "done"
    assert done["confidence"] == 0
    assert "max iterations" in done.get("error", "")


def test_loop_unknown_tool_and_bad_args(monkeypatch):
    tool_call = [{"id": "t9", "name": "nope", "arguments": "not-json"}]
    client = _fake_client(
        [
            ([".."], tool_call),
            (['done.\n{"confidence": 8, "related_questions": []}'], None),
        ]
    )
    monkeypatch.setattr(ai_mod, "_sdk_client", lambda cfg: (client, "openai:gpt-test", "openai"))
    events = _run(_collect(ai_mod.stream_answer("q", CFG)))
    done = events[-1]
    assert done["confidence"] == 8
    # transcript contained the unknown-tool error without crashing


def test_sources_collected_from_tool_results(monkeypatch):
    tool_call = [{"id": "t1", "name": "web_search", "arguments": '{"query": "q"}'}]
    client = _fake_client(
        [
            (["."], tool_call),
            (['final.\n{"confidence": 8, "related_questions": []}'], None),
        ]
    )

    def web_search(query, source="web", num_results=5):
        return {
            "results": [
                {"title": "T", "url": "https://example.com/a", "text": "x"},
                {"title": "T", "url": "https://example.com/a", "text": "dup"},
            ]
        }

    monkeypatch.setattr(ai_mod, "_sdk_client", lambda cfg: (client, "openai:gpt-test", "openai"))
    monkeypatch.setattr(ai_mod, "build_toolset", lambda *a: {"web_search": web_search})
    events = _run(_collect(ai_mod.stream_answer("q", CFG)))
    sources_ev = next(e for e in events if e["type"] == "sources")
    assert sources_ev["sources"] == [{"title": "T", "url": "https://example.com/a"}]


def test_answer_cache_key_stable_and_model_scoped():
    assert ai_mod.answer_cache_key("Q", "openai:m") == ai_mod.answer_cache_key("q ", "openai:m")
    assert ai_mod.answer_cache_key("q", "openai:m") != ai_mod.answer_cache_key("q", "anthropic:m")


# -- tools --------------------------------------------------------------------


def test_web_search_tool_wraps_do_search(tmp_path):

    c = TTLCache(tmp_path / "c.db")

    class StubBackend:
        name = "stub"

        def search(self, req):
            return {
                "requestId": "r",
                "searchType": "auto",
                "results": [
                    {
                        "title": "T",
                        "url": "https://example.com/s",
                        "text": "b",
                        "highlights": [],
                        "highlightScores": [],
                    }
                ],
                "costDollars": {"total": 0.0},
            }

    tools = ai_mod.build_toolset(c, backend=StubBackend(), on_result=None)
    assert set(tools) == {"web_search", "user_history"}
    hist = tools["user_history"](query="python")
    assert hist == {"clicks": [], "count": 0}
    bad = tools["web_search"](query="x", source="bogus")  # source falls back to web
    assert bad["source"] in ("web", "network")
    c.close()


# -- HTTP layer ----------------------------------------------------------------

from fastapi.testclient import TestClient

from oxe.server import make_app


def _client(tmp_path):
    c = TTLCache(tmp_path / "c.db")
    app = make_app(cache=c, backend=object())
    return c, TestClient(app)


def test_answer_requires_query(tmp_path):
    _, client = _client(tmp_path)
    r = client.post("/answer", json={})
    assert r.status_code == 422


def test_answer_unconfigured_returns_409(tmp_path, monkeypatch):
    monkeypatch.delenv("OXE_CONFIG_DIR", raising=False)
    monkeypatch.setattr("oxe.server.load_config_path", None, raising=False)
    import oxe.config as cfg_mod

    monkeypatch.setattr(cfg_mod, "config_path", lambda: tmp_path / "absent.toml")
    c, client = _client(tmp_path)
    # server's load_config is imported inside the handler from .config

    r = client.post("/answer", json={"query": "hello"})
    assert r.status_code == 409
    assert c.answer_stats()["rows"] == 0


def test_answer_cache_hit_single_done_event(tmp_path):
    c, client = _client(tmp_path)
    from oxe import ai as ai_mod

    key = ai_mod.answer_cache_key("hello", "openai:gpt-test")
    final = {
        "type": "done",
        "answer": "Hi",
        "related_questions": [],
        "confidence": 9,
        "model": "openai:gpt-test",
        "cached": False,
    }
    c.put_answer(key, "hello", final, ttl=3600, model="openai:gpt-test")
    # make AI look configured by pre-seeding the cache; config load will return
    # None, so we patch load_config inside oxe.server's import path
    import oxe.config as cfg_mod

    orig = cfg_mod.load_config
    cfg_mod.load_config = lambda path=None: AIConfig(
        provider="openai", model="gpt-test", api_key="k"
    )
    try:
        r = client.post("/answer", json={"query": "hello"})
    finally:
        cfg_mod.load_config = orig
    assert r.status_code == 200
    assert r.headers["content-type"].startswith("text/event-stream")
    events = [json.loads(l[6:]) for l in r.text.splitlines() if l.startswith("data: ")]
    assert len(events) == 1
    assert events[0]["cached"] is True
    assert events[0]["answer"] == "Hi"


def _patch_stream(monkeypatch, final_event):
    """Patch ai.stream_answer + config for the /answer HTTP tests."""
    import oxe.config as cfg_mod

    async def fake_stream(query, cfg, **kw):
        yield {"type": "delta", "text": final_event.get("answer", "")}
        yield final_event

    monkeypatch.setattr(ai_mod, "stream_answer", fake_stream)
    monkeypatch.setattr(
        cfg_mod,
        "load_config",
        lambda path=None: AIConfig(provider="openai", model="gpt-test", api_key="k"),
    )


def test_answer_low_confidence_not_cached(tmp_path, monkeypatch):
    c, client = _client(tmp_path)
    final = {
        "type": "done",
        "answer": "some answer",
        "related_questions": [],
        "confidence": 3,
        "model": "openai:gpt-test",
        "cached": False,
    }
    _patch_stream(monkeypatch, final)
    r = client.post("/answer", json={"query": "q"})
    assert r.status_code == 200
    assert c.answer_stats()["rows"] == 0


def test_answer_high_confidence_cached(tmp_path, monkeypatch):
    c, client = _client(tmp_path)
    final = {
        "type": "done",
        "answer": "a real answer",
        "related_questions": [],
        "confidence": 5,
        "model": "openai:gpt-test",
        "cached": False,
    }
    _patch_stream(monkeypatch, final)
    r = client.post("/answer", json={"query": "q"})
    assert r.status_code == 200
    assert c.answer_stats()["rows"] == 1


def test_greeting_guard_flags_error():
    done_err = ai_mod._greeting_error("Hello! How can I help you today?", 0)
    assert done_err == "model did not answer the query - try a different model"
    assert ai_mod._greeting_error("Paris is the capital of France.", 0) is None
    assert ai_mod._greeting_error("How can I help?", 5) is None


def test_stream_answer_greeting_sets_done_error(monkeypatch):
    client = _fake_client([(["Hello! How can I help you today?"], None)])
    monkeypatch.setattr(ai_mod, "_sdk_client", lambda cfg: (client, "openai:gpt-test", "openai"))
    events = _run(_collect(ai_mod.stream_answer("capital of france", CFG)))
    done = events[-1]
    assert done["type"] == "done"
    assert done["confidence"] == 0
    assert done["error"] == "model did not answer the query - try a different model"
    assert done["answer"]  # text still streamed/kept
