"""Rebuild the committed research snapshot in isolated cwd/tmp with network connections forbidden.

Run: uv run verify_snapshot.py. The original source, ledger and data remain untouched.
"""

import hashlib
import json
import runpy
import shutil
import socket
import sys
import tempfile
from pathlib import Path


def blocked_connection(*_args: object, **_kwargs: object) -> None:
    """The snapshot verification must never refresh an external source."""
    raise AssertionError("Snapshot regeneration attempted a network connection")


def main() -> None:
    """Check source bytes and portable JSON semantics against the committed publication."""
    original = Path(__file__).resolve().parent.parent
    scratch = Path.cwd() / "tmp"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="snapshot-", dir=scratch) as temporary:
        rebuilt = Path(temporary) / "report"
        shutil.copytree(original, rebuilt, ignore=shutil.ignore_patterns(".venv", "__pycache__", "tmp"))
        sys.path.insert(0, str(rebuilt / "research"))
        # Optional collectors are deliberately absent from this execution path.
        socket.socket.connect = blocked_connection
        socket.socket.connect_ex = blocked_connection
        for script in ("analyze", "build_ledger", "build_report", "build_cover", "verify_images"):
            sys.argv = [str(rebuilt / "research" / f"{script}.py")]
            runpy.run_path(sys.argv[0], run_name="__main__")
        checks = {}
        for relative in ("report.rmd", "ledger/sources.csv", "assets/cover-two-publics.svg"):
            expected = (original / relative).read_bytes()
            actual = (rebuilt / relative).read_bytes()
            if expected != actual:
                raise AssertionError(f"Snapshot changed: {relative}")
            checks[relative] = {"identical": True, "bytes": len(actual), "sha256": hashlib.sha256(actual).hexdigest()}
        for relative in ("content/report.json", "content/citations.json", "data/facts.json", "data/charts/index.json"):
            if json.loads((original / relative).read_text()) != json.loads((rebuilt / relative).read_text()):
                raise AssertionError(f"Portable content changed: {relative}")
            checks[relative] = {"semantic_identical": True}
        evidence = {"network": "socket.connect/connect_ex forbidden", "checks": checks}
        (scratch / "snapshot-verification.json").write_text(json.dumps(evidence, indent=2) + "\n")
        print(json.dumps(evidence, indent=2))


if __name__ == "__main__":
    main()
