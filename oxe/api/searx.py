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


_SAFESEARCH_OFF: Literal[0] = 0
_SAFESEARCH_MODERATE: Literal[1] = 1
_SAFESEARCH_STRICT: Literal[2] = 2


def _narrow_safesearch(value: int) -> Literal[0, 1, 2]:
    """FastAPI's ``Query(ge=0, le=2)`` already rejects anything outside
    ``{0, 1, 2}`` with a 422 before this runs, but that's a runtime bound
    basedpyright strict can't see from ``int`` alone -- explicit branches
    (not a ``cast``) narrow it to the ``Literal`` ``SearchRequest.safesearch``
    actually declares, keeping the narrowing itself type-checked rather than
    asserted away.
    """
    if value == _SAFESEARCH_OFF:
        return _SAFESEARCH_OFF
    if value == _SAFESEARCH_MODERATE:
        return _SAFESEARCH_MODERATE
    if value == _SAFESEARCH_STRICT:
        return _SAFESEARCH_STRICT
    # pragma: no cover - Query(ge=0, le=2) already validated this upstream
    msg = f"safesearch out of Query(ge=0, le=2) bounds: {value}"
    raise ValueError(msg)


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
    # Declared as plain `int` (not `Literal[0, 1, 2]`) so FastAPI's
    # non-strict query-param parsing coerces the querystring "0"/"1"/"2"
    # into an int the way it already does for `pageno`; `Literal`, unlike a
    # scalar type, does not get that str->int coercion (confirmed:
    # `GET /search?safesearch=0` 422'd with `Literal[0, 1, 2]` here, which
    # schemathesis's contract check flagged as a schema-compliant request
    # the API wrongly rejected). `ge`/`le` re-declare the same 0-2 bound in
    # the generated OpenAPI schema; `SearchRequest.safesearch` still
    # strictly re-validates the now-int value against `Literal[0, 1, 2]`.
    safesearch: Annotated[int, Query(ge=0, le=2, description="0=off, 1=moderate, 2=strict.")] = 0,
) -> SearchRequest:
    return SearchRequest(
        q=q,
        pageno=pageno,
        categories=list(categories),
        language=language,
        time_range=time_range,
        safesearch=_narrow_safesearch(safesearch),
    )


GetSearchRequestDep = Annotated[SearchRequest, Depends(build_search_request)]


@router.get("/search", response_model=SearxResponse)
async def search_get(req: GetSearchRequestDep, service: SearchServiceDep) -> SearxResponse:
    return await service.search(req)


@router.post("/search", response_model=SearxResponse)
async def search_post(req: SearchRequest, service: SearchServiceDep) -> SearxResponse:
    return await service.search(req)
