"""GET /api/history: merged activity feed for the wave-3 history screen.

Semantics lifted from legacy ``oxe/server/cache_admin.py`` ``api_history``:
clicks (from the click log) merged newest-first with a stats line. The wave-3
screen (``.agents/docs/screens/history.md``) is click-only: rows, 24h/total/
oldest stats line, ``since`` hours filter (24|168|720 or absent = all) and a
200-row cap. Legacy's ``kind=cache`` rows and the ``/row/<query_hash>`` detail
route are NOT re-exposed here: the current screen spec does not consume them
(query cells link to re-running the search, via ``copy json``).

Models re-declared fresh under the data ladder (``extra="forbid"``,
``frozen``, ``strict``); query params are individually ``Annotated[..., Query()]``
parameters for the same strict-mode reason as ``oxe/api/searx.py`` (pydantic
strict refuses to coerce ``"24"`` to ``24``, FastAPI's query parsing must do it).
"""

import asyncio
from typing import Annotated, Literal

from fastapi import APIRouter, Query
from pydantic import BaseModel, ConfigDict

from oxe.api.stats import CacheDep

router = APIRouter(prefix="/api")

SinceParam = Literal[24, 168, 720]
_LIMIT_CAP = 200

# Declared as ``str`` with a ``pattern`` (not ``Literal[24, 168, 720]``) so
# invalid values come back as FastAPI 422s from the schema itself: a
# ``Literal`` of ints gets no str->int coercion (see ``oxe/api/searx.py``
# ``safesearch``), and accepting a bare ``int`` with ge/le would let range
# gap values (e.g. 719) through to a 500 instead of a schema-level 422.
_SINCE_PATTERN = "^(24|168|720)$"


def _narrow_since(value: str) -> SinceParam:
    """Explicit branches (not a ``cast``) narrow the pattern-validated
    string to ``SinceParam``; the branch conditions mirror the pattern."""
    if value == "24":
        return 24
    if value == "168":
        return 168
    if value == "720":
        return 720
    msg = f"since must be 24|168|720, got {value}"  # pragma: no cover - pattern already rejected
    raise ValueError(msg)


class ClickItem(BaseModel):
    """One click row, newest-first in ``HistoryResponse.items``."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    clicked_at: int
    query_hash: str
    query: str
    result_id: str
    url: str
    title: str
    source: str


class HistoryStats(BaseModel):
    """The stats line above the history table (history.md)."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    last_24h: int
    total: int
    oldest: int | None


class HistoryResponse(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    items: list[ClickItem]
    stats: HistoryStats
    limit: int
    since: SinceParam | None


def _history_sync(
    cache: CacheDep, since: SinceParam | None, q: str | None, limit: int
) -> HistoryResponse:
    """Blocking tail of ``GET /api/history``, executed via ``asyncio.to_thread``."""
    rows = cache.get_clicks(query_text=q, limit=limit, since_hours=since)
    stats = cache.click_stats()
    return HistoryResponse(
        items=[
            ClickItem(
                clicked_at=r.clicked_at,
                query_hash=r.query_hash,
                query=r.query,
                result_id=r.result_id,
                url=r.url,
                title=r.title,
                source=r.source,
            )
            for r in rows
        ],
        stats=HistoryStats(last_24h=stats.last_24h, total=stats.total, oldest=stats.oldest),
        limit=limit,
        since=since,
    )


@router.get("/history", response_model=HistoryResponse, summary="Click history feed.")
async def api_history(
    cache: CacheDep,
    # str + pattern (see _SINCE_PATTERN); narrowed by _narrow_since below.
    since: Annotated[
        str | None,
        Query(
            pattern=_SINCE_PATTERN,
            description="Hours back: 24=day, 168=week, 720=month; absent = all time.",
        ),
    ] = None,
    q: Annotated[str | None, Query(description="Query-text substring filter.")] = None,
    limit: Annotated[int, Query(ge=1, le=_LIMIT_CAP)] = 50,
) -> HistoryResponse:
    """Click rows newest-first plus the header stats line.

    ``Query``'s ``ge``/``le`` bounds are a range, not a set, so in-range gap
    values (e.g. 48) reach the handler; ``_narrow_since`` turns them into a
    ``ValueError``, which FastAPI reports as 500 today. Only the three
    literal values (24/168/720) are schema-valid for clients, per the
    generated OpenAPI ``enum`` -- gap values are caller bugs, not user input.
    """
    since_param: SinceParam | None = _narrow_since(since) if since is not None else None
    return await asyncio.to_thread(_history_sync, cache, since_param, q, limit)
