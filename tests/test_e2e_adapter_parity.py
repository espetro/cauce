"""Guard the e2e burn-down: active checkpoints must have wired behaviors.

Companion to the markdown adapter (tests/e2e/checkpoints.md.ts): unskipping a
checkpoint .md without adding its behavior to the adapter's BEHAVIORS map (and
vice versa) fails this fast pytest gate, so `mise run validate` catches drift
before the slow Playwright run in `validate:full` even starts.
"""

import re
from pathlib import Path

ADAPTER = Path(__file__).resolve().parent / "e2e" / "checkpoints.md.ts"
E2E_DIR = Path(__file__).resolve().parent / "e2e"

STATUS_RE = re.compile(r"^status:\s*(skip|active)\s*$", re.MULTILINE)
NUMBER_RE = re.compile(r"^(\d+)-")
BEHAVIOR_RE = re.compile(r"^\s+'(\d+)':", re.MULTILINE)


def _active_checkpoints() -> set[str]:
    numbers: set[str] = set()
    for path in sorted(E2E_DIR.glob("*.md")):
        match = STATUS_RE.search(path.read_text())
        assert match, f"{path.name}: missing status: frontmatter"
        if match.group(1) == "active":
            n = NUMBER_RE.match(path.stem)
            assert n, f"{path.name}: does not start with '<n>-'"
            numbers.add(n.group(1))
    return numbers


def _wired_behaviors() -> set[str]:
    return set(BEHAVIOR_RE.findall(ADAPTER.read_text()))


def test_active_checkpoints_have_adapter_behaviors() -> None:
    unwired = _active_checkpoints() - _wired_behaviors()
    assert not unwired, (
        "checkpoints marked status: active with no behavior wired in "
        f"tests/e2e/checkpoints.md.ts: {sorted(unwired)}"
    )


def test_adapter_behaviors_target_active_checkpoints() -> None:
    phantom = _wired_behaviors() - _active_checkpoints()
    assert not phantom, (
        "behaviors wired in tests/e2e/checkpoints.md.ts for checkpoints whose "
        f".md file is status: skip (unskip the file first): {sorted(phantom)}"
    )
