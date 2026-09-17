"""Typed errors for the ``oxe.search`` domain.

Per the error ladder in ``oxe/AGENTS.md``: domain modules raise typed
exceptions rather than bare ``Exception``, so every ``except`` elsewhere in
the codebase can name a concrete type.
"""


class BackendError(Exception):
    """Raised when a search engine/backend fails at the provider level.

    An empty ``results`` list is never an error signal on its own -- it
    means "no hits" and is returned normally. ``BackendError`` is reserved
    for genuine provider failure (timeout, rate limit, transport error, or
    the service-level rule that a non-empty ``unresponsive_engines`` with
    empty ``results`` is not a trustworthy empty result set).
    """
