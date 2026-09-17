import logging
import os
from dataclasses import dataclass
from typing import Any, Callable

from ..cache import TTLCache
from .config import CACHE_DIR

log = logging.getLogger(__name__)


@dataclass
class AppState:
    cache: TTLCache
    backend: Any
    observer: Callable[[dict], None]


def _http_search_logger(c: TTLCache) -> Callable[[dict], None]:
    """Default observer for make_app: writes search_log rows, client='http'."""

    def _log(payload: dict) -> None:
        q = (payload.get("_q") or "").strip()
        if not q:
            # network rows miss _q; backfill from the same hash's cache/log text
            q = c.lookup_query_text(payload.get("_q_hash") or "") or ""
        c.log_search(
            query_text=q[:200],
            query_hash=payload.get("_q_hash") or "",
            source=payload.get("_source") or "network",
            backend=payload.get("_backend", "ddg"),
            result_count=len(payload.get("results") or []),
            duration_ms=payload.get("_duration_ms"),
            client="http",
        )

    return _log


# module-level cache kept for mcp_server wiring and backward compat
cache = TTLCache(os.path.join(CACHE_DIR, "cache.db"))
log.info("cache initialized at %s/cache.db", CACHE_DIR)
