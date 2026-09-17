"""SQL loading: .sql files in oxe/sql/ via aiosql, resolved from the package.

``aiosql`` builds its query registry dynamically from ``-- name:`` comments in
the ``.sql`` files, so it has no static type signature to offer basedpyright —
every attribute on the object it returns is effectively ``Any``. ``query()``
is the single, narrow place that reaches into that dynamism (via ``getattr``
+ ``cast``) so the rest of the codebase only ever sees a concretely typed
``Callable[..., object]``.

``aiosql.Queries`` in the ``queries()`` return annotation resolves only via
the local stub in ``typings/aiosql/`` (basedpyright); the real package
exposes it as ``aiosql.queries.Queries``, not a top-level attribute, so the
annotation would fail at import time without ``from __future__ import
annotations`` deferring its evaluation.
"""

from __future__ import annotations

from collections.abc import Callable
from functools import lru_cache
from importlib import resources
from typing import cast

import aiosql

_PACKAGE = "oxe"

QueryFn = Callable[..., object]


@lru_cache(maxsize=1)
def queries() -> aiosql.Queries:
    """Load and cache the named-query registry from oxe/sql/*.sql."""
    with resources.as_file(resources.files(_PACKAGE) / "sql") as sql_dir:
        return aiosql.from_path(str(sql_dir), "sqlite3")


def query(name: str) -> QueryFn:
    """Look up a named query as a plain callable.

    ``getattr`` with a non-literal name always types as ``Any`` per typeshed,
    regardless of how precisely ``aiosql.Queries`` itself is typed (its
    attributes are generated at runtime from ``-- name:`` SQL comments, so
    there is no static member list to check against). ``cast()`` is the
    single point that boundary is converted into a concrete callable type.
    """
    return cast(QueryFn, getattr(queries(), name))


def schema_sql() -> str:
    """Raw schema text (executed directly, not a named query)."""
    return resources.files(_PACKAGE).joinpath("sql/schema.sql").read_text(encoding="utf-8")
