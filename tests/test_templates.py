"""Sanity checks: each HTML template loads from oxe.static and has placeholders."""

from oxe.ui import _CARD, _HISTORY, _INDEX, _ROW, _SEARCH, _SHELL

EXPECTED = {
    _SHELL: ("${title}", "${body}"),
    _INDEX: ("${stats_line}", "${rows_html}"),
    _SEARCH: ("${results_html}", "landing-${landing}"),
    _HISTORY: ("${stats_line}", "${sel_24}"),
    _ROW: ("${q_esc}", "${raw}"),
    _CARD: ("${rid}", "${snippet_html}"),
}


def test_templates():
    for tpl, needles in EXPECTED.items():
        for needle in needles:
            assert needle in tpl.template, f"{tpl.template[:40]!r} missing {needle}"


if __name__ == "__main__":
    test_templates()
    print("ok: all 6 templates load and contain expected placeholders")
