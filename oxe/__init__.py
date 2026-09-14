"""oxe: Exa-compatible web-search proxy and MCP server backed by DuckDuckGo."""

__version__ = "0.3.0"

from .backends import BackendError, FallbackBackend, FanoutBackend, SearchBackend
from .cache import TTLCache
from .registry import build_from_env
from .search import do_search
from .server import make_app

__all__ = [
    "BackendError",
    "FallbackBackend",
    "FanoutBackend",
    "SearchBackend",
    "TTLCache",
    "build_from_env",
    "do_search",
    "make_app",
    "__version__",
]
