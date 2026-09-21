"""Guard the e2e burn-down: active checkpoints must have wired behaviors and a
baseline screenshot.

Companion to the markdown adapter (tests/e2e/checkpoints.md.ts): unskipping a
checkpoint .md without adding its behavior to the adapter's BEHAVIORS map (and
vice versa) fails this fast pytest gate, so `mise run validate` catches drift
before the slow Playwright run in `validate:full` even starts. This also
extends the pair check (checkpoint .md <-> adapter behavior) to a triple by
requiring a baseline screenshot in tests/e2e/checkpoints.md.ts-snapshots/ for
every active checkpoint, and rejecting orphan baselines left behind once a
checkpoint is skipped again.
"""

import re
from pathlib import Path

ADAPTER = Path(__file__).resolve().parent / "e2e" / "checkpoints.md.ts"
E2E_DIR = Path(__file__).resolve().parent / "e2e"
# Default Playwright snapshotPathTemplate for testDir=tests/e2e,
# testMatch='**/*.md.ts': {testFileName}-snapshots/{arg}-{platform}{ext}.
# No custom snapshotPathTemplate is set in ui/playwright.config.ts, so this is
# the convention already in effect, not one invented for this check.
SNAPSHOT_DIR = E2E_DIR / "checkpoints.md.ts-snapshots"

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


def _all_checkpoints() -> set[str]:
    numbers: set[str] = set()
    for path in sorted(E2E_DIR.glob("*.md")):
        n = NUMBER_RE.match(path.stem)
        assert n, f"{path.name}: does not start with '<n>-'"
        numbers.add(n.group(1))
    return numbers


def _wired_behaviors() -> set[str]:
    return set(BEHAVIOR_RE.findall(ADAPTER.read_text()))


def _baselined_checkpoints() -> set[str]:
    if not SNAPSHOT_DIR.is_dir():
        return set()
    numbers: set[str] = set()
    for path in sorted(SNAPSHOT_DIR.glob("*.png")):
        n = NUMBER_RE.match(path.name)
        assert n, f"{path.name}: baseline filename does not start with '<n>-'"
        numbers.add(n.group(1))
    return numbers


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


def test_active_checkpoints_have_baseline_screenshots() -> None:
    missing = _active_checkpoints() - _baselined_checkpoints()
    assert not missing, (
        "checkpoints marked status: active with no baseline screenshot in "
        f"tests/e2e/checkpoints.md.ts-snapshots/: {sorted(missing)} "
        "(run `bun x playwright test --update-snapshots` from ui/ to generate one)"
    )


def test_baseline_screenshots_target_active_checkpoints() -> None:
    skipped = _all_checkpoints() - _active_checkpoints()
    orphans = _baselined_checkpoints() & skipped
    assert not orphans, (
        "baseline screenshots exist in tests/e2e/checkpoints.md.ts-snapshots/ "
        f"for checkpoints whose .md file is status: skip: {sorted(orphans)} "
        "(delete the stale baseline or re-activate the checkpoint)"
    )
