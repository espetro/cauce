"""query -> engine -> cache -> SearxResponse.

Wires ``oxe.cache.TTLCache`` in front of a ``SearchEngine`` (a leaf engine
like ``DdgsEngine``, or a composed ``FallbackEngine``/``FanoutEngine``).

Cache identity is computed on the canonical ``SearchRequest``
(``cache_key``), not on whatever wire shape a caller started from. This
matters once the Exa compat adapter lands (a later task): it will build a
``SearchRequest`` from the Exa-shaped request before calling this service,
so both entry points share one cache row for the same underlying query.
"""

import hashlib
import logging
from typing import cast

from oxe.cache import TTLCache
from oxe.jsontypes import JSONDict
from oxe.search.engines.protocol import SearchEngine
from oxe.search.errors import BackendError
from oxe.search.model import SearchRequest, SearxResponse

log = logging.getLogger(__name__)

DEFAULT_TTL_S = 3600


def cache_key(req: SearchRequest) -> str:
    """Cache identity for a canonical search request.

    Computed only on the fields that affect the result set: the query text
    (normalized), page, categories (order-independent), language, time
    range, and safesearch level.
    """
    norm = (
        req.q.strip().lower(),
        req.pageno,
        tuple(sorted(req.categories)),
        req.language,
        req.time_range or "",
        req.safesearch,
    )
    return hashlib.sha256(repr(norm).encode("utf-8")).hexdigest()


def _enforce_unresponsive_rule(response: SearxResponse) -> SearxResponse:
    """A non-empty ``unresponsive_engines`` with empty ``results`` is a
    provider failure, not a legitimate empty result set, and is never
    returned as one -- it raises ``BackendError`` instead. Per the plan's
    "search contract" section, this is the single most common SearXNG
    client bug, and the one a round-trip test pins.
    """
    if response.unresponsive_engines and not response.results:
        engines = ", ".join(e.engine for e in response.unresponsive_engines)
        msg = f"search backend(s) unresponsive with no results: {engines}"
        raise BackendError(msg)
    return response


class SearchService:
    """query -> engine -> cache -> SearxResponse."""

    def __init__(self, engine: SearchEngine, cache: TTLCache, ttl: int = DEFAULT_TTL_S) -> None:
        self._engine = engine
        self._cache = cache
        self._ttl = ttl

    async def search(self, req: SearchRequest) -> SearxResponse:
        key = cache_key(req)
        cached = self._cache.get(key)
        if cached is not None:
            return SearxResponse.model_validate(cached)
        response = _enforce_unresponsive_rule(await self._engine.search(req))
        self._cache.set(key, cast(JSONDict, response.model_dump(mode="json")), self._ttl)
        return response
