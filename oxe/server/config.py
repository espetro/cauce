import logging
import os

from .. import __version__

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
CLICK_RETENTION_DAYS = int(os.getenv("OXE_CLICK_RETENTION_DAYS", "30"))
