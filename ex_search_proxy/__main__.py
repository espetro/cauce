import os

import uvicorn

from .server import app


def main() -> None:
    uvicorn.run(
        app,
        host="127.0.0.1",
        port=int(os.getenv("EX_SEARCH_PORT", "4479")),
        workers=1,
        log_level=os.getenv("EX_SEARCH_LOG_LEVEL", "info").lower(),
    )


if __name__ == "__main__":
    main()
