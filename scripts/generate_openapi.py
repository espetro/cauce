"""Generates ``openapi.json`` at the repo root from the live FastAPI app.

Run via ``mise run gen:openapi``. This is the single source of the spec:
``ui/``'s TS client/types and ``schemathesis`` both consume the file this
script writes, and ``check:openapi`` (see ``mise.toml``) regenerates it and
diffs against the committed copy to catch drift (day-0 gate 5 in
``.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md``).
"""

import json
from pathlib import Path

from oxe.app import create_app

ROOT = Path(__file__).resolve().parent.parent
OUTPUT_PATH = ROOT / "openapi.json"


def main() -> None:
    app = create_app()
    schema = app.openapi()
    OUTPUT_PATH.write_text(json.dumps(schema, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
