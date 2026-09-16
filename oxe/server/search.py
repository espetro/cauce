import logging
import time
from typing import Optional
from urllib.parse import quote

from fastapi import APIRouter, Header, Query, Request
from fastapi.responses import (
    FileResponse,
    HTMLResponse,
    JSONResponse,
    RedirectResponse,
    Response,
)

from .. import exa_compat
from ..devlog import DEV as _DEV
from ..devlog import event as _dev_event
from ..search import do_search
from .schemas import ExaRequest, SearchResponse
from .state import AppState
from .ui_dist import _NO_UI_PAGE, _shell, _ui_dist_dir

log = logging.getLogger(__name__)


def build_router(state: AppState) -> APIRouter:
    router = APIRouter()

    def _xcache_headers(payload: dict) -> dict:
        return {"X-Cache": "HIT" if payload.get("_source") == "cache" else "MISS"}

    def _serialize_search(payload: dict) -> dict:
        """Validate + alias-serialize a search payload (byte-identical _-fields)."""
        return SearchResponse.model_validate(payload).model_dump(
            by_alias=True, exclude_none=False
        )

    def _search_payload(q: str, num_results: int = 10, page: int = 1) -> dict:
        return do_search(
            state.cache,
            {
                "query": q,
                "numResults": num_results,
                "page": page,
                "contents": {"text": True, "highlights": True},
            },
            backend=state.backend,
            on_result=state.observer,
        )

    @router.post("/search", response_model=SearchResponse)
    def exa_search(
        req: ExaRequest,
        authorization: Optional[str] = Header(default=None),
        x_api_key: Optional[str] = Header(default=None, alias="x-api-key"),
    ) -> dict:
        if authorization or x_api_key:
            log.info("/search: auth header present (ignored)")
        req_dict = req.model_dump(exclude_none=True)
        if req_dict.get("numResults") is None:
            req_dict["numResults"] = 10
        refresh = bool(req_dict.pop("_refresh", False))
        if refresh:
            key = exa_compat.cache_key(
                req_dict | {"_backend": getattr(state.backend, "name", "ddg")}
            )
            state.cache.delete(key)
        _t0 = time.monotonic() if _DEV else None
        try:
            out = do_search(state.cache, req_dict, backend=state.backend, on_result=state.observer)
        except Exception as e:
            _dev_event(
                "search",
                q=req_dict.get("query", ""),
                page=req_dict.get("page", 1),
                error=str(e)[:200],
            )
            raise
        if _DEV:
            _dev_event(
                "search",
                q=req_dict.get("query", ""),
                page=req_dict.get("page", 1),
                source=out.get("_source"),
                duration_ms=out.get("_duration_ms")
                if out.get("_duration_ms") is not None
                else int((time.monotonic() - _t0) * 1000),
                results=len(out.get("results") or []),
            )
        return JSONResponse(_serialize_search(out), headers=_xcache_headers(out))

    @router.get("/search", response_class=HTMLResponse)
    def ui_search_get(
        q: Optional[str] = Query(default=None),
        p: int = Query(default=1, ge=1),
        request: Request = None,
        accept: Optional[str] = Header(default=None),
    ) -> Response:
        if not q or not q.strip():
            return RedirectResponse(url="/", status_code=302)
        q = q.strip()
        if accept and "application/json" in accept:
            payload = _search_payload(q, page=p)
            # Model only the JSON branch: validate + alias-serialize so the
            # documented schema matches the wire format (_-prefixed fields kept).
            # The HTML branch below stays outside the response model (content
            # negotiation is an invariant).
            return JSONResponse(
                _serialize_search(payload), headers=_xcache_headers(payload)
            )
        payload = _search_payload(q, page=p)
        dist = _ui_dist_dir()
        if dist is not None:
            return FileResponse(
                _shell(dist, "/search"),
                media_type="text/html",
                headers=_xcache_headers(payload),
            )
        return HTMLResponse(_NO_UI_PAGE, headers=_xcache_headers(payload))

    @router.get("/suggest")
    def suggest(q: str = Query(...)) -> list:
        """OpenSearch Suggestions JSON: ["prefix", ["s1", "s2"], ...]."""
        prefix = q.strip()
        if not prefix:
            return [prefix, [], [], []]
        out = [prefix, state.cache.suggest_queries(prefix, limit=3), [], []]
        _dev_event("suggest", q=prefix, results=len(out[1]))
        return out

    @router.get("/ac")
    def ac(q: str = Query(...)) -> list[str]:
        """Proxy DuckDuckGo autocomplete (browser CORS blocks direct calls)."""
        prefix = q.strip()
        if not prefix:
            return []
        try:
            import json as _json
            from urllib.request import Request, urlopen

            url = "https://duckduckgo.com/ac/?type=list&q=" + quote(prefix)
            req = Request(url, headers={"User-Agent": "oxe/autocomplete"})
            with urlopen(req, timeout=3) as resp:
                data = _json.loads(resp.read())
            phrases = data[1] if isinstance(data, list) and len(data) > 1 else []
            out = [str(p) for p in phrases][:6]
            _dev_event("ac", q=prefix, results=len(out))
            return out
        except Exception as e:
            log.warning("/ac: ddg autocomplete failed: %s", e)
            return []

    return router
