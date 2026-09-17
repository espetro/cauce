"""Tests for oxe.search.engines.registry.

Adapted from legacy tests/test_backends.py's registry coverage
(test_resolve_string, test_resolve_json_specs, test_resolve_unknown_defaults_to_ddg,
test_build_from_env, test_entry_point_discovery_builtins_present).
"""

import json
import os

import pytest

from oxe.search.engines.compose import FallbackEngine, FanoutEngine
from oxe.search.engines.ddgs import DdgsEngine
from oxe.search.engines.registry import build_from_env, discover, resolve
from oxe.search.errors import BackendError


def test_resolve_string() -> None:
    b = resolve("ddg")
    assert isinstance(b, DdgsEngine)


def test_resolve_json_specs() -> None:
    spec = json.loads('{"mode":"fallback","backends":["ddg"]}')
    b = resolve(spec)
    assert isinstance(b, FallbackEngine)
    assert b.name == "fb:ddg"

    nested = json.loads(
        '{"mode":"fanout","backends":[{"mode":"fallback","backends":["ddg"]},"ddg"]}'
    )
    fo = resolve(nested)
    assert isinstance(fo, FanoutEngine)
    assert fo.name == "fo:fb:ddg,ddg"

    assert isinstance(resolve(["ddg"]), FallbackEngine)
    assert isinstance(resolve("ddg"), DdgsEngine)


def test_resolve_unknown_defaults_to_ddg() -> None:
    assert isinstance(resolve("nope-not-real"), DdgsEngine)
    assert isinstance(resolve({"mode": "wat", "backends": []}), DdgsEngine)
    assert isinstance(resolve(12345), DdgsEngine)


def test_resolve_named_ddgs_engine() -> None:
    b = resolve("bing")
    assert isinstance(b, DdgsEngine)
    assert b.name == "bing"


def test_build_from_env() -> None:
    old = os.environ.pop("OXE_BACKENDS", None)
    try:
        assert isinstance(build_from_env(), DdgsEngine)  # unset

        os.environ["OXE_BACKENDS"] = "garbage{"
        assert isinstance(build_from_env(), DdgsEngine)  # unparseable

        os.environ["OXE_BACKENDS"] = '{"mode":"fallback","backends":["ddg"]}'
        b = build_from_env()
        assert isinstance(b, FallbackEngine)
        assert b.name == "fb:ddg"
    finally:
        if old is not None:
            os.environ["OXE_BACKENDS"] = old
        else:
            os.environ.pop("OXE_BACKENDS", None)


def test_entry_point_discovery_builtins_present() -> None:
    d = discover()
    for k in ("ddg", "bing", "brave"):
        assert k in d


def test_resolve_depth_limit_raises() -> None:
    spec: object = "ddg"
    for _ in range(20):
        spec = {"mode": "fallback", "backends": [spec]}
    with pytest.raises(BackendError):
        resolve(spec)
