"""oxe.server package: FastAPI app split into APIRouter modules.

Compat surface re-exports keep `from oxe.server import make_app, app, ...`
working (tests, oxe.__init__, oxe.__main__, embedding callers).
"""

from .app import make_app
from .cache_admin import _share_info
from .schemas import ContentsModel, ExaRequest
from .state import cache
from .ui_dist import _NO_UI_PAGE, _ui_dist_dir

# module-level app for `uvicorn oxe.server:app` backward compat
app = make_app()

__all__ = [
    "make_app",
    "app",
    "cache",
    "_ui_dist_dir",
    "_NO_UI_PAGE",
    "ExaRequest",
    "ContentsModel",
    "_share_info",
]
