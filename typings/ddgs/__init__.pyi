"""Minimal local stub for ddgs's public surface used by oxe.

ddgs ships no py.typed marker, so basedpyright infers ``DDGS`` and its
methods as ``Unknown``. Rather than let that leak into
oxe/search/engines/ddgs.py (or suppress it with ignore comments), this stub
declares the one method oxe actually calls (``text``) with a concrete
signature and return type. ``cast()`` at the single call site in
oxe/search/engines/ddgs.py still narrows the boundary explicitly, matching
the aiosql stub's rationale.
"""

class DDGS:
    def __init__(self) -> None: ...
    def text(self, query: str, **kwargs: object) -> list[dict[str, str]]: ...
