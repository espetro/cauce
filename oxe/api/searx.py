"""The canonical search route: ``GET|POST /search``, SearXNG-shaped both ways.

Per the plan's "search contract" section, this is the one canonical,
extensible search surface; ``oxe/api/exa.py`` and ``oxe/api/mcp.py`` are thin
compatibility layers built on top of the same ``SearchService``.

GET accepts flat query parameters rather than
``Annotated[SearchRequest, Query()]``: ``SearchRequest`` sets
``strict=True`` (data ladder rule 1), and pydantic strict mode refuses to
coerce the query string ``"2"`` into ``int`` 2, which is exactly the
coercion FastAPI needs to do for query params. Declaring each field as its
own ``Annotated[..., Query()]`` parameter keeps the fields individually
typed and documented in the OpenAPI schema (satisfying the plan's "query
parameters land in the schema" requirement) while still letting FastAPI's
own (non-strict) query-param parsing do the string-to-int/bool coercion;
the handler then assembles the canonical, strict ``SearchRequest`` from
already-validated python values. POST keeps ``SearchRequest`` as the request
body directly, since a JSON body's ints/lists don't need that coercion.
"""

from collections.abc import Sequence
from typing import Annotated, Literal

from fastapi import APIRouter, Depends, Query, Request

from oxe.search.model import SearchRequest, SearxResponse
from oxe.search.service import SearchService

router = APIRouter()


def get_search_service(request: Request) -> SearchService:
    """Reads the single ``SearchService`` built by ``oxe.app.create_app``.

    ``request.app.state.search_service`` is set once at app-factory time (one
    ``TTLCache`` + one engine per process), never per-request.
    """
    service = request.app.state.search_service
    if not isinstance(service, SearchService):  # pragma: no cover - app-factory invariant
        msg = "app.state.search_service is not configured"
        raise TypeError(msg)
    return service


SearchServiceDep = Annotated[SearchService, Depends(get_search_service)]


def build_search_request(
    q: Annotated[str, Query(min_length=1, description="Query text.")],
    pageno: Annotated[int, Query(ge=1, description="1-based result page.")] = 1,
    # Sequence[str] with a tuple default (not list[str]) so the FastAPI-mandated
    # `= default` avoids ruff B006 (mutable default argument); converted to a
    # list right below, for SearchRequest.
    categories: Annotated[Sequence[str], Query(description="SearXNG category names.")] = (
        "general",
    ),
    language: Annotated[str, Query(description="Language code, or 'all'.")] = "all",
    time_range: Annotated[
        Literal["day", "week", "month", "year"] | None,
        Query(description="Restrict to results published within this window."),
    ] = None,
    safesearch: Annotated[Literal[0, 1, 2], Query(description="0=off, 1=moderate, 2=strict.")] = 0,
) -> SearchRequest:
    return SearchRequest(
        q=q,
        pageno=pageno,
        categories=list(categories),
        language=language,
        time_range=time_range,
        safesearch=safesearch,
    )


GetSearchRequestDep = Annotated[SearchRequest, Depends(build_search_request)]


@router.get("/search", response_model=SearxResponse)
async def search_get(req: GetSearchRequestDep, service: SearchServiceDep) -> SearxResponse:
    return await service.search(req)


@router.post("/search", response_model=SearxResponse)
async def search_post(req: SearchRequest, service: SearchServiceDep) -> SearxResponse:
    return await service.search(req)
