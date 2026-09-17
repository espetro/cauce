"""Enforces the checkpoint list <-> e2e file structural contract.

`.agents/docs/screens/userflow-checkpoints.md` is the source of truth for
which user-flow checkpoints exist. Per the v0.5.0 archive-rebuild plan
(`.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md`, "the structural
upgrade" + wave 1 step 10), every numbered checkpoint in that file must have
exactly one matching `tests/e2e/<n>-<slug>.md` file, and every file under
`tests/e2e/` must correspond to a real checkpoint. A checkpoint with no e2e
file is red; an e2e file with no checkpoint (e.g. left behind after a spec
edit) is also red. This is what turns "the screen list" into a burn-down
instead of a judgement call.
"""

import re
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CHECKPOINTS_FILE = REPO_ROOT / ".agents/docs/screens/userflow-checkpoints.md"
E2E_DIR = REPO_ROOT / "tests/e2e"

CHECKPOINT_HEADING_RE = re.compile(r"^### (\d+)\.\s+(.+)$", re.MULTILINE)
E2E_FILENAME_RE = re.compile(r"^(\d+)-(.+)\.md$")


def _slugify(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")


def _checkpoints_from_spec() -> dict[str, str]:
    """Map checkpoint number -> expected e2e filename stem."""
    text = CHECKPOINTS_FILE.read_text()
    matches = CHECKPOINT_HEADING_RE.findall(text)
    assert matches, f"no '### N. Title' checkpoint headings found in {CHECKPOINTS_FILE}"
    return {n: f"{n}-{_slugify(title)}" for n, title in matches}


def _e2e_file_stems() -> dict[str, str]:
    """Map checkpoint number -> e2e filename stem found on disk."""
    assert E2E_DIR.is_dir(), f"{E2E_DIR} does not exist"
    stems: dict[str, str] = {}
    for path in E2E_DIR.glob("*.md"):
        m = E2E_FILENAME_RE.match(path.name)
        assert m, f"{path.name} does not match '<n>-<slug>.md'"
        n = m.group(1)
        assert n not in stems, f"duplicate checkpoint number {n} in tests/e2e/"
        stems[n] = path.stem
    return stems


def test_checkpoint_count_matches_plan() -> None:
    """The plan names ~20 checkpoints; guard against silent drift either way."""
    checkpoints = _checkpoints_from_spec()
    assert len(checkpoints) == 20, (
        f"expected 20 checkpoints per the v0.5.0 archive-rebuild plan, "
        f"found {len(checkpoints)}"
    )


def test_every_checkpoint_has_an_e2e_file() -> None:
    checkpoints = _checkpoints_from_spec()
    e2e_stems = _e2e_file_stems()

    missing = {
        n: stem for n, stem in checkpoints.items() if n not in e2e_stems
    }
    assert not missing, (
        "checkpoints with no tests/e2e/<n>-<slug>.md file: "
        f"{sorted(missing.values())}"
    )

    mismatched = {
        n: (stem, e2e_stems[n])
        for n, stem in checkpoints.items()
        if n in e2e_stems and e2e_stems[n] != stem
    }
    assert not mismatched, (
        "checkpoint slug does not match its e2e filename (spec title "
        f"changed without renaming the e2e file?): {mismatched}"
    )


def test_every_e2e_file_has_a_checkpoint() -> None:
    checkpoints = _checkpoints_from_spec()
    e2e_stems = _e2e_file_stems()

    orphaned = {n: stem for n, stem in e2e_stems.items() if n not in checkpoints}
    assert not orphaned, (
        "tests/e2e/ files with no matching checkpoint in "
        f"userflow-checkpoints.md (stale after a spec edit?): {sorted(orphaned.values())}"
    )


def test_checkpoint_set_equals_e2e_file_set() -> None:
    """The headline invariant: the two sets match 1:1, by number."""
    checkpoints = _checkpoints_from_spec()
    e2e_stems = _e2e_file_stems()
    assert set(checkpoints) == set(e2e_stems)
