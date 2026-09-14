import logging
import os
from typing import Optional

from . import exa_compat
from .cache import TTLCache

log = logging.getLogger(__name__)

TTL_DEFAULT = int(os.getenv("EX_SEARCH_TTL_DEFAULT", "3600"))
TTL_MAX = int(os.getenv("EX_SEARCH_TTL_MAX", "86400"))
NEGATIVE_TTL = int(os.getenv("EX_SEARCH_NEGATIVE_TTL", "300"))


def do_search(cache: TTLCache, req_dict: dict, ttl: Optional[int] = None) -> dict:
    key = exa_compat.cache_key(req_dict)
    cached = cache.get(key)
    if cached is not None:
        out = dict(cached)
        out["_source"] = "cache"
        out["_q_hash"] = key
        return out

    response = exa_compat.search(req_dict)
    effective_ttl = ttl if ttl is not None else (NEGATIVE_TTL if not response["results"] else TTL_DEFAULT)
    effective_ttl = min(effective_ttl, TTL_MAX)
    to_store = dict(response)
    to_store["_q"] = (req_dict.get("query") or "")[:200]
    cache.set(key, to_store, effective_ttl)
    out = dict(response)
    out["_source"] = "network"
    out["_q_hash"] = key
    return out
