"""Backend registry: builtins + entry-point discovery + OXE_BACKENDS env resolution."""

import json
import logging
import os
from importlib.metadata import entry_points

from .backends import (
    BackendError,
    DDGBackend,
    DdgsBackend,
    FallbackBackend,
    FanoutBackend,
)

log = logging.getLogger(__name__)

# Compositors are plain classes; engines are constructed with their name.
_BUILTIN = {
    "ddg": DDGBackend,
    "fallback": FallbackBackend,
    "fanout": FanoutBackend,
}
_ENGINE_NAMES = [e for e in DdgsBackend._DDGS_ENGINE if e != "ddg"]


def discover() -> dict:
    """Merge builtins + engine names + entry_points(group='oxe.backends').

    Builtins win on collision; the shadowed entry point is logged and skipped.
    Engine names resolve lazily via a small factory wrapper.
    """

    class _Engine:
        def __init__(self, name):
            self._name = name

        def __call__(self):
            return DdgsBackend(self._name)

    out = dict(_BUILTIN)
    for name in _ENGINE_NAMES:
        out[name] = _Engine(name)
    try:
        eps = entry_points(group="oxe.backends")
    except Exception as e:
        log.warning("registry: entry point discovery failed: %s", e)
        return out
    for ep in eps:
        if ep.name in out:
            log.info("registry: entry point %r shadowed by builtin, skipping", ep.name)
            continue
        try:
            out[ep.name] = ep.load()
        except Exception as e:
            log.warning("registry: failed to load entry point %r: %s", ep.name, e)
    return out


def resolve(spec, _depth: int = 0) -> object:
    """Resolve a backend spec (str, dict, or list) into a backend instance."""
    if _depth > 8:
        raise BackendError("registry: backend spec nested too deeply")
    if isinstance(spec, str):
        cls = discover().get(spec)
        if cls is None:
            log.warning("registry: unknown backend %r, falling back to ddg", spec)
            return DDGBackend()
        return cls()
    if isinstance(spec, list):
        return FallbackBackend([resolve(s, _depth + 1) for s in spec])
    if isinstance(spec, dict):
        mode = spec.get("mode", "fallback")
        if mode not in ("fallback", "fanout"):
            log.warning("registry: unknown mode %r, falling back to ddg", mode)
            return DDGBackend()
        cls = discover().get(mode)
        backends = [resolve(s, _depth + 1) for s in spec.get("backends") or []]
        return cls(backends)
    log.warning("registry: unsupported spec %r, falling back to ddg", spec)
    return DDGBackend()


def build_from_env() -> object:
    """Build the backend from OXE_BACKENDS. Unset/empty means plain DDGBackend."""
    raw = os.getenv("OXE_BACKENDS", "").strip()
    if not raw:
        return DDGBackend()
    try:
        spec = json.loads(raw)
    except json.JSONDecodeError as e:
        log.warning("registry: OXE_BACKENDS is not valid JSON (%s), using ddg", e)
        return DDGBackend()
    try:
        return resolve(spec)
    except BackendError as e:
        log.warning("registry: failed to resolve OXE_BACKENDS (%s), using ddg", e)
        return DDGBackend()
