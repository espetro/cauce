"""Vendored search module: the only place search logic lives.

``model.py`` holds the canonical SearXNG-shaped wire types. ``engines/``
holds the pluggable, stateless backends (``ddgs.py`` today, ``searxng.py``
over real HTTP later) plus their composition and discovery machinery.
``service.py`` wires query -> engine -> cache -> ``SearxResponse``.
"""
