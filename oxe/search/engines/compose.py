"""Composition of search engines: fallback and fanout, plus timeout wrapping.

Lifted from legacy ``oxe/backends.py``'s ``FallbackBackend``, ``FanoutBackend``
and ``run_one``, retyped against the async ``SearchEngine`` protocol and the
canonical ``SearxResponse``/``SearchResult`` models.

Both compositors record engine failures into the response's
``unresponsive_engines`` list instead of silently discarding them (legacy
dropped failed engines on the floor). Whether a non-empty
``unresponsive_engines`` alongside empty ``results`` is an error is *not*
decided here -- that is a service-level invariant enforced in
``oxe/search/service.py`` so it applies uniformly regardless of which engine
(or composition of engines) produced the response.
"""

import asyncio
import logging

from oxe.search.engines.protocol import SearchEngine
from oxe.search.errors import BackendError
from oxe.search.model import SearchRequest, SearchResult, SearxResponse, UnresponsiveEngine

log = logging.getLogger(__name__)

_DEFAULT_TIMEOUT_S = 10.0


async def run_one(
    engine: SearchEngine, req: SearchRequest, timeout: float | None = None
) -> SearxResponse:
    """Run ``engine.search(req)`` under a per-call timeout.

    A timeout raises ``BackendError`` (never returns an empty response to
    signal it -- same rule as provider failure).
    """
    t = timeout if timeout is not None else engine.timeout
    try:
        return await asyncio.wait_for(engine.search(req), timeout=t)
    except TimeoutError as e:
        msg = f"{engine.name} timed out after {t}s"
        raise BackendError(msg) from e


class FallbackEngine:
    """Try engines in order; the first with a non-empty result set wins."""

    def __init__(self, engines: list[SearchEngine]) -> None:
        self.engines = list(engines)
        self.name = "fb:" + ",".join(e.name for e in self.engines)
        self.timeout = sum(e.timeout for e in self.engines) or _DEFAULT_TIMEOUT_S

    async def search(self, req: SearchRequest) -> SearxResponse:
        if not self.engines:
            msg = f"{self.name}: no engines configured"
            raise BackendError(msg)
        unresponsive: list[UnresponsiveEngine] = []
        for engine in self.engines:
            try:
                response = await engine.search(req)
            except BackendError as e:
                unresponsive.append(UnresponsiveEngine(engine=engine.name, error=str(e)))
                log.warning("%s: engine %s failed: %s", self.name, engine.name, e)
                continue
            if response.results:
                return response.model_copy(update={"unresponsive_engines": unresponsive})
        if len(unresponsive) == len(self.engines):
            msg = f"{self.name}: all engines failed"
            raise BackendError(msg)
        return SearxResponse(query=req.q, unresponsive_engines=unresponsive)


class FanoutEngine:
    """Run all engines concurrently, dedup results by url in declared order."""

    def __init__(self, engines: list[SearchEngine]) -> None:
        self.engines = list(engines)
        self.name = "fo:" + ",".join(e.name for e in self.engines)
        self.timeout = max((e.timeout for e in self.engines), default=_DEFAULT_TIMEOUT_S)

    async def search(self, req: SearchRequest) -> SearxResponse:
        if not self.engines:
            return SearxResponse(query=req.q)
        outcomes = await asyncio.gather(*(self._safe_search(e, req) for e in self.engines))
        return self._merge(req, outcomes)

    async def _safe_search(
        self, engine: SearchEngine, req: SearchRequest
    ) -> SearxResponse | UnresponsiveEngine:
        try:
            return await engine.search(req)
        except BackendError as e:
            log.warning("%s: engine %s failed: %s", self.name, engine.name, e)
            return UnresponsiveEngine(engine=engine.name, error=str(e))

    def _merge(
        self, req: SearchRequest, outcomes: list[SearxResponse | UnresponsiveEngine]
    ) -> SearxResponse:
        seen: set[str] = set()
        results: list[SearchResult] = []
        unresponsive: list[UnresponsiveEngine] = []
        for outcome in outcomes:
            if isinstance(outcome, UnresponsiveEngine):
                unresponsive.append(outcome)
                continue
            for r in outcome.results:
                if r.url in seen:
                    continue
                seen.add(r.url)
                results.append(r)
        return SearxResponse(
            query=req.q,
            number_of_results=len(results),
            results=results,
            unresponsive_engines=unresponsive,
        )
