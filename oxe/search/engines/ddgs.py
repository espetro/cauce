"""DuckDuckGo-backed search engine, via the bundled ``ddgs`` library.

Lifted from legacy ``oxe/exa_compat.py`` (the actual ``DDGS().text()`` call,
the retry-once-after-a-delay behavior on failure, the backend/region/
timelimit mapping) and legacy ``oxe/backends.py``'s ``DdgsBackend`` /
``DDGBackend`` (the engine name table, per-call timeout). Rewritten to build
the canonical ``SearchResult`` / ``SearxResponse`` models directly -- no
intermediate Exa-shaped dict -- and to run ``async`` with the blocking
``DDGS().text()`` call wrapped in ``asyncio.to_thread`` (ruff's ``ASYNC``
group gates this).
"""

import asyncio
import logging
from dataclasses import dataclass
from typing import Final
from urllib.parse import urlparse

from ddgs import DDGS
from ddgs.exceptions import DDGSException

from oxe.search.errors import BackendError
from oxe.search.model import SearchRequest, SearchResult, SearxResponse

log = logging.getLogger(__name__)

# oxe engine name -> ddgs `backend` kwarg. Exported (no leading underscore)
# since oxe/search/engines/registry.py enumerates it to register one leaf
# engine per named ddgs backend.
DDGS_ENGINES: Final[dict[str, str]] = {
    "ddg": "duckduckgo",
    "auto": "auto",
    "google": "google",
    "bing": "bing",
    "brave": "brave",
    "mojeek": "mojeek",
    "yahoo": "yahoo",
    "yandex": "yandex",
    "wikipedia": "wikipedia",
}

_TIME_RANGE_TO_TIMELIMIT: Final[dict[str, str]] = {
    "day": "d",
    "week": "w",
    "month": "m",
    "year": "y",
}
_SAFESEARCH_TO_DDGS: Final[dict[int, str]] = {0: "off", 1: "moderate", 2: "on"}
_FAVICON = "https://www.google.com/s2/favicons?domain={netloc}&sz=32"
# Pause before the single retry (DDG html paging is aggressively rate limited).
_PAGE_RETRY_DELAY_S = 1.5
_DEFAULT_TIMEOUT_S = 10.0


def _favicon_for(url: str) -> str | None:
    try:
        netloc = urlparse(url).netloc
    except ValueError:
        return None
    return _FAVICON.format(netloc=netloc) if netloc else None


def _region_for(language: str) -> str | None:
    """Best-effort SearXNG language code -> ddgs region mapping.

    ddgs regions look like ``us-en`` (country-language); SearXNG language
    codes look like ``en-US`` (language-COUNTRY) or the sentinel ``all``.
    Anything that doesn't parse into that shape is left unset -- ddgs then
    searches without a region restriction, same as legacy's ``None``.
    """
    if not language or language == "all":
        return None
    normalized = language.lower().replace("_", "-")
    if "-" not in normalized:
        return None
    lang, _, country = normalized.partition("-")
    return f"{country}-{lang}"


def _timelimit_for(req: SearchRequest) -> str | None:
    if req.time_range is not None:
        return _TIME_RANGE_TO_TIMELIMIT[req.time_range]
    if "news" in req.categories:
        return "d"
    return None


def _row_to_result(row: dict[str, str], *, engine_name: str) -> SearchResult:
    url = row.get("href") or ""
    return SearchResult(
        url=url,
        title=row.get("title") or "",
        content=row.get("body") or "",
        engine=engine_name,
        engines=[engine_name],
        thumbnail=_favicon_for(url),
    )


@dataclass(frozen=True)
class DdgsCallParams:
    """Groups the ddgs.text() call shape to keep _call_ddgs under the max-args cap."""

    backend: str
    max_results: int
    region: str | None
    timelimit: str | None
    safesearch: str
    page: int


def _call_ddgs(query: str, params: DdgsCallParams) -> list[dict[str, str]]:
    """Blocking ``DDGS().text()`` call. Always run via ``asyncio.to_thread``.

    ``ddgs`` ships without a ``py.typed`` marker; ``typings/ddgs/`` carries a
    minimal local stub (mirroring ``typings/aiosql/``) so this call is fully
    typed rather than leaking ``Unknown``/``Any`` into this module.
    """
    kwargs: dict[str, object] = {
        "max_results": params.max_results,
        "backend": params.backend,
        "safesearch": params.safesearch,
    }
    if params.region:
        kwargs["region"] = params.region
    if params.timelimit:
        kwargs["timelimit"] = params.timelimit
    if params.page > 1:
        kwargs["page"] = params.page
    return list(DDGS().text(query, **kwargs))


class DdgsEngine:
    """Any engine the bundled ``ddgs`` library supports (bing, brave, google, ...).

    The engine name doubles as the ddgs backend selection and as the cache
    namespace (via ``oxe.search.service.cache_key``), so results from
    different engines never collide.
    """

    page_size = 10

    def __init__(self, engine: str = "ddg") -> None:
        if engine not in DDGS_ENGINES:
            msg = f"unknown ddgs engine {engine!r}"
            raise BackendError(msg)
        self._ddgs_backend = DDGS_ENGINES[engine]
        self.name = engine
        self.timeout = _DEFAULT_TIMEOUT_S

    async def search(self, req: SearchRequest) -> SearxResponse:
        params = DdgsCallParams(
            backend=self._ddgs_backend,
            max_results=self.page_size,
            region=_region_for(req.language),
            timelimit=_timelimit_for(req),
            safesearch=_SAFESEARCH_TO_DDGS.get(req.safesearch, "moderate"),
            page=req.pageno,
        )
        raw = await self._search_with_retry(req.q, params)
        results = [_row_to_result(row, engine_name=self.name) for row in raw]
        return SearxResponse(query=req.q, number_of_results=len(results), results=results)

    async def _search_with_retry(self, query: str, params: DdgsCallParams) -> list[dict[str, str]]:
        try:
            return await asyncio.to_thread(_call_ddgs, query, params)
        except DDGSException as e:
            log.warning(
                "ddgs: backend %s failed (%s), retrying once after %ss",
                self._ddgs_backend,
                e,
                _PAGE_RETRY_DELAY_S,
            )
            await asyncio.sleep(_PAGE_RETRY_DELAY_S)
            try:
                return await asyncio.to_thread(_call_ddgs, query, params)
            except DDGSException as retry_err:
                msg = f"{self.name}: ddgs backend {self._ddgs_backend} failed: {retry_err}"
                raise BackendError(msg) from retry_err
