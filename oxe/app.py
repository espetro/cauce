"""FastAPI application entrypoint.

Wave 1 scope: a single ``/health`` route. Feature routers land in later waves
(see ``.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md``).
"""

from fastapi import FastAPI
from pydantic import BaseModel


class HealthStatus(BaseModel):
    """Response body for the liveness check."""

    model_config = {"extra": "forbid", "frozen": True, "strict": True}

    status: str


def create_app() -> FastAPI:
    """Build and return the oxe FastAPI application."""
    app = FastAPI(title="oxe")

    @app.get("/health", response_model=HealthStatus)
    async def health() -> HealthStatus:
        return HealthStatus(status="ok")

    return app


app = create_app()
