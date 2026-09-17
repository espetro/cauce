"""Every `[gate: X]` tag in an AGENTS.md must resolve to something real.

X is either a key under `[tasks.*]` in mise.toml, or an existing file path
relative to the repo root. See AGENTS.md's "Enforced" rule for why this exists.
"""

import re
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parent.parent
GATE_RE = re.compile(r"\[gate:\s*([^\]]+?)\s*\]")


def test_all_agents_md_gates_resolve() -> None:
    mise_tasks = tomllib.loads((ROOT / "mise.toml").read_text())["tasks"]
    agents_files = sorted(ROOT.rglob("AGENTS.md"))
    assert agents_files, "expected at least one AGENTS.md in the repo"

    unresolved: list[str] = []
    for agents_md in agents_files:
        for gate in GATE_RE.findall(agents_md.read_text()):
            if gate in mise_tasks or (ROOT / gate).exists():
                continue
            unresolved.append(f"{agents_md.relative_to(ROOT)}: [gate: {gate}]")

    assert not unresolved, "unresolved gate tags:\n" + "\n".join(unresolved)
