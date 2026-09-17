import os
from pathlib import Path
from typing import Optional
from urllib.parse import quote

from fastapi import APIRouter, Query, Request
from fastapi.responses import FileResponse, HTMLResponse, JSONResponse, RedirectResponse, Response
from starlette.staticfiles import StaticFiles

from .config import SERVICE_NAME, VERSION
from .schemas import HealthResponse
from .state import AppState
from .ui_dist import _NO_UI_PAGE, _ui_dist_dir


def build_router(state: AppState) -> APIRouter:
    router = APIRouter()

    @router.get("/health", response_model=HealthResponse)
    def health() -> dict:
        return {
            "status": "ok",
            "service": SERVICE_NAME,
            "cache_size": state.cache.stats()["rows"],
            "version": VERSION,
            "pid": os.getpid(),
        }

    @router.get("/", response_class=HTMLResponse)
    def ui_search(q: Optional[str] = Query(default=None)) -> Response:
        if not q or not q.strip():
            dist = _ui_dist_dir()
            if dist is not None:
                return FileResponse(dist / "index.html", media_type="text/html")
            return HTMLResponse(_NO_UI_PAGE)
        return RedirectResponse(url=f"/search?q={quote(q)}", status_code=302)

    return router


def mount_static(app, dist: Path) -> None:
    """Static assets + SPA catch-all. Register after all API routers."""
    if (dist / "assets").is_dir():
        app.mount("/assets", StaticFiles(directory=dist / "assets"), name="assets")

    def _wants_json(request: Request) -> bool:
        accept = request.headers.get("accept", "")
        return "application/json" in accept and "text/html" not in accept

    @app.get("/{path:path}", include_in_schema=False)
    def spa_catch_all(path: str, request: Request) -> Response:
        # Prerendered file for this exact path? (e.g. dist/about.html for
        # /about, dist/404.html for /404)
        prerendered = None
        if path:
            for candidate in (dist / f"{path}.html", dist / path / "index.html"):
                if candidate.is_file():
                    prerendered = candidate
                    break
        if prerendered is not None:
            status = 404 if prerendered == dist / "404.html" else 200
            return FileResponse(prerendered, media_type="text/html", status_code=status)
        if _wants_json(request):
            return JSONResponse(
                status_code=404,
                content={"error": {"code": "not_found", "message": f"no route for /{path}"}},
            )
        return FileResponse(dist / "404.html", media_type="text/html", status_code=404)
