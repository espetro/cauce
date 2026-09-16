import os
from pathlib import Path

_NO_UI_PAGE = """<!doctype html><html><head><title>oxe</title></head>
<body style="font-family:system-ui,sans-serif;max-width:36rem;margin:4rem auto;padding:0 1rem">
<h1>oxe web UI not built</h1>
<p>The SPA bundle is not present. Options:</p>
<ul>
<li>Dev checkout: run <code>mise run build:ui</code> (builds <code>ui/dist</code>),
   or set <code>OXE_UI_DIST</code>.</li>
<li>Installed package: set <code>OXE_UI_DIST</code> to a directory containing
   <code>index.html</code>.</li>
</ul>
<p>The JSON API (<code>POST /search</code>) and MCP (<code>/mcp/</code>) work without the UI.</p>
</body></html>"""


def _ui_dist_dir() -> Path | None:
    """Locate the built web UI (Preact SPA), if present.

    Order: $OXE_UI_DIST, ./ui/dist (repo checkout), packaged oxe/ui_dist.
    """
    env = os.getenv("OXE_UI_DIST", "").strip()
    if env:
        p = Path(env).expanduser()
        return p if (p / "index.html").is_file() else None
    repo = Path("ui/dist")
    if (repo / "index.html").is_file():
        return repo
    pkg = Path(__file__).parent.parent / "ui_dist"
    if (pkg / "index.html").is_file():
        return pkg
    return None


def _shell(dist: Path, route: str) -> Path:
    """Per-route prerendered shell (dist/<route>/index.html), else root shell."""
    shell = dist / route.strip("/") / "index.html"
    return shell if shell.is_file() else dist / "index.html"


_MEDIA_TYPES = {
    ".html": "text/html",
    ".js": "application/javascript",
    ".css": "text/css",
    ".json": "application/json",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".ico": "image/x-icon",
    ".woff": "font/woff",
    ".woff2": "font/woff2",
    ".map": "application/json",
    ".txt": "text/plain",
    ".webmanifest": "application/manifest+json",
}
