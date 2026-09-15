"""Tests for GET/PUT /settings endpoints."""

import pytest
from fastapi.testclient import TestClient

from oxe import ai as ai_mod
from oxe.config import AIConfig
from oxe.server import make_app


@pytest.fixture
def client(tmp_path, monkeypatch):
    monkeypatch.setenv("OXE_CONFIG_DIR", str(tmp_path))
    from oxe.cache import TTLCache

    app = make_app(cache=TTLCache(str(tmp_path / "db")), backend=object())
    return TestClient(app)


def test_settings_unconfigured(client):
    res = client.get("/settings")
    assert res.status_code == 200
    body = res.json()
    assert body["configured"] is False
    assert body["config_path"].endswith("config.toml")


def test_settings_roundtrip(client):
    res = client.put(
        "/settings",
        json={
            "ai": {
                "provider": "openai",
                "model": "gpt-4o-mini",
                "api_key": "sk-test",
            }
        },
    )
    assert res.status_code == 200
    assert res.json()["ok"] is True

    res = client.get("/settings")
    body = res.json()
    assert body["configured"] is True
    assert body["ai"]["provider"] == "openai"
    assert body["ai"]["model"] == "gpt-4o-mini"
    assert body["ai"]["api_key_set"] is True
    assert "api_key" not in body["ai"]  # redacted


def test_settings_put_key_omitted_keeps_stored(client):
    client.put(
        "/settings",
        json={
            "ai": {
                "provider": "openai",
                "model": "m1",
                "api_key": "sk-keep",
            }
        },
    )
    client.put("/settings", json={"ai": {"provider": "openai", "model": "m2"}})
    from oxe.config import load_config

    cfg = load_config()
    assert cfg.model == "m2"
    assert cfg.api_key == "sk-keep"


def test_settings_put_validation(client):
    assert client.put("/settings", json={"ai": {"provider": "openai"}}).status_code == 422
    assert client.put("/settings", json={}).status_code == 422
    assert (
        client.put(
            "/settings",
            json={
                "ai": {
                    "provider": "nope",
                    "model": "m",
                }
            },
        ).status_code
        == 422
    )  # unknown provider


def test_settings_bad_config_returns_500(client, tmp_path):
    (tmp_path / "config.toml").write_text("[ai]\nprovider = 123\n")
    assert client.get("/settings").status_code == 500


def test_save_config_toml_roundtrip(tmp_path):
    from oxe.config import load_config, save_config

    cfg = AIConfig(
        provider="anthropic",
        model="claude",
        api_key_env="ANTHROPIC_API_KEY",
        base_url="https://x",
        enabled=False,
    )
    p = save_config(cfg, tmp_path / "config.toml")
    loaded = load_config(p)  # enabled=False -> None
    assert loaded is None
    import tomllib

    data = tomllib.loads(p.read_text())
    assert data["ai"]["provider"] == "anthropic"
    assert data["ai"]["enabled"] is False


def test_env_interpolation(tmp_path, monkeypatch):
    from oxe.config import load_config

    monkeypatch.setenv("MY_KEY", "sk-from-env")
    (tmp_path / "config.toml").write_text(
        '[ai]\nprovider = "openai"\nmodel = "m"\napi_key = "{env.MY_KEY}"\n'
    )
    cfg = load_config(tmp_path / "config.toml")
    assert cfg.api_key == "sk-from-env"


def test_env_interpolation_missing_var(tmp_path, monkeypatch):
    from oxe.config import ConfigError, load_config

    monkeypatch.delenv("NOPE_VAR", raising=False)
    (tmp_path / "config.toml").write_text(
        '[ai]\nprovider = "openai"\nmodel = "m"\nbase_url = "{env.NOPE_VAR}"\n'
    )
    with pytest.raises(ConfigError, match="NOPE_VAR"):
        load_config(tmp_path / "config.toml")


def test_load_dotenv(tmp_path, monkeypatch):
    from oxe.config import load_dotenv

    (tmp_path / ".env").write_text("A=1\n# c\nB = two \n\nA_EXIST=kept\n")
    monkeypatch.setenv("A_EXIST", "original")
    monkeypatch.chdir(tmp_path)
    load_dotenv()
    import os

    assert os.environ["A"] == "1"
    assert os.environ["B"] == "two"
    assert os.environ["A_EXIST"] == "original"


def test_ac_proxy_empty(client):
    assert client.get("/ac?q=").json() == []


def test_suggest_empty(client):
    body = client.get("/suggest", params={"q": " "}).json()
    assert body == ["", [], [], []]  # whitespace-only prefix normalized to empty


def test_row_delete_idempotent_for_api_clients(client):
    # non-HTML clients get 204 on missing row (UI refresh flow), HTML still redirects
    r = client.post("/row/doesnotexist/delete", headers={"accept": "application/json"})
    assert r.status_code == 204


# -- POST /settings/test -------------------------------------------------------

from unittest.mock import MagicMock, patch


def _put_cfg(client):
    client.put(
        "/settings",
        json={
            "ai": {
                "provider": "openai",
                "model": "gpt-4o-mini",
                "api_key": "sk-test",
            }
        },
    )


def test_settings_test_requires_provider_and_model(client):
    assert client.post("/settings/test", json={"ai": {}}).status_code == 422
    assert client.post("/settings/test", json={"ai": {"provider": "openai"}}).status_code == 422
    assert client.post("/settings/test", json={}).status_code == 422


def _patch_openai(completion=None, listing=None):
    """Patch oxe.ai's lazy `from openai import OpenAI` with a fake client."""
    fake_mod = MagicMock()
    client_inst = fake_mod.OpenAI.return_value
    if completion is not None:
        client_inst.chat.completions.create = completion
    if listing is not None:
        client_inst.models.list = listing
    return patch.dict("sys.modules", {"openai": fake_mod})


def test_settings_test_ok_completion(client):
    _put_cfg(client)
    with _patch_openai(completion=lambda **kw: MagicMock()):
        res = client.post(
            "/settings/test", json={"ai": {"provider": "openai", "model": "gpt-4o-mini"}}
        )
    assert res.status_code == 200
    body = res.json()
    assert body["ok"] is True
    assert "completion succeeded" in body["detail"]


def test_settings_test_bad_model_list_ok(client):
    """Completion 404 + successful listing -> ok=False with a model hint."""
    _put_cfg(client)

    def completion_fail(**kw):
        err = Exception("Error code: 404 - model 'openrouter/free' not found")
        raise err

    listing = MagicMock(return_value=MagicMock(data=[MagicMock()]))
    with _patch_openai(completion=completion_fail, listing=listing):
        res = client.post("/settings/test", json={"ai": {"provider": "openai", "model": "nope"}})
    body = res.json()
    assert body["ok"] is False
    assert "not found" in body["detail"]


class _FakeErr(Exception):
    pass


def test_settings_test_bad_key_fails_both(client):
    _put_cfg(client)

    def fail401(**kw):
        raise _FakeErr("Error code: 401 - invalid api key")

    def list_fail():
        raise _FakeErr("Error code: 401 - invalid api key")

    with _patch_openai(completion=fail401, listing=list_fail):
        res = client.post("/settings/test", json={"ai": {"provider": "openai", "model": "m"}})
    body = res.json()
    assert body["ok"] is False
    assert "401" in body["detail"]


def test_settings_test_no_key(client):
    res = client.post("/settings/test", json={"ai": {"provider": "openai", "model": "m"}})
    body = res.json()
    assert body["ok"] is False
    assert "api key" in body["detail"]


# -- text-embedded tool calls (openrouter-style) --------------------------------

CFG2 = AIConfig(provider="openai", model="gpt-test", api_key="sk-test")


def _run(coro):
    import asyncio

    return asyncio.run(coro)


async def _collect(gen):
    return [e async for e in gen]


def _fake_client_one(text):
    """Fake aisuite client streaming one text-only turn."""
    from types import SimpleNamespace

    chunks = [
        SimpleNamespace(
            choices=[SimpleNamespace(delta=SimpleNamespace(content=c, tool_calls=None))]
        )
        for c in text
    ]

    class S:
        def __iter__(self):
            return iter(chunks)

    def create(**kw):
        return S()

    return SimpleNamespace(chat=SimpleNamespace(completions=SimpleNamespace(create=create)))


def test_stream_answer_text_tool_calls_surfaces_model_error(monkeypatch):
    """A model that emits <tool_call> tags as text gets a clear model error."""
    client = _fake_client_one(
        ["<tool_call>web_search\n<arg_key>query</arg_key>\n<arg_value>q</arg_value></tool_call>"]
    )
    monkeypatch.setattr(ai_mod, "_sdk_client", lambda cfg: (client, "openai:badmodel", "openai"))
    events = _run(_collect(ai_mod.stream_answer("q", CFG2)))
    done = events[-1]
    assert done["type"] == "done"
    assert done["confidence"] == 0
    assert "does not support tool calling" in done["error"]
    # raw tool-call text is never streamed as the answer
    assert all("<tool_call>" not in (e.get("text") or "") for e in events)


def test_friendly_provider_error_mapping():
    from oxe.ai import _friendly_provider_error

    assert "401" in _friendly_provider_error(Exception("Error code: 401 - no auth"), CFG2)
    assert "not found" in _friendly_provider_error(Exception("404 model missing"), CFG2)
    assert "rate limited" in _friendly_provider_error(Exception("429 too many"), CFG2)
    assert "credits" in _friendly_provider_error(Exception("402 insufficient credits"), CFG2)
    assert "provider error" in _friendly_provider_error(Exception("weird"), CFG2)


def test_load_dotenv_from_config_dir(tmp_path, monkeypatch):
    from oxe.config import load_dotenv

    conf_dir = tmp_path / "conf"
    conf_dir.mkdir()
    (conf_dir / ".env").write_text("FROM_CONF=yes\n")
    monkeypatch.setenv("OXE_CONFIG_DIR", str(conf_dir))
    monkeypatch.chdir(tmp_path)  # no .env in cwd
    load_dotenv()
    import os

    assert os.environ["FROM_CONF"] == "yes"


def test_interpolated_key_survives_settings_roundtrip(client, monkeypatch):
    from oxe.config import load_config

    monkeypatch.setenv("MY_SECRET", "sk-hidden")
    from pathlib import Path

    import oxe.config as cfg_mod

    p = Path(client.app.state.__dict__.get("_cfg_path", cfg_mod.config_path()))
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text('[ai]\nprovider = "openai"\nmodel = "m"\napi_key = "{env.MY_SECRET}"\n')
    # PUT without touching the key: template must survive, not plaintext
    r = client.put("/settings", json={"ai": {"provider": "openai", "model": "m2"}})
    assert r.status_code == 200
    toml = p.read_text()
    assert "{env.MY_SECRET}" in toml
    assert "sk-hidden" not in toml
    cfg = load_config(p)
    assert cfg.model == "m2"
    assert cfg.api_key == "sk-hidden"


def test_changed_literal_key_saved_as_literal(client, monkeypatch):
    import oxe.config as cfg_mod

    monkeypatch.setenv("MY_SECRET2", "sk-old")
    p = cfg_mod.config_path()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text('[ai]\nprovider = "openai"\nmodel = "m"\napi_key = "{env.MY_SECRET2}"\n')
    r = client.put(
        "/settings",
        json={
            "ai": {
                "provider": "openai",
                "model": "m",
                "api_key": "sk-literal",
            }
        },
    )
    assert r.status_code == 200
    toml = p.read_text()
    assert "sk-literal" in toml
    assert "{env." not in toml
