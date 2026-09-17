"""Minimal local stub for aiosql's public surface used by oxe.

aiosql ships no py.typed marker, so basedpyright infers most of its API as
partially-unknown. Rather than let that ``Unknown``-ness leak into oxe (or
suppress it with ignore comments), this stub declares the one entry point
oxe actually calls (``from_path``) with a concrete return type. Downstream
consumption of the dynamically-generated query object is still centralised
through ``getattr`` + ``cast`` in ``oxe/sqlload.py``, since aiosql builds
its query attributes at runtime from ``-- name:`` SQL comments and cannot be
statically typed any more precisely than that.
"""

from .queries import Queries as Queries

def from_path(sql_path: str, driver_adapter: str) -> Queries: ...
