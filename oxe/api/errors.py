"""The one broad HTTP boundary handler (error ladder rule 3 in ``oxe/AGENTS.md``).

Domain modules (``oxe.search.errors.BackendError`` today; more will land as
other domains grow typed exceptions) raise typed exceptions. This module is
the single place that catches them broadly and maps them onto a wire
``ErrorEnvelope``, registered as a FastAPI exception handler in
``oxe.app.create_app``. Nowhere else in the HTTP layer should catch a domain
exception broadly -- a route that needs to react to a specific failure names
the concrete exception type.
"""

from pydantic import BaseModel, ConfigDict
from starlette.requests import Request
from starlette.responses import JSONResponse

from oxe.search.errors import BackendError

BACKEND_ERROR_STATUS = 502


class ErrorDetail(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    code: str
    message: str


class ErrorEnvelope(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, strict=True)

    error: ErrorDetail


async def backend_error_handler(request: Request, exc: Exception) -> JSONResponse:
    """Maps ``BackendError`` (provider failure, timeout, or the
    unresponsive-engines-with-no-results invariant) onto a 502 envelope.

    FastAPI's ``add_exception_handler`` types the handler's second parameter
    as the registered exception class; it is narrowed here since this
    function is only ever registered for ``BackendError``.
    """
    del request
    if not isinstance(exc, BackendError):  # pragma: no cover - defensive, see docstring
        raise exc
    envelope = ErrorEnvelope(error=ErrorDetail(code="backend_error", message=str(exc)))
    return JSONResponse(status_code=BACKEND_ERROR_STATUS, content=envelope.model_dump())
