"""CLI entrypoint for running the oxe server."""

import uvicorn


def main() -> None:
    """Run the oxe FastAPI application with uvicorn."""
    uvicorn.run("oxe.app:app", host="127.0.0.1", port=4479)


if __name__ == "__main__":
    main()
