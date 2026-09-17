"""Typed errors for the ``oxe.ai`` domain (error ladder rule 1).

The one broad boundary handler lives in ``oxe/api/errors.py``; everything
here is a concrete exception other modules can name.
"""


class ProviderError(Exception):
    """Raised when the AI provider endpoint fails (transport, HTTP, auth).

    Carries an already-friendly, user-facing message (see
    ``oxe.ai._friendly_provider_error``); ``str(exc)`` is what gets streamed
    to the client in the ``done`` frame's ``error`` field.
    """
