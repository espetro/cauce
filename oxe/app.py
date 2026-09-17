"""FastAPI application entrypoint.

Wave 2 scope: ``/health`` plus the canonical search router
(``oxe/api/searx.py``). Feature routers land in later waves (see
``.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md``).
"""

import os
from pathlib import Path

from fastapi import FastAPI
from pydantic import BaseModel

from oxe.api import answer, history, searx, settings, stats
from oxe.api.ai_frames import register_answer_frame_schemas
from oxe.api.errors import backend_error_handler, config_error_handler
from oxe.cache import TTLCache
from oxe.config import ConfigError, cache_db_path
from oxe.search.engines.registry import build_from_env
from oxe.search.errors import BackendError
from oxe.search.service import SearchService


class HealthStatus(BaseModel):
    """Response body for the liveness check."""

    model_config = {"extra": "forbid", "frozen": True, "strict": True}

    status: str


def create_app() -> FastAPI:
    """Build and return the oxe FastAPI application.

    Constructs exactly one ``TTLCache`` (pointed at ``OXE_CACHE_DIR`` via
    ``oxe.config.cache_db_path``) and one engine (resolved from
    ``OXE_BACKENDS``, defaulting to plain DuckDuckGo) per process, wired into
    one ``SearchService`` on ``app.state``. Route handlers reach it via the
    ``oxe.api.searx.get_search_service`` dependency rather than a module
    global, so tests can swap ``app.state.search_service`` for a fake.
    """
    app = FastAPI(title="oxe")
    app.state.search_service = SearchService(build_from_env(), TTLCache(cache_db_path()))
    app.add_exception_handler(BackendError, backend_error_handler)
    app.add_exception_handler(ConfigError, config_error_handler)
    app.include_router(searx.router)
    app.include_router(stats.router)
    app.include_router(history.router)
    app.include_router(settings.router)
    app.include_router(answer.router)

    @app.get("/health", response_model=HealthStatus)
    async def health() -> HealthStatus:
        return HealthStatus(status="ok")

    register_answer_frame_schemas(app)

    dist_dir = os.environ.get("OXE_DIST_DIR")
    if dist_dir and Path(dist_dir).is_dir():
        app.frontend("/", directory=Path(dist_dir), fallback="index.html")
    return app


app = create_app()
