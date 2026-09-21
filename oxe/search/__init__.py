"""Vendored search module: the only place search logic lives.

``model.py`` holds the canonical SearXNG-shaped wire types. ``engines/``
holds the pluggable, stateless backends (``ddgs.py`` today, ``searxng.py``
over real HTTP later, ``wikipedia.py`` a low-rate-limit keyless dev/test
backend for realistic-but-narrow data, not a production web-search
substitute) plus their composition and discovery machinery.
``service.py`` wires query -> engine -> cache -> ``SearxResponse``.
"""
