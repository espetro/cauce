"""``GET /api/history``: click-log rows plus aggregate click stats.

Backs the `/history` screen (`.agents/docs/screens/history.md`,
`userflow-checkpoints.md` checkpoints 15-17). Read-only: rows are written
elsewhere (``TTLCache.record_click``, called from the search UI's click
handler and from MCP agents), this route only reads them back.

``since`` mirrors the UI's time-range filter (`24`/`168`/`720` hours;
absent means all time). ``q`` is the server-side substring filter the UI's
`qf` URL param maps onto (named ``q`` here, not ``qf``: the URL-level
`qf`-vs-`q` distinction is purely to avoid colliding with a differently
shaped `q` on other routes -- this route has no such collision, so its own
param is just `q`). Capped at ``MAX_ROWS`` per the screen spec's "Cap of
200 rows per view" note.
"""

from typing import Annotated

from fastapi import APIRouter, Depends, Query, Request
from pydantic import BaseModel, ConfigDict

from oxe.cache import TTLCache

router = APIRouter()

MAX_ROWS = 200


class HistoryRow(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    id: int
    query_hash: str
    query: str
    result_id: str
    url: str
    title: str
    clicked_at: int
    source: str


class HistoryStats(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    total: int
    last_24h: int
    oldest: int | None


class HistoryResponse(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    rows: list[HistoryRow]
    stats: HistoryStats


def get_cache(request: Request) -> TTLCache:
    """Reads the single ``TTLCache`` built by ``oxe.app.create_app``.

    Same one-instance-per-process pattern as
    ``oxe.api.searx.get_search_service``: ``request.app.state.cache`` is set
    once at app-factory time, and ``SearchService`` shares this exact
    instance rather than owning a second connection to the same db file.
    """
    cache = request.app.state.cache
    if not isinstance(cache, TTLCache):  # pragma: no cover - app-factory invariant
        msg = "app.state.cache is not configured"
        raise TypeError(msg)
    return cache


CacheDep = Annotated[TTLCache, Depends(get_cache)]


@router.get("/api/history", response_model=HistoryResponse)
async def get_history(
    cache: CacheDep,
    since: Annotated[
        int | None,
        Query(
            ge=1,
            description=(
                "Hours to look back (the UI only ever sends 24/168/720, "
                "but this route does not itself restrict to that set); "
                "absent means all time."
            ),
        ),
    ] = None,
    q: Annotated[
        str | None, Query(description="Substring filter against the click's query text.")
    ] = None,
) -> HistoryResponse:
    rows = cache.get_clicks(query_text=q, limit=MAX_ROWS, since_hours=since)
    stats = cache.click_stats()
    return HistoryResponse(
        rows=[
            HistoryRow(
                id=row.id,
                query_hash=row.query_hash,
                query=row.query,
                result_id=row.result_id,
                url=row.url,
                title=row.title,
                clicked_at=row.clicked_at,
                source=row.source,
            )
            for row in rows
        ],
        stats=HistoryStats(total=stats.total, last_24h=stats.last_24h, oldest=stats.oldest),
    )
