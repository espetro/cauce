import logging
import os
import time
from typing import Callable, Optional

from . import exa_compat
from .cache import TTLCache

log = logging.getLogger(__name__)

TTL_DEFAULT = int(os.getenv("OXE_TTL_DEFAULT", "3600"))
TTL_MAX = int(os.getenv("OXE_TTL_MAX", "86400"))
NEGATIVE_TTL = int(os.getenv("OXE_NEGATIVE_TTL", "300"))


def do_search(
    cache: TTLCache,
    req_dict: dict,
    ttl: Optional[int] = None,
    on_result: Optional[Callable[[dict], None]] = None,
    with_duration: bool = False,
) -> dict | tuple[dict, int | None]:
    key = exa_compat.cache_key(req_dict)
    cached = cache.get(key)
    if cached is not None:
        out = dict(cached)
        out["_source"] = "cache"
        out["_q_hash"] = key
        if on_result is not None:
            _notify(on_result, out, req_dict, key, "cache", None)
        return (out, None) if with_duration else out

    started = time.monotonic()
    response = exa_compat.search(req_dict)
    duration_ms = int((time.monotonic() - started) * 1000)
    effective_ttl = ttl if ttl is not None else (NEGATIVE_TTL if not response["results"] else TTL_DEFAULT)
    effective_ttl = min(effective_ttl, TTL_MAX)
    to_store = dict(response)
    to_store["query_text"] = req_dict.get("query") or ""
    to_store["query_hash"] = key
    cache.set(key, to_store, effective_ttl)
    out = dict(response)
    out["_source"] = "network"
    out["_q_hash"] = key
    if on_result is not None:
        _notify(on_result, out, req_dict, key, "network", duration_ms)
    return (out, duration_ms) if with_duration else out


def _notify(
    on_result: Callable[[dict], None],
    payload: dict,
    req_dict: dict,
    key: str,
    source: str,
    duration_ms: int | None,
) -> None:
    """Call the observer with the final payload plus _duration_ms; never break the search."""
    try:
        payload["_duration_ms"] = duration_ms
        on_result(payload)
    except Exception as e:
        log.exception("on_result observer failed for %r: %s", req_dict.get("query"), e)
