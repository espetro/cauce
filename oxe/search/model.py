"""Canonical SearXNG-shaped wire types.

Per the v0.5.0 plan's "search contract" section, the SearXNG response is
the only canonical search surface in oxe; the Exa HTTP/MCP adapters (a
later task) are thin, frozen conversions on top of these types, not a
second source of truth.

All models set ``ConfigDict(extra="forbid", frozen=True, strict=True)`` per
the data ladder in ``oxe/AGENTS.md``: an unexpected or dropped field is a
422 naming the offending key, not a silent discard.

Not every field below is populated by every engine -- ``oxe/search/engines/
ddgs.py`` only ever returns bare web results, so ``answers``, ``corrections``,
``infoboxes`` and ``suggestions`` stay empty for that engine today. The
fields still belong on the canonical model because the contract promises
them (a future ``searxng.py`` HTTP engine will populate some of them), and a
genuinely absent field defaults to an empty list or ``None`` rather than
being omitted from the type.
"""

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class InfoboxUrl(BaseModel):
    """One entry in an ``Infobox.urls`` list."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    title: str
    url: str


class Infobox(BaseModel):
    """SearXNG infobox entry (Wikipedia-style side panel content)."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    infobox: str
    id: str | None = None
    content: str = ""
    img_src: str | None = None
    urls: list[InfoboxUrl] = Field(default_factory=list)


class UnresponsiveEngine(BaseModel):
    """One engine that failed to answer, per SearXNG's ``unresponsive_engines``.

    SearXNG itself represents this as a two-element list; modelled here as
    a named pair so both entry points and tests reference ``.engine`` /
    ``.error`` rather than positional indices.
    """

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    engine: str
    error: str


class SearchResult(BaseModel):
    """One canonical SearXNG-shaped search result."""

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True, populate_by_name=True)

    url: str
    title: str
    content: str = ""
    engine: str
    engines: list[str] = Field(default_factory=list)
    score: float = 0.0
    category: str = "general"
    # Wire name is camelCase (SearXNG's own contract); aliased rather than
    # left camelCase on the Python attribute so ruff's N-group naming rules
    # stay clean while `model_dump(by_alias=True)` still emits "publishedDate".
    published_date: str | None = Field(default=None, alias="publishedDate")
    thumbnail: str | None = None


class SearxResponse(BaseModel):
    """The canonical SearXNG-shaped search response.

    Two of these three rules from the plan's "search contract" section are
    enforced here as shape; the third (unresponsive_engines-with-no-results
    must raise, never be returned) is a service-level invariant, enforced in
    ``oxe/search/service.py`` rather than at the model layer, since a
    freshly-constructed response legitimately passes through this state
    while an engine is still composing partial results.
    """

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    query: str
    number_of_results: int = 0
    results: list[SearchResult] = Field(default_factory=list)
    answers: list[str] = Field(default_factory=list)
    corrections: list[str] = Field(default_factory=list)
    infoboxes: list[Infobox] = Field(default_factory=list)
    suggestions: list[str] = Field(default_factory=list)
    unresponsive_engines: list[UnresponsiveEngine] = Field(default_factory=list)


class SearchRequest(BaseModel):
    """The canonical SearXNG-shaped search request.

    Cache identity (``oxe.search.service.cache_key``) is computed on this
    model's fields, so the native ``/search`` route and the future Exa
    compat adapter -- which will build a ``SearchRequest`` from an
    Exa-shaped request before calling the service -- share one cache row
    for the same underlying query.
    """

    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    q: str
    pageno: int = 1
    categories: list[str] = Field(default_factory=lambda: ["general"])
    language: str = "all"
    time_range: Literal["day", "week", "month", "year"] | None = None
    safesearch: Literal[0, 1, 2] = 0
