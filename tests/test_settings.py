"""Tests for oxe.config (AIConfig load/save/env-interpolation).

Legacy's test_settings.py also covered the /settings and /settings/test HTTP
endpoints via oxe.server + oxe.ai; those routes are out of scope for this
port (oxe/api/ and oxe/ai.py land in later wave-2 steps). This file keeps
only the subset that exercises oxe.config directly.
"""

import os
from pathlib import Path

import pytest
import tomllib

from oxe.config import AIConfig, ConfigError, load_config, load_dotenv, save_config


def test_save_config_toml_roundtrip(tmp_path: Path) -> None:
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

    data = tomllib.loads(p.read_text())
    ai = data["ai"]
    assert isinstance(ai, dict)
    assert ai["provider"] == "anthropic"
    assert ai["enabled"] is False


def test_env_interpolation(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("MY_KEY", "sk-from-env")
    (tmp_path / "config.toml").write_text(
        '[ai]\nprovider = "openai"\nmodel = "m"\napi_key = "{env.MY_KEY}"\n'
    )
    cfg = load_config(tmp_path / "config.toml")
    assert cfg is not None
    assert cfg.api_key == "sk-from-env"


def test_env_interpolation_missing_var(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("NOPE_VAR", raising=False)
    (tmp_path / "config.toml").write_text(
        '[ai]\nprovider = "openai"\nmodel = "m"\nbase_url = "{env.NOPE_VAR}"\n'
    )
    with pytest.raises(ConfigError, match="NOPE_VAR"):
        load_config(tmp_path / "config.toml")


def test_load_dotenv(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    (tmp_path / ".env").write_text("A=1\n# c\nB = two \n\nA_EXIST=kept\n")
    monkeypatch.setenv("A_EXIST", "original")
    monkeypatch.chdir(tmp_path)
    load_dotenv()

    assert os.environ["A"] == "1"
    assert os.environ["B"] == "two"
    assert os.environ["A_EXIST"] == "original"


def test_load_dotenv_from_config_dir(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    conf_dir = tmp_path / "conf"
    conf_dir.mkdir()
    (conf_dir / ".env").write_text("FROM_CONF=yes\n")
    monkeypatch.setenv("OXE_CONFIG_DIR", str(conf_dir))
    monkeypatch.chdir(tmp_path)  # no .env in cwd
    load_dotenv()

    assert os.environ["FROM_CONF"] == "yes"


def test_unknown_provider_rejected(tmp_path: Path) -> None:
    (tmp_path / "config.toml").write_text('[ai]\nprovider = "nope"\nmodel = "m"\n')
    with pytest.raises(ConfigError, match="not supported"):
        load_config(tmp_path / "config.toml")


def test_unknown_key_rejected(tmp_path: Path) -> None:
    (tmp_path / "config.toml").write_text('[ai]\nprovider = "openai"\nmodel = "m"\nbogus = "x"\n')
    with pytest.raises(ConfigError, match=r"unknown \[ai\] keys"):
        load_config(tmp_path / "config.toml")


def test_missing_file_is_none(tmp_path: Path) -> None:
    assert load_config(tmp_path / "nope.toml") is None
