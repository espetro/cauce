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


@pytest.fixture
def oxe_tmp_dirs() -> dict[str, Path]:
    """Expose the redirected cache/config dirs to tests that need them."""
    return {
        "cache": Path(os.environ["OXE_CACHE_DIR"]),
        "config": Path(os.environ["OXE_CONFIG_DIR"]),
    }
