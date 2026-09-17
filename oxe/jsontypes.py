"""Shared JSON-safe type aliases.

Cached payloads (search results, AI answers) are arbitrary JSON blobs whose
concrete shape lives elsewhere (``oxe/search/model.py``, not built yet — see
the v0.5.0 plan, wave 2 step 13). Until that model exists, ``cache.py`` needs
a parameterised stand-in so a bare, unparameterised ``dict`` never crosses a
module boundary and ``Any`` is never used.
"""

JSONValue = str | int | float | bool | None | dict[str, "JSONValue"] | list["JSONValue"]
JSONDict = dict[str, JSONValue]
