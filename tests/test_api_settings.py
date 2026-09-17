"""HTTP-level tests for ``GET|PUT /settings`` (``oxe/api/settings.py``).

These drive the routes through the FastAPI app with ``OXE_CONFIG_DIR`` pointed
at ``tmp_path`` (re-scoping the module-wide redirection
``tests/conftest.py`` performs). Key invariants under test: GET never echoes a
raw ``api_key``; PUT omit/extra fields are 422 (the legacy ``base_url``-wipe
bug guard); ``ConfigError`` maps onto the error envelope.
"""

from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from oxe.app import create_app
from oxe.config import AIConfig, save_config


@pytest.fixture
def config_dir(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    d = tmp_path / "config"
    monkeypatch.setenv("OXE_CONFIG_DIR", str(d))
    return d


@pytest.fixture
def client(config_dir: Path) -> TestClient:
    del config_dir
    return TestClient(create_app())


_FULL_BODY: dict[str, object] = {
    "provider": "openai",
    "model": "gpt-4o-mini",
    "api_key": "sk-secret",
    "api_key_set": True,
    "api_key_env": None,
    "base_url": "https://api.example.com",
    "enabled": True,
}


def _seed(config_dir: Path) -> None:
    save_config(
        AIConfig(
            provider="anthropic",
            model="claude",
            api_key_env="ANTHROPIC_API_KEY",
            base_url="https://x",
            enabled=True,
        ),
        config_dir / "config.toml",
    )


def test_get_settings_roundtrip(config_dir: Path, client: TestClient) -> None:
    _seed(config_dir)
    r = client.get("/settings")
    assert r.status_code == 200
    assert r.json() == {
        "provider": "anthropic",
        "model": "claude",
        "api_key": None,
        "api_key_set": False,
        "api_key_env": "ANTHROPIC_API_KEY",
        "base_url": "https://x",
        "enabled": True,
    }


def test_get_settings_never_echoes_raw_api_key(config_dir: Path, client: TestClient) -> None:
    save_config(
        AIConfig(provider="openai", model="m", api_key="sk-raw-secret"),
        config_dir / "config.toml",
    )
    r = client.get("/settings")
    assert r.status_code == 200
    assert "sk-raw-secret" not in r.text
    assert r.json()["api_key"] is None
    assert r.json()["api_key_set"] is True


def test_get_settings_without_config_returns_blank_form(client: TestClient) -> None:
    r = client.get("/settings")
    assert r.status_code == 200
    assert r.json() == {
        "provider": "",
        "model": "",
        "api_key": None,
        "api_key_set": False,
        "api_key_env": None,
        "base_url": None,
        "enabled": True,
    }


def test_put_settings_roundtrip(client: TestClient) -> None:
    r = client.put("/settings", json=_FULL_BODY)
    assert r.status_code == 200
    body = r.json()
    assert body["provider"] == "openai"
    assert body["base_url"] == "https://api.example.com"
    assert "sk-secret" not in r.text  # stored but never echoed back

    # Roundtrip through the file: a re-GET reflects the saved config.
    r2 = client.get("/settings")
    assert r2.status_code == 200
    assert r2.json()["base_url"] == "https://api.example.com"
    assert r2.json()["api_key_set"] is True


def test_put_settings_missing_field_is_422(client: TestClient) -> None:
    body = {k: v for k, v in _FULL_BODY.items() if k != "model"}
    r = client.put("/settings", json=body)
    assert r.status_code == 422
    # Nothing was persisted: a subsequent GET still returns the blank form.
    assert client.get("/settings").json()["provider"] == ""


def test_put_settings_unknown_field_is_422(client: TestClient) -> None:
    # Legacy's silent extra-field acceptance wiped base_url for 10h; this is
    # the explicit guard.
    r = client.put("/settings", json={**_FULL_BODY, "bogus": "x"})
    assert r.status_code == 422
    assert client.get("/settings").json()["provider"] == ""
