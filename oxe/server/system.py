import os
from typing import Optional
from urllib.parse import quote

from fastapi import APIRouter, HTTPException, Query
from fastapi.responses import FileResponse, HTMLResponse, RedirectResponse, Response

from .config import SERVICE_NAME, VERSION
from .state import AppState
from .ui_dist import _MEDIA_TYPES, _NO_UI_PAGE, _ui_dist_dir


def build_router(state: AppState) -> APIRouter:
    router = APIRouter()

    @router.get("/health")
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

    @router.get("/assets/{name}")
    def ui_assets(name: str) -> Response:
        """Serve built SPA assets from the UI dist dir (404 when absent)."""
        if "/" in name or ".." in name:
            raise HTTPException(status_code=400, detail="bad path")
        dist = _ui_dist_dir()
        p = dist / "assets" / name if dist else None
        if p is None or not p.is_file():
            raise HTTPException(status_code=404, detail="not found")
        media = _MEDIA_TYPES.get(p.suffix, "application/octet-stream")
        return FileResponse(p, media_type=media)

    return router
