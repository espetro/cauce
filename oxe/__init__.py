"""oxe: Exa-compatible web-search proxy and MCP server backed by DuckDuckGo."""

__version__ = "0.2.0"

from .cache import TTLCache
from .search import do_search
from .server import make_app

__all__ = [
    "TTLCache",
    "do_search",
    "make_app",
    "__version__",
]
