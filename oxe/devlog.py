"""Dev-only structured logging for the QA loop.

Emits one-line JSON per key event at DEBUG level, only when dev mode is
active: OXE_DEV=1 or OXE_LOG_LEVEL=DEBUG. Production INFO stays
human-readable and spam-free (these lines simply never fire).
"""

import json
import logging
import os
import time

log = logging.getLogger("oxe.dev")


def dev_enabled() -> bool:
    return os.getenv("OXE_DEV") == "1" or os.getenv("OXE_LOG_LEVEL", "INFO").upper() == "DEBUG"


def event(name: str, **fields: object) -> None:
    """Emit one structured JSON line. No-op unless dev mode is on."""
    if not dev_enabled():
        return
    if not log.isEnabledFor(logging.DEBUG):
        # OXE_DEV=1 alone must surface DEBUG lines even when root is INFO.
        log.setLevel(logging.DEBUG)
    fields["event"] = name
    fields["ts"] = round(time.time(), 3)
    try:
        log.debug(json.dumps(fields, ensure_ascii=False, default=str))
    except (TypeError, ValueError):
        log.debug("devlog: failed to serialize event=%s", name)
