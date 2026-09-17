"""Stub for aiosql.queries.Queries; see aiosql/__init__.pyi for rationale."""

from collections.abc import Callable

class Queries:
    def __getattr__(self, name: str) -> Callable[..., object]: ...
