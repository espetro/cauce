import logging
from contextlib import asynccontextmanager
from typing import Callable

from fastapi import FastAPI, HTTPException, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse

from ..cache import TTLCache
from . import ai as ai_router_mod
from . import cache_admin, system
from . import search as search_mod
from .config import (
    CLICK_RETENTION_DAYS,
    SEARCH_LOG_RETENTION_DAYS,
    SERVICE_NAME,
    VERSION,
)
from .state import AppState, _http_search_logger
from .state import cache as default_cache

log = logging.getLogger(__name__)


def make_app(
    cache: TTLCache | None = None,
    backend: object | None = None,
    on_result: Callable[[dict], None] | None = None,
) -> FastAPI:
    """Build the oxe FastAPI app. All handlers close over the given cache.

    cache defaults to the module-level instance; on_result defaults to a
    search_log writer with client='http'.
    """
    c = cache or default_cache
    if backend is None:
        from ..registry import build_from_env

        backend = build_from_env()
    observer = on_result or _http_search_logger(c)
    state = AppState(cache=c, backend=backend, observer=observer)

    try:
        from mcp.server.transport_security import TransportSecuritySettings

        from ..mcp_server import mcp, set_cache

        set_cache(c)
        mcp_app = mcp.streamable_http_app(
            streamable_http_path="/",
            transport_security=TransportSecuritySettings(enable_dns_rebinding_protection=False),
        )

        @asynccontextmanager
        async def lifespan(app: FastAPI):
            try:
                async with mcp_app.router.lifespan_context(mcp_app):
                    _prune_on_startup()
                    yield
            finally:
                c.close()

        app = FastAPI(title=SERVICE_NAME, version=VERSION, lifespan=lifespan)
        app.mount("/mcp", mcp_app)
        log.info("MCP route mounted at /mcp (DNS rebinding protection disabled)")
    except Exception as e:
        log.exception("failed to mount MCP sub-app: %s", e)

        @asynccontextmanager
        async def lifespan(app: FastAPI):
            try:
                _prune_on_startup()
                yield
            finally:
                c.close()

        app = FastAPI(title=SERVICE_NAME, version=VERSION, lifespan=lifespan)

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

    # Unified error envelope: {"error": {"code": "<http_error|validation_error>",
    # "message": ...}}. Status codes are preserved from the original exception.
    def _envelope_error(status: int, message: str, code: str) -> JSONResponse:
        return JSONResponse(
            status_code=status,
            content={"error": {"code": code, "message": message}},
        )

    @app.exception_handler(HTTPException)
    async def _http_exception_handler(request: Request, exc: HTTPException):
        return _envelope_error(exc.status_code, str(exc.detail), "http_error")

    @app.exception_handler(RequestValidationError)
    async def _validation_exception_handler(request: Request, exc: RequestValidationError):
        parts = []
        for err in exc.errors():
            loc = ".".join(str(p) for p in err.get("loc", []) if p != "body")
            msg = err.get("msg", "invalid value")
            parts.append(f"{loc}: {msg}" if loc else msg)
        return _envelope_error(422, "; ".join(parts) or "validation error", "validation_error")

    app.include_router(system.build_router(state))
    app.include_router(search_mod.build_router(state))
    app.include_router(cache_admin.build_router(state))
    app.include_router(ai_router_mod.build_router(state))

    # Static SPA hosting last: hashed assets + SPA catch-all (registered after
    # every API router so API routes always win).
    from .system import mount_static
    from .ui_dist import _ui_dist_dir

    dist = _ui_dist_dir()
    if dist is not None:
        mount_static(app, dist)
    return app
