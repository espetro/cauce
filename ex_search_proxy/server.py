import logging
import os
from contextlib import asynccontextmanager
from typing import Optional

from fastapi import FastAPI, Header
from mcp.server.transport_security import TransportSecuritySettings
from pydantic import BaseModel
from typing import Any

from .cache import TTLCache
from .search import do_search

log = logging.getLogger(__name__)

PORT = int(os.getenv("EX_SEARCH_PORT", "4479"))
CACHE_DIR = os.getenv("EX_SEARCH_CACHE_DIR", os.path.expanduser("~/.cache/ex-search-proxy"))
LOG_LEVEL = os.getenv("EX_SEARCH_LOG_LEVEL", "INFO").upper()

logging.basicConfig(
    level=getattr(logging, LOG_LEVEL, logging.INFO),
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
)

VERSION = "0.1.0"
SERVICE_NAME = "ex-search-proxy"


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


cache = TTLCache(os.path.join(CACHE_DIR, "cache.db"))
log.info("cache initialized at %s/cache.db", CACHE_DIR)

try:
    from .mcp_server import mcp, set_cache

    set_cache(cache)
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


@app.get("/health")
def health() -> dict:
    return {
        "status": "ok",
        "service": SERVICE_NAME,
        "cache_size": cache.stats()["rows"],
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
    return do_search(cache, req_dict)


@app.get("/cache/stats")
def cache_stats() -> dict:
    return cache.stats()


@app.post("/cache/invalidate")
def cache_invalidate(
    authorization: Optional[str] = Header(default=None),
    x_api_key: Optional[str] = Header(default=None, alias="x-api-key"),
) -> dict:
    actor = (authorization or x_api_key or "").split()[-1][:12] if (authorization or x_api_key) else "anonymous"
    log.warning("/cache/invalidate called by %s", actor)
    deleted = cache.invalidate()
    return {"deleted": deleted}
