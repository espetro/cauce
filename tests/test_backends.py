"""Tests for oxe.backends and oxe.registry. Runnable as a plain script or pytest."""

import json

from oxe.backends import (
    BackendError,
    DDGBackend,
    FallbackBackend,
    FanoutBackend,
    run_one,
)
from oxe.registry import build_from_env, resolve


class Fake:
    def __init__(self, name, results=None, error=None):
        self.name = name
        self.timeout = 10.0
        self._results = results
        self._error = error

    def search(self, req):
        if self._error:
            raise self._error
        return {
            "requestId": "r-" + self.name,
            "searchType": req.get("type") or "auto",
            "results": list(self._results or []),
            "costDollars": {"total": 0.0},
        }


class Slow:
    name = "slow"
    timeout = 0.2

    def search(self, req):
        import time

        time.sleep(5)
        return {}


def R(url):
    return {"title": url, "url": url, "id": url}


def test_fallback_first_non_empty():
    fb = FallbackBackend([Fake("a"), Fake("b", results=[R("u1")])])
    out = fb.search({})
    assert out["results"][0]["url"] == "u1"
    assert out["results"][0]["_engine"] == "b"


def test_fallback_empty_fallthrough():
    fb = FallbackBackend([Fake("a"), Fake("b", results=[R("u1")])])
    fb.backends[1] = Fake("c", results=[R("u2")])
    out = fb.search({})
    assert out["results"][0]["url"] == "u2"


def test_fallback_exception_fallthrough():
    fb = FallbackBackend([Fake("a", error=RuntimeError("boom")), Fake("b", results=[R("u1")])])
    out = fb.search({})
    assert out["results"][0]["url"] == "u1"


def test_fallback_all_fail_raises():
    fb = FallbackBackend([Fake("a", error=RuntimeError("x")), Fake("b", error=RuntimeError("y"))])
    try:
        fb.search({})
        raise AssertionError("expected BackendError")
    except BackendError:
        pass


def test_fallback_all_empty_returns_empty():
    fb = FallbackBackend([Fake("a"), Fake("b")])
    out = fb.search({})
    assert out["results"] == []


def test_fanout_dedup_and_order():
    a = Fake("a", results=[R("u1"), R("u2")])
    b = Fake("b", results=[R("u2"), R("u3")])
    fo = FanoutBackend([a, b])
    out = fo.search({})
    urls = [r["url"] for r in out["results"]]
    assert urls == ["u1", "u2", "u3"], urls
    engines = [r["_engine"] for r in out["results"]]
    assert engines == ["a", "a", "b"]


def test_fanout_failed_provider_skipped():
    a = Fake("a", results=[R("u1")])
    b = Fake("b", error=RuntimeError("boom"))
    fo = FanoutBackend([a, b])
    out = fo.search({})
    assert [r["url"] for r in out["results"]] == ["u1"]


def test_run_one_timeout():
    try:
        run_one(Slow(), {}, timeout=0.1)
        raise AssertionError("expected BackendError")
    except BackendError as e:
        assert "timed out" in str(e)


def test_resolve_string():
    b = resolve("ddg")
    assert isinstance(b, DDGBackend)


def test_resolve_json_specs():
    spec = json.loads('{"mode":"fallback","backends":["ddg"]}')
    b = resolve(spec)
    assert isinstance(b, FallbackBackend)
    assert b.name == "fb:ddg"
    nested = json.loads('{"mode":"fanout","backends":[{"mode":"fallback","backends":["ddg"]},"ddg"]}')
    fo = resolve(nested)
    assert isinstance(fo, FanoutBackend)
    assert fo.name == "fo:fb:ddg,ddg"
    assert isinstance(resolve(["ddg"]), FallbackBackend)
    assert isinstance(resolve("ddg"), DDGBackend)


def test_resolve_unknown_defaults_to_ddg():
    assert isinstance(resolve("nope-not-real"), DDGBackend)
    assert isinstance(resolve({"mode": "wat", "backends": []}), DDGBackend)
    assert isinstance(resolve(12345), DDGBackend)


def test_build_from_env(monkeypatch=None):
    import os

    old = os.environ.pop("OXE_BACKENDS", None)
    try:
        assert isinstance(build_from_env(), DDGBackend)  # unset
        os.environ["OXE_BACKENDS"] = "garbage{"
        assert isinstance(build_from_env(), DDGBackend)  # unparseable
        os.environ["OXE_BACKENDS"] = '{"mode":"fallback","backends":["ddg"]}'
        b = build_from_env()
        assert isinstance(b, FallbackBackend) and b.name == "fb:ddg"
    finally:
        if old is not None:
            os.environ["OXE_BACKENDS"] = old
        else:
            os.environ.pop("OXE_BACKENDS", None)


def test_entry_point_discovery_builtins_present():
    from oxe.registry import discover

    d = discover()
    for k in ("ddg", "fallback", "fanout"):
        assert k in d


def run_all():
    ns = dict(globals())
    failures = 0
    for name in sorted(k for k in ns if k.startswith("test_")):
        try:
            ns[name]()
            print(f"ok: {name}")
        except Exception as e:
            failures += 1
            print(f"FAIL: {name}: {e}")
    return failures


if __name__ == "__main__":
    raise SystemExit(1 if run_all() else 0)
