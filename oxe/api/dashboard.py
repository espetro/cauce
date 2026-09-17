"""``GET /api/stats``: dashboard aggregates for the `/dashboard` screen.

Backs `.agents/docs/screens/dashboard.md` (checkpoint 20). Two producers of
stats data already exist and this route combines both rather than picking
one:

- ``TTLCache.stats()`` (``oxe/cache.py``): live cache-table numbers (row
  count, unexpired count, db size, total hits, newest row) -- always
  populated, since every search response passes through the cache.
- ``oxe.stats.build_json()`` (``oxe/stats.py``): search-log-derived
  aggregates (searches per day, latency percentiles, top/zero-result
  queries, client split) -- empty/placeholder today because nothing on the
  canonical search path (``oxe/api/searx.py``) calls
  ``TTLCache.log_search`` yet. Per the spec, "Panels for data the backend
  does not yet aggregate ... render a flat muted 'no search log data yet'
  line"; the frontend decides that from ``log.hit_rate.total == 0`` rather
  than this route omitting the field, so "the corresponding fields" widening
  later (more search_log producers) needs no route change, only richer data
  flowing into the same schema -- matching the spec's "no route changes
  expected, only a widened response schema" note.

``hit_rate_pct`` is deliberately not ``log.hit_rate.rate``: the mockup's
"cache hit rate 78% (N total hits)" box is described as backed by
"unexpired/rows" (the cache table), not the search-log-derived hit/miss
split, which is the only one of the two actually populated today.
"""

from typing import Annotated

from fastapi import APIRouter, Depends, Request
from pydantic import BaseModel, ConfigDict

from oxe.cache import TTLCache
from oxe.stats import StatsSummary, build_json

router = APIRouter()

LOG_WINDOW_DAYS = 14


class CacheStatsOut(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    rows: int
    unexpired_rows: int
    db_size_bytes: int
    total_hits: int
    oldest_unexpired: int | None
    newest: int | None


class DashboardResponse(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    cache: CacheStatsOut
    hit_rate_pct: float | None
    log: StatsSummary


def get_cache(request: Request) -> TTLCache:
    """See ``oxe.api.history.get_cache`` -- same shared-instance pattern."""
    cache = request.app.state.cache
    if not isinstance(cache, TTLCache):  # pragma: no cover - app-factory invariant
        msg = "app.state.cache is not configured"
        raise TypeError(msg)
    return cache


CacheDep = Annotated[TTLCache, Depends(get_cache)]


def _hit_rate_pct(rows: int, unexpired_rows: int) -> float | None:
    if not rows:
        return None
    return round(100 * unexpired_rows / rows, 1)


@router.get("/api/stats", response_model=DashboardResponse)
async def get_dashboard(cache: CacheDep) -> DashboardResponse:
    stats = cache.stats()
    log: StatsSummary = build_json(cache.db_path, days=LOG_WINDOW_DAYS)
    return DashboardResponse(
        cache=CacheStatsOut(
            rows=stats.rows,
            unexpired_rows=stats.unexpired_rows,
            db_size_bytes=stats.db_size_bytes,
            total_hits=stats.total_hits,
            oldest_unexpired=stats.oldest_unexpired,
            newest=stats.newest,
        ),
        hit_rate_pct=_hit_rate_pct(stats.rows, stats.unexpired_rows),
        log=log,
    )
