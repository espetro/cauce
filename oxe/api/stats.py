"""GET /api/stats: dashboard aggregates for the wave-3 dashboard screen.

Thin typed boundary over ``oxe.stats.build_json``; the aggregation itself is
tested in ``tests/test_stats.py``. The sqlite reads are synchronous, so they
run behind ``asyncio.to_thread`` (ASYNC rule in ``oxe/AGENTS.md``).
"""

import asyncio
from typing import Annotated

from fastapi import APIRouter, Depends, Query, Request

from oxe.cache import TTLCache
from oxe.search.service import SearchService
from oxe.stats import StatsSummary, build_json

router = APIRouter(prefix="/api")


def get_cache(request: Request) -> TTLCache:
    """Reads the single ``TTLCache`` held by the app's ``SearchService``
    (set once at app-factory time by ``oxe.app.create_app``)."""
    service = request.app.state.search_service
    if not isinstance(service, SearchService):  # pragma: no cover - app-factory invariant
        msg = "app.state.search_service is not configured"
        raise TypeError(msg)
    return service.cache


CacheDep = Annotated[TTLCache, Depends(get_cache)]


@router.get("/stats", response_model=StatsSummary, summary="Dashboard aggregates.")
async def api_stats(
    cache: CacheDep,
    days: Annotated[int, Query(ge=1, le=90, description="Aggregation window in days.")] = 14,
) -> StatsSummary:
    """Aggregates over the search log (searches/day, hit rate, latency
    percentiles, top and zero-result queries, client split)."""
    return await asyncio.to_thread(_stats_sync, cache, days)


def _stats_sync(cache: TTLCache, days: int) -> StatsSummary:
    """Blocking tail of ``GET /api/stats``, executed via ``asyncio.to_thread``."""
    return build_json(cache.db_path, days=days)
