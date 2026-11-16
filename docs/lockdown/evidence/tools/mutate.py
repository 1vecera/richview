"""Apply or restore one deliberate mutation in the isolated source copy.

Usage: uv run mutate.py apply <id> | restore <id> | check
Each mutation replaces exactly one known snippet; restore puts the original bytes back
and `check` verifies every touched file matches the b4db233 archive byte for byte.

This is the script DEL-26 cites, as it ran on Claw Mini on 2026-10-21 from the lockdown
worktree's tmp/lockdown-delivery/ folder. COPY and WORKTREE resolve relative to that folder:
the isolated source copy sat in mut/src beside the script, with its own Cargo target, and the
repository two levels up. To rerun it, copy it back into such a folder. The per-tier logs of
that run stayed local and are not committed.
"""

import hashlib
import subprocess
import sys
from pathlib import Path

COPY = Path(__file__).resolve().parent / "mut" / "src"
WORKTREE = Path(__file__).resolve().parents[2]

MUTATIONS = {
    "M1": (
        "crates/richview-core/src/lib.rs",
        "pub const MAX_REPORT_BYTES: usize = 64 * 1024 * 1024;",
        "pub const MAX_REPORT_BYTES: usize = usize::MAX / 4;",
    ),
    "M2": (
        "crates/richview-app/src/asset_upload/upload.rs",
        "let missing: Vec<u16> = (1..=plan.parts).filter(|n| !done.contains(n)).collect();",
        "let missing: Vec<u16> = (1..=plan.parts).collect();",
    ),
    "M3": (
        "richview-chart/src/rmd_render.rs",
        '"0.0%" if *value > 0.0 => "<0.1%".into(),',
        '"0.0%" if *value > 0.0 => share,',
    ),
    "M4": (
        "crates/richview-core/src/parser.rs",
        'if fm.contains_key("data") {',
        'if fm.contains_key("data") && false {',
    ),
}


def archived(path: str) -> bytes:
    return subprocess.run(
        ["git", "-C", str(WORKTREE), "show", f"b4db233:{path}"],
        check=True,
        capture_output=True,
    ).stdout


def main() -> None:
    action = sys.argv[1]
    if action == "check":
        for path, _, _ in MUTATIONS.values():
            same = (COPY / path).read_bytes() == archived(path)
            print(f"{path}: {'identical' if same else 'DIFFERENT'}")
        return
    path, original, mutated = MUTATIONS[sys.argv[2]]
    file = COPY / path
    text = file.read_text()
    before, after = (original, mutated) if action == "apply" else (mutated, original)
    if text.count(before) != 1:
        raise SystemExit(f"{path}: expected one occurrence of the snippet, found {text.count(before)}")
    file.write_text(text.replace(before, after))
    digest = hashlib.sha256(file.read_bytes()).hexdigest()[:12]
    print(f"{action} {sys.argv[2]} {path} sha256={digest}")


main()
