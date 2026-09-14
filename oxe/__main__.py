import os

import uvicorn

from .server import app


def main() -> None:
    uvicorn.run(
        app,
        host="127.0.0.1",
        port=int(os.getenv("OXE_PORT", "4479")),
        workers=1,
        log_level=os.getenv("OXE_LOG_LEVEL", "info").lower(),
    )


if __name__ == "__main__":
    main()
