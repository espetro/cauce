"""SQL loading: .sql files in oxe/sql/ via aiosql, resolved from the package."""

from importlib import resources

import aiosql

_queries = None


def queries():
    """Load and cache the named-query registry from oxe/sql/*.sql."""
    global _queries
    if _queries is None:
        with resources.as_file(resources.files(__package__) / "sql") as sql_dir:
            _queries = aiosql.from_path(str(sql_dir), "sqlite3")
    return _queries


def schema_sql() -> str:
    """Raw schema text (executed directly, not a named query)."""
    return (resources.files(__package__).joinpath("sql/schema.sql")).read_text(encoding="utf-8")
