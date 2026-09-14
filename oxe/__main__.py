import argparse
import os

import uvicorn

from .server import app


def _default_db() -> str:
    cache_dir = os.getenv("OXE_CACHE_DIR") or os.path.expanduser("~/.cache/oxe")
    return os.path.join(cache_dir, "cache.db")


def _run_stats(args: argparse.Namespace) -> None:
    from .stats import build

    path = build(args.db, args.out, args.days)
    print(path)


def main() -> None:
    parser = argparse.ArgumentParser(prog="oxe")
    sub = parser.add_subparsers(dest="command")
    stats = sub.add_parser("stats", help="build the static stats dashboard")
    stats.add_argument("--db", default=_default_db(), help="path to cache.db")
    stats.add_argument("--out", default="./dist/dashboard", help="output directory")
    stats.add_argument("--days", type=int, default=30, help="stats window in days")
    args = parser.parse_args()
    if args.command == "stats":
        _run_stats(args)
        return
    uvicorn.run(
        app,
        host="127.0.0.1",
        port=int(os.getenv("OXE_PORT", "4479")),
        workers=1,
        log_level=os.getenv("OXE_LOG_LEVEL", "info").lower(),
    )


if __name__ == "__main__":
    main()
