"""Regenerate openapi.json at the repo root from the FastAPI app.

Run via `mise run openapi`. Keys are sorted for stable diffs; the committed
file is checked by tests/test_openapi_fresh.py.
"""

import json
from pathlib import Path

from oxe.server import make_app

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "openapi.json"


def main() -> None:
    app = make_app()
    spec = app.openapi()
    OUT.write_text(json.dumps(spec, indent=2, sort_keys=True) + "\n")
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
