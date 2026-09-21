"""Engine registry: builtins + entry-point discovery + ``OXE_BACKENDS`` env.

Lifted from legacy ``oxe/registry.py``, adapted to the async ``SearchEngine``
protocol and the canonical models. Unlike legacy, leaf engines (things
constructed with no arguments) and compositors (fallback/fanout, constructed
from a list of already-resolved sub-engines) are tracked in two separate
dicts rather than one polymorphic one, so each has a concrete, checkable
callable type instead of ``object``.
"""

import json
import logging
import os
import re
from collections.abc import Callable
from importlib.metadata import EntryPoint, entry_points
from typing import cast

from oxe.search.engines.compose import FallbackEngine, FanoutEngine
from oxe.search.engines.ddgs import DDGS_ENGINES, DdgsEngine
from oxe.search.engines.protocol import SearchEngine
from oxe.search.engines.wikipedia import WikipediaEngine
from oxe.search.errors import BackendError

log = logging.getLogger(__name__)

EngineFactory = Callable[[], SearchEngine]
Compositor = Callable[[list[SearchEngine]], SearchEngine]

_COMPOSITORS: dict[str, Compositor] = {
    "fallback": FallbackEngine,
    "fanout": FanoutEngine,
}
_MAX_SPEC_DEPTH = 8
_BARE_NAME_RE = re.compile(r"[A-Za-z][A-Za-z0-9_-]*")


class _NamedDdgsFactory:
    """Binds a specific ddgs engine name so it can sit in an ``EngineFactory`` dict."""

    def __init__(self, name: str) -> None:
        self._name = name

    def __call__(self) -> SearchEngine:
        return DdgsEngine(self._name)


def _builtin_engines() -> dict[str, EngineFactory]:
    engines: dict[str, EngineFactory] = {"ddg": DdgsEngine}
    for name in DDGS_ENGINES:
        if name == "ddg":
            continue
        engines[name] = _NamedDdgsFactory(name)
    engines["wikipedia-opensearch"] = WikipediaEngine
    return engines


def _load_entry_point(ep: EntryPoint, out: dict[str, EngineFactory]) -> None:
    if ep.name in out:
        log.info("registry: entry point %r shadowed by builtin, skipping", ep.name)
        return
    try:
        loaded = ep.load()
    except (ImportError, AttributeError) as e:
        log.warning("registry: failed to load entry point %r: %s", ep.name, e)
        return
    out[ep.name] = cast(EngineFactory, loaded)


def discover() -> dict[str, EngineFactory]:
    """Merge builtin leaf engines with ``entry_points(group='oxe.search_engines')``.

    Builtins win on collision; a shadowed entry point is logged and skipped.
    """
    out = _builtin_engines()
    for ep in entry_points(group="oxe.search_engines"):
        _load_entry_point(ep, out)
    return out


def resolve(spec: object, _depth: int = 0) -> SearchEngine:
    """Resolve a backend spec (str, list, or dict) into an engine instance.

    ``spec`` is whatever ``json.loads`` produced from ``OXE_BACKENDS``, so it
    is typed as ``object`` and narrowed with ``isinstance`` rather than
    trusted -- same pattern as ``oxe/config.py``'s ``_as_object_dict``.
    """
    if _depth > _MAX_SPEC_DEPTH:
        msg = "registry: backend spec nested too deeply"
        raise BackendError(msg)
    if isinstance(spec, str):
        return _resolve_name(spec)
    if isinstance(spec, list):
        spec_list = cast(list[object], spec)
        return FallbackEngine([resolve(s, _depth + 1) for s in spec_list])
    if isinstance(spec, dict):
        spec_dict = cast(dict[object, object], spec)
        return _resolve_mode(spec_dict, _depth)
    log.warning("registry: unsupported spec %r, falling back to ddg", spec)
    return DdgsEngine()


def _resolve_name(name: str) -> SearchEngine:
    factory = discover().get(name)
    if factory is None:
        log.warning("registry: unknown backend %r, falling back to ddg", name)
        return DdgsEngine()
    return factory()


def _resolve_mode(spec_dict: dict[object, object], depth: int) -> SearchEngine:
    mode = spec_dict.get("mode", "fallback")
    if not isinstance(mode, str) or mode not in _COMPOSITORS:
        log.warning("registry: unknown mode %r, falling back to ddg", mode)
        return DdgsEngine()
    raw_backends = spec_dict.get("backends")
    sub_specs = cast(list[object], raw_backends) if isinstance(raw_backends, list) else []
    engines = [resolve(s, depth + 1) for s in sub_specs]
    return _COMPOSITORS[mode](engines)


def build_from_env() -> SearchEngine:
    """Build the engine from ``OXE_BACKENDS``. Unset/empty means plain ``DdgsEngine``."""
    raw = os.getenv("OXE_BACKENDS", "").strip()
    if not raw:
        return DdgsEngine()
    if _BARE_NAME_RE.fullmatch(raw):
        return resolve(raw)
    try:
        spec = cast(object, json.loads(raw))
    except json.JSONDecodeError as e:
        log.warning("registry: OXE_BACKENDS is not valid JSON (%s), using ddg", e)
        return DdgsEngine()
    try:
        return resolve(spec)
    except BackendError as e:
        log.warning("registry: failed to resolve OXE_BACKENDS (%s), using ddg", e)
        return DdgsEngine()
