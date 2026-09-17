"""Frozen Exa compatibility adapter: two pure functions, no routes of its own.

Per the plan's "search contract" section: SearXNG is the only canonical
search surface (``oxe.search.model``); Exa is compatibility only, kept
because existing agent wiring (Claude Code, Hermes, maki -- see the repo
owner's global ``SEARCH.md``) already points at Exa-shaped endpoints.
``oxe/api/mcp.py`` is the sole caller of these two functions today; a future
task may also mount them behind an HTTP route, which is why they stay pure
and side-effect free here rather than reaching into ``SearchService``
themselves.

The wire models below mirror Exa's published schema (``ExaSearchRequest``,
``ExaSearchResponse`` and friends), referenced against legacy
``oxe/server/schemas.py`` / ``oxe/exa_compat.py``. Unlike legacy, every field
is concretely typed (no ``Any``, no bare ``dict``) and the request side sets
``extra="forbid"`` -- Exa is a schema we do not control, but a field oxe
does not understand should 422 rather than silently vanish. camelCase wire
names are declared as snake_case attributes with a pydantic ``alias``
(matching the pattern in ``oxe/search/model.py``'s ``SearchResult``), so
ruff's N-group naming rules stay clean while ``model_dump(by_alias=True)``
still emits the camelCase Exa contract.

Three adapter rules from the plan, each pinned by the round-trip property
test in ``tests/api/test_exa_adapter.py``:

1. ``numResults`` maps onto ``pageno`` plus a slice. ``exa_request_to_query``
   passes the Exa ``page`` straight through as SearXNG ``pageno`` (both are
   already 1-based, so no page-size arithmetic is needed there); the
   *slicing* half of the rule lives in ``searx_response_to_exa``, which
   trims the engine's results down to ``numResults``. This function is pure
   and has no access to a specific engine's page size (``DdgsEngine.page_size
   == 10`` today, and a future ``searxng.py`` HTTP engine may differ), so it
   only ever slices *down*: if the engine returned fewer than ``numResults``
   results for that page, the caller gets fewer than it asked for rather
   than this module fetching additional engine pages to make up the count.
2. A non-empty ``unresponsive_engines`` with empty ``results`` raising
   ``BackendError`` is already enforced one layer down, in
   ``oxe.search.service.SearchService.search`` -- by the time a
   ``SearxResponse`` reaches ``searx_response_to_exa``, that invariant has
   already held, so there is nothing left for this module to check.
3. Cache identity is shared "for free": ``exa_request_to_query`` builds a
   canonical ``SearchRequest``, and ``SearchService``/``cache_key`` key on
   that model's fields regardless of which caller (native ``/search`` or an
   Exa-shaped caller) produced it.
"""

import re
import uuid
from typing import Final

from pydantic import BaseModel, ConfigDict, Field

from oxe.search.model import SearchRequest, SearchResult, SearxResponse

_SENTENCE_SPLIT: Final = re.compile(r"(?<=[.!?])\s+")
_MAX_HIGHLIGHTS: Final = 3
_NUM_RESULTS_BOUNDS: Final = (1, 30)
_HIGHLIGHT_SCORE: Final = 0.5


class ExaContents(BaseModel):
    """Which optional fields the caller wants populated on each result."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    text: bool = False
    highlights: bool = False


class ExaSearchRequest(BaseModel):
    """Exa's ``POST /search`` request shape, the fields oxe supports.

    Exa's actual API accepts more fields (``startPublishedDate``, ``stream``,
    ``outputSchema``, ...); those are not modelled here at all rather than
    accepted-and-ignored, so a caller sending them gets a 422 naming the
    field instead of oxe silently dropping it -- same rationale as
    ``extra="forbid"`` generally, applied one level up.
    """

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True, populate_by_name=True)

    query: str
    type: str = "auto"
    num_results: int = Field(
        default=10, ge=_NUM_RESULTS_BOUNDS[0], le=_NUM_RESULTS_BOUNDS[1], alias="numResults"
    )
    page: int = Field(default=1, ge=1)
    category: str = ""
    include_domains: list[str] = Field(default_factory=list, alias="includeDomains")
    exclude_domains: list[str] = Field(default_factory=list, alias="excludeDomains")
    contents: ExaContents = Field(default_factory=ExaContents)


class ExaExtras(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    links: list[str] = Field(default_factory=list)


class ExaResultItem(BaseModel):
    """One row of Exa's published ``results[]`` shape."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True, populate_by_name=True)

    title: str = ""
    url: str = ""
    id: str = ""
    text: str = ""
    highlights: list[str] = Field(default_factory=list)
    highlight_scores: list[float] = Field(default_factory=list, alias="highlightScores")
    published_date: str | None = Field(default=None, alias="publishedDate")
    author: str | None = None
    image: str | None = None
    favicon: str | None = None
    extras: ExaExtras = Field(default_factory=ExaExtras)


class ExaCostDollars(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    total: float = 0.0


class ExaSearchResponse(BaseModel):
    """Exa's published ``POST /search`` response shape."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True, populate_by_name=True)

    request_id: str = Field(alias="requestId")
    search_type: str = Field(alias="searchType")
    results: list[ExaResultItem] = Field(default_factory=list)
    cost_dollars: ExaCostDollars = Field(default_factory=ExaCostDollars, alias="costDollars")


def _build_query_text(req: ExaSearchRequest) -> str:
    """``query`` plus ``site:``/``-site:`` operators for domain filters.

    Lifted from legacy ``oxe/exa_compat.py``'s ``build_query`` -- SearXNG has
    no native include/exclude-domains request field, so oxe folds them into
    the query text the same way legacy did.
    """
    parts = [req.query.strip()]
    parts += [f"site:{d}" for d in req.include_domains if d]
    parts += [f"-site:{d}" for d in req.exclude_domains if d]
    return " ".join(p for p in parts if p)


def exa_request_to_query(req: ExaSearchRequest) -> SearchRequest:
    """Exa-shaped request -> canonical ``SearchRequest``.

    ``category == "news"`` maps onto SearXNG's ``news`` category (mirroring
    legacy's day-only time limit for news); anything else stays "general".
    """
    categories = ["news"] if req.category == "news" else ["general"]
    return SearchRequest(q=_build_query_text(req), pageno=req.page, categories=categories)


def _extract_highlights(content: str) -> list[str]:
    if not content:
        return []
    sentences = [s.strip() for s in _SENTENCE_SPLIT.split(content) if s.strip()]
    return sentences[:_MAX_HIGHLIGHTS]


def _result_to_exa(result: SearchResult, contents: ExaContents) -> ExaResultItem:
    highlights = _extract_highlights(result.content) if contents.highlights else []
    return ExaResultItem(
        title=result.title,
        url=result.url,
        id=result.url,
        text=result.content if contents.text else "",
        highlights=highlights,
        highlightScores=[_HIGHLIGHT_SCORE] * len(highlights),
        publishedDate=result.published_date,
        favicon=result.thumbnail,
    )


def searx_response_to_exa(resp: SearxResponse, req: ExaSearchRequest) -> ExaSearchResponse:
    """Canonical ``SearxResponse`` -> Exa-shaped response.

    Takes ``req`` alongside ``resp`` (rather than ``resp`` alone) because
    Exa's ``numResults``/``type`` fields, needed for the rule-1 slice and the
    ``searchType`` echo, live on the request, not the canonical response.
    """
    sliced = resp.results[: req.num_results]
    results = [_result_to_exa(r, req.contents) for r in sliced]
    return ExaSearchResponse(
        requestId=str(uuid.uuid4()),
        searchType=req.type,
        results=results,
    )
