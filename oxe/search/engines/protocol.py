"""The engine contract every search backend implements.

Lifted from legacy ``oxe/backends.py``'s ``SearchBackend`` Protocol, retyped
against the canonical SearXNG-shaped ``SearchRequest``/``SearxResponse``
models in ``oxe/search/model.py`` instead of Exa-shaped dicts, and made
``async`` since FastAPI route handlers call ``search()`` directly (a later
task) rather than going through a thread pool at the call site.

Contract (unchanged from legacy in substance):

- ``search()`` is ``async``; a blocking implementation wraps its blocking
  call in ``asyncio.to_thread`` internally (see ``engines/ddgs.py``) rather
  than pushing that concern onto callers.
- An empty ``results`` list means "no hits", NOT an error. Return the empty
  payload normally.
- On provider failure, raise ``BackendError``. NEVER return an empty
  ``SearxResponse`` to signal an error; callers treat an empty result set as
  a real, successful "no hits" answer.
- Every ``SearchResult`` in the returned response MUST have its ``engine``
  field stamped to this engine's ``name``.
- Backends are stateless and perform no internal caching; caching is the
  caller's job (``oxe.cache.TTLCache``, wired in via ``oxe.search.service``).
- A caller needing a hard per-call timeout wraps ``search()`` itself (see
  ``oxe.search.engines.compose.run_one``); an engine's own ``.timeout``
  attribute is advisory, not self-enforced.
"""

from typing import Protocol, runtime_checkable

from oxe.search.model import SearchRequest, SearxResponse


@runtime_checkable
class SearchEngine(Protocol):
    name: str
    timeout: float

    async def search(self, req: SearchRequest) -> SearxResponse:
        """Run a canonical search request, returning a canonical response."""
        ...
