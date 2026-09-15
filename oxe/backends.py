"""Pluggable search backends and cache protocol for oxe.

SearchBackend contract
----------------------
A backend is anything with:

    name: str          # unique identifier, stamped on results as _engine
    timeout: float     # per-call wall-clock budget in seconds
    def search(self, req: dict) -> dict

Rules every backend must follow:

- search() is blocking and synchronous; the caller wraps it in a thread
  when it needs timeout enforcement (see run_one).
- An empty results list means "no hits", NOT an error. Return the empty
  payload normally.
- On provider failure, raise BackendError. NEVER return [] as a way to
  signal an error; callers treat [] as a real (empty) result.
- Every result dict in the returned payload MUST have its "_engine"
  field stamped to self.name.
- Backends are stateless and do no internal caching; caching is the
  caller's job (oxe.cache.TTLCache).

Backends are swapped in via make_app(cache=..., backend=...) or
do_search(..., backend=...).
"""

import logging
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FutTimeoutError
from typing import Protocol, runtime_checkable

log = logging.getLogger(__name__)


class BackendError(Exception):
    """Raised when a search backend fails at the provider level."""


@runtime_checkable
class SearchBackend(Protocol):
    name: str
    timeout: float

    def search(self, req: dict) -> dict:
        """Run a search request (Exa-shaped dict in, Exa-shaped dict out)."""
        ...


@runtime_checkable
class CacheAdapter(Protocol):
    def get(self, key: str) -> dict | None:
        """Return the cached payload for key, or None on miss/expiry."""
        ...

    def set(self, key: str, value: dict, ttl: int) -> None:
        """Store payload under key for ttl seconds."""
        ...


class DdgsBackend:
    """Any engine the bundled ddgs library supports (bing, brave, google, ...).

    The backend name doubles as the ddgs backend selection and as the cache
    namespace, so results from different engines never collide.
    """

    # registry name -> ddgs backend kwarg
    _DDGS_ENGINE = {
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

    def __init__(self, engine: str = "ddg"):
        if engine not in self._DDGS_ENGINE:
            raise BackendError(f"unknown ddgs engine {engine!r}")
        self.engine = self._DDGS_ENGINE[engine]
        self.name = engine
        self.timeout = 10.0

    def search(self, req: dict) -> dict:
        from . import exa_compat

        payload = exa_compat.search(req, engine=self.engine)
        for r in payload.get("results") or []:
            r["_engine"] = self.name
        return payload


class DDGBackend(DdgsBackend):
    """DuckDuckGo-backed search via exa_compat (ddgs-level). Default backend."""

    def __init__(self):
        super().__init__("ddg")


def run_one(b, req: dict, timeout: float | None = None) -> dict:
    """Run b.search(req) under a per-call timeout. Timeout raises BackendError."""
    t = timeout if timeout is not None else getattr(b, "timeout", 10.0)
    with ThreadPoolExecutor(max_workers=1) as ex:
        fut = ex.submit(b.search, req)
        try:
            return fut.result(timeout=t)
        except FutTimeoutError as e:
            raise BackendError(f"{getattr(b, 'name', b)} timed out after {t}s") from e


def _empty(req: dict) -> dict:
    """Build an empty Exa-shaped payload for req."""
    import uuid

    return {
        "requestId": str(uuid.uuid4()),
        "searchType": req.get("type") or "auto",
        "results": [],
        "costDollars": {"total": 0.0},
    }


class FallbackBackend:
    """Try backends in order; first non-empty result wins."""

    def __init__(self, backends: list):
        self.backends = list(backends)
        self.name = "fb:" + ",".join(getattr(b, "name", str(b)) for b in self.backends)
        self.timeout = sum(getattr(b, "timeout", 10.0) for b in self.backends) or 10.0

    def search(self, req: dict) -> dict:
        if not self.backends:
            raise BackendError(f"{self.name}: no backends configured")
        raised = False
        returned = False
        for b in self.backends:
            try:
                payload = b.search(req)
            except Exception as e:
                raised = True
                log.warning("%s: backend %s failed: %s", self.name, getattr(b, "name", b), e)
                continue
            returned = True
            results = payload.get("results") or []
            if results:
                for r in results:
                    r.setdefault("_engine", getattr(b, "name", "unknown"))
                return payload
        if raised and not returned:
            raise BackendError(f"{self.name}: all backends failed")
        return _empty(req)


class FanoutBackend:
    """Run all backends concurrently, dedup results by url in declared order."""

    def __init__(self, backends: list):
        self.backends = list(backends)
        self.name = "fo:" + ",".join(getattr(b, "name", str(b)) for b in self.backends)
        self.timeout = max(getattr(b, "timeout", 10.0) for b in self.backends) if self.backends else 10.0

    def search(self, req: dict) -> dict:
        if not self.backends:
            return _empty(req)
        with ThreadPoolExecutor(max_workers=len(self.backends)) as ex:
            futures = [ex.submit(run_one, b, req) for b in self.backends]
            payloads: list[dict | None] = []
            for i, fut in enumerate(futures):
                try:
                    payloads.append(fut.result())
                except Exception as e:
                    log.warning("%s: backend %s failed: %s", self.name, self.backends[i].name, e)
                    payloads.append(None)

        seen: set[str] = set()
        results: list[dict] = []
        for b, payload in zip(self.backends, payloads):
            if payload is None:
                continue
            for r in payload.get("results") or []:
                url = r.get("url") or ""
                if url in seen:
                    continue
                seen.add(url)
                r["_engine"] = getattr(b, "name", "unknown")
                results.append(r)

        payload = _empty(req)
        payload["results"] = results
        return payload
