"""The minimal no-UI fallback page: loads inline, explains how to get the SPA."""

from oxe.server import _NO_UI_PAGE


def test_no_ui_page():
    assert "<html" in _NO_UI_PAGE.lower()
    assert "OXE_UI_DIST" in _NO_UI_PAGE
    assert "mise run build:ui" in _NO_UI_PAGE


if __name__ == "__main__":
    test_no_ui_page()
    print("ok: fallback page present")
