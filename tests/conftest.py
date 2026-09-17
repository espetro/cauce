"""Shared pytest fixtures.

Redirects ``OXE_CACHE_DIR`` and ``OXE_CONFIG_DIR`` to a tmp path before any
``oxe`` module is imported, so tests never touch a real user cache/config dir.
See ``.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md`` day-0 gate 9.
"""

import os
import tempfile
from pathlib import Path

_tmp_root = tempfile.mkdtemp(prefix="oxe-test-")
os.environ["OXE_CACHE_DIR"] = str(Path(_tmp_root) / "cache")
os.environ["OXE_CONFIG_DIR"] = str(Path(_tmp_root) / "config")

import pytest  # noqa: E402


@pytest.fixture(autouse=True)
def _isolate_oxe_dirs(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Give every test its own fresh cache/config dir.

    The module-level redirection above only guards against touching the real
    user dirs; state written into the shared tmp dirs by one test (notably
    schemathesis fuzzing ``PUT /settings``, which persists arbitrary payloads
    via ``save_config``) would leak into later tests' ``load_config()`` and
    surface as ``config_error`` 500s in an order-dependent way.
    """
    monkeypatch.setenv("OXE_CACHE_DIR", str(tmp_path / "cache"))
    monkeypatch.setenv("OXE_CONFIG_DIR", str(tmp_path / "config"))


@pytest.fixture
def oxe_tmp_dirs() -> dict[str, Path]:
    """Expose the redirected cache/config dirs to tests that need them."""
    return {
        "cache": Path(os.environ["OXE_CACHE_DIR"]),
        "config": Path(os.environ["OXE_CONFIG_DIR"]),
    }
