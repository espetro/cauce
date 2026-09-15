"""Tests for GET/PUT /settings endpoints."""

import pytest
from fastapi.testclient import TestClient

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
    res = client.put("/settings", json={"ai": {
        "provider": "openai", "model": "gpt-4o-mini", "api_key": "sk-test",
    }})
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
    client.put("/settings", json={"ai": {
        "provider": "openai", "model": "m1", "api_key": "sk-keep",
    }})
    client.put("/settings", json={"ai": {"provider": "openai", "model": "m2"}})
    from oxe.config import load_config
    cfg = load_config()
    assert cfg.model == "m2"
    assert cfg.api_key == "sk-keep"


def test_settings_put_validation(client):
    assert client.put("/settings", json={"ai": {"provider": "openai"}}).status_code == 422
    assert client.put("/settings", json={}).status_code == 422
    assert client.put("/settings", json={"ai": {
        "provider": "nope", "model": "m",
    }}).status_code == 422  # unknown provider


def test_settings_bad_config_returns_500(client, tmp_path):
    (tmp_path / "config.toml").write_text("[ai]\nprovider = 123\n")
    assert client.get("/settings").status_code == 500


def test_save_config_toml_roundtrip(tmp_path):
    from oxe.config import load_config, save_config

    cfg = AIConfig(provider="anthropic", model="claude", api_key_env="ANTHROPIC_API_KEY",
                   base_url="https://x", enabled=False)
    p = save_config(cfg, tmp_path / "config.toml")
    loaded = load_config(p)  # enabled=False -> None
    assert loaded is None
    import tomllib
    data = tomllib.loads(p.read_text())
    assert data["ai"]["provider"] == "anthropic"
    assert data["ai"]["enabled"] is False
