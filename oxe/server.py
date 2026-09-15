import logging
import os
import time
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Callable, Optional
from urllib.parse import quote

from fastapi import FastAPI, Form, Header, HTTPException, Query, Request
from fastapi.responses import HTMLResponse, JSONResponse, RedirectResponse, Response
from mcp.server.transport_security import TransportSecuritySettings
from pydantic import BaseModel
from typing import Any

from .cache import TTLCache
from .search import do_search
from . import ui
from . import __version__

log = logging.getLogger(__name__)

PORT = int(os.getenv("OXE_PORT", "4479"))
CACHE_DIR = os.getenv("OXE_CACHE_DIR", os.path.expanduser("~/.cache/oxe"))
LOG_LEVEL = os.getenv("OXE_LOG_LEVEL", "INFO").upper()

logging.basicConfig(
    level=getattr(logging, LOG_LEVEL, logging.INFO),
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
)

VERSION = __version__
SERVICE_NAME = "oxe"
SEARCH_LOG_RETENTION_DAYS = int(os.getenv("OXE_SEARCH_LOG_RETENTION_DAYS", "30"))


class ContentsModel(BaseModel):
    text: Any = None
    highlights: Any = None
    summary: Any = None

    model_config = {"extra": "allow"}


class ExaRequest(BaseModel):
    query: str
    type: Optional[str] = "auto"
    numResults: Optional[int] = 10
    category: Optional[str] = None
    userLocation: Optional[str] = None
    includeDomains: Optional[list[str]] = None
    excludeDomains: Optional[list[str]] = None
    startPublishedDate: Optional[str] = None
    endPublishedDate: Optional[str] = None
    contents: Optional[ContentsModel] = None
    additionalQueries: Optional[list[str]] = None
    systemPrompt: Optional[str] = None
    outputSchema: Optional[dict] = None
    stream: Optional[bool] = None

    model_config = {"extra": "allow"}


# module-level cache kept for mcp_server wiring and backward compat
cache = TTLCache(os.path.join(CACHE_DIR, "cache.db"))
log.info("cache initialized at %s/cache.db", CACHE_DIR)

_STATIC_DIR = Path(__file__).parent / "static"
CLICK_RETENTION_DAYS = int(os.getenv("OXE_CLICK_RETENTION_DAYS", "30"))


def _http_search_logger(c: TTLCache) -> Callable[[dict], None]:
    """Default observer for make_app: writes search_log rows, client='http'."""

    def _log(payload: dict) -> None:
        c.log_search(
            query_text=(payload.get("_q") or "")[:200],
            query_hash=payload.get("_q_hash") or "",
            source=payload.get("_source") or "network",
            backend=payload.get("_backend", "ddg"),
            result_count=len(payload.get("results") or []),
            duration_ms=payload.get("_duration_ms"),
            client="http",
        )

    return _log


def make_app(
    cache: TTLCache | None = None,
    backend: object | None = None,
    on_result: Callable[[dict], None] | None = None,
) -> FastAPI:
    """Build the oxe FastAPI app. All handlers close over the given cache.

    cache defaults to the module-level instance; on_result defaults to a
    search_log writer with client='http'.
    """
    c = cache or globals()["cache"]
    if backend is None:
        from .registry import build_from_env

        backend = build_from_env()
    observer = on_result or _http_search_logger(c)

    try:
        from .mcp_server import mcp, set_cache

        set_cache(c)
        mcp_app = mcp.streamable_http_app(
            streamable_http_path="/",
            transport_security=TransportSecuritySettings(enable_dns_rebinding_protection=False),
        )

        @asynccontextmanager
        async def lifespan(app: FastAPI):
            async with mcp_app.router.lifespan_context(mcp_app):
                yield

        app = FastAPI(title=SERVICE_NAME, version=VERSION, lifespan=lifespan)
        app.mount("/mcp", mcp_app)
        log.info("MCP route mounted at /mcp (DNS rebinding protection disabled)")
    except Exception as e:
        log.exception("failed to mount MCP sub-app: %s", e)
        app = FastAPI(title=SERVICE_NAME, version=VERSION)

    @app.on_event("startup")
    def _prune_on_startup() -> None:
        n = c.prune_clicks(CLICK_RETENTION_DAYS)
        if n:
            log.info("startup: pruned %d clicks older than %d days", n, CLICK_RETENTION_DAYS)
        m = c.prune_search_log(SEARCH_LOG_RETENTION_DAYS)
        if m:
            log.info(
                "startup: pruned %d search_log rows older than %d days",
                m,
                SEARCH_LOG_RETENTION_DAYS,
            )

    @app.get("/health")
    def health() -> dict:
        return {
            "status": "ok",
            "service": SERVICE_NAME,
            "cache_size": c.stats()["rows"],
            "version": VERSION,
            "pid": os.getpid(),
        }

    @app.post("/search")
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
        return do_search(c, req_dict, backend=backend, on_result=observer)

    def _search_payload(q: str, num_results: int = 10) -> dict:
        return do_search(
            c,
            {
                "query": q,
                "numResults": num_results,
                "contents": {"text": True, "highlights": True},
            },
            backend=backend,
            on_result=observer,
        )

    @app.get("/search", response_class=HTMLResponse)
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
            return JSONResponse(_search_payload(q))
        try:
            initial_results = _search_payload(q)
        except Exception as e:
            log.exception("UI: initial search for %r failed: %s", q, e)
            initial_results = None
        share = _share_info(c, q, initial_results)
        title, body = ui.render_search(
            initial_query=q,
            initial_results=initial_results,
            share=share,
            page=p,
        )
        return HTMLResponse(ui.render_shell(title, body, VERSION, page_class="search"))

    @app.get("/suggest")
    def suggest(q: str = Query(...)) -> list:
        """OpenSearch Suggestions JSON: ["prefix", ["s1", "s2"], ...]."""
        prefix = q.strip()
        if not prefix:
            return [prefix, [], [], []]
        return [prefix, c.suggest_queries(prefix, limit=3), [], []]

    @app.get("/cache/stats")
    def cache_stats() -> dict:
        return c.stats()

    @app.post("/cache/invalidate")
    def cache_invalidate(
        authorization: Optional[str] = Header(default=None),
        x_api_key: Optional[str] = Header(default=None, alias="x-api-key"),
    ) -> dict:
        actor = (authorization or x_api_key or "").split()[-1][:12] if (authorization or x_api_key) else "anonymous"
        log.warning("/cache/invalidate called by %s", actor)
        deleted = c.invalidate()
        return {"deleted": deleted}

    @app.get("/", response_class=HTMLResponse)
    def ui_search(q: Optional[str] = Query(default=None)) -> Response:
        if not q or not q.strip():
            title, body = ui.render_search(initial_query="")
            return HTMLResponse(ui.render_shell(title, body, VERSION, page_class="search"))
        return RedirectResponse(url=f"/search?q={quote(q)}", status_code=302)

    @app.get("/history", response_class=HTMLResponse)
    def ui_history(
        q: Optional[str] = Query(default=None),
        since: Optional[str] = Query(default=None),
    ) -> str:
        since_hours = int(since) if since else None
        title, body = ui.render_history(c, q=q, since_hours=since_hours)
        return ui.render_shell(title, body, VERSION, page_class="history")

    @app.post("/history/delete")
    def ui_history_delete(scope: str = Form(...)) -> RedirectResponse:
        n = c.delete_clicks(scope)
        log.info("UI: deleted %d clicks (scope=%s)", n, scope)
        return RedirectResponse(url="/history", status_code=303)

    @app.post("/click")
    def ui_click(payload: dict) -> dict:
        qh = (payload.get("query_hash") or "").strip()
        rid = (payload.get("result_id") or "").strip()
        url = (payload.get("url") or "").strip()
        title = (payload.get("title") or "").strip()[:500]
        source = (payload.get("source") or "web").strip()[:16]
        if not (qh and rid and url):
            raise HTTPException(status_code=422, detail="query_hash, result_id, url required")
        click_id = c.record_click(qh, rid, url, title, source=source)
        return {"ok": True, "click_id": click_id}

    @app.get("/cache", response_class=HTMLResponse)
    def ui_cache(
        q: Optional[str] = Query(default=None),
        include_expired: bool = Query(default=False),
        page: int = Query(default=0, ge=0),
    ) -> str:
        title, body = ui._render_index(c, q=q, include_expired=include_expired, page=page)
        return ui.render_shell(title, body, VERSION, page_class="cache")

    @app.get("/row/{key}")
    def ui_row(key: str) -> Response:
        qtext = c.lookup_query_text(key)
        if not qtext:
            raise HTTPException(status_code=404, detail="row not found")
        return RedirectResponse(url=f"/search?q={quote(qtext)}", status_code=302)

    @app.post("/row/{key}/delete")
    def ui_row_delete(key: str) -> RedirectResponse:
        if not c.delete(key):
            raise HTTPException(status_code=404, detail="cache row not found")
        log.info("UI: deleted cache row %s", key[:12])
        return RedirectResponse(url="/cache", status_code=303)

    @app.get("/static/{name}")
    def ui_static(name: str) -> Response:
        if "/" in name or ".." in name:
            raise HTTPException(status_code=400, detail="bad path")
        p = _STATIC_DIR / name
        if not p.is_file():
            raise HTTPException(status_code=404, detail="not found")
        media = "text/css" if name.endswith(".css") else "application/javascript" if name.endswith(".js") else "text/plain"
        return Response(p.read_text(encoding="utf-8"), media_type=media)

    return app


def _share_info(c: TTLCache, q: str, payload: dict | None) -> dict | None:
    """Build Share row data for the search UI: query, count, source, age, ttl."""
    if payload is None:
        return None
    count = len(payload.get("results") or [])
    source = payload.get("_source") or "network"
    qh = payload.get("_q_hash") or ""
    age_s = None
    ttl_left = None
    if qh:
        rows = c.list_rows(q=q, limit=5)
        row = next((r for r in rows if r["hash"] == qh), None)
        if row is None and rows:
            row = rows[0]
        if row is not None:
            now = int(time.time())
            if row["expired"]:
                ttl_left = 0
            else:
                total = max(row["expires_at"] - now, 0)
                ttl_left = total
    return {
        "query": q,
        "result_count": count,
        "source": source,
        "query_hash": qh,
        "age_s": age_s,
        "ttl_left_s": ttl_left,
    }


# module-level app for `uvicorn oxe.server:app` backward compat
app = make_app()
