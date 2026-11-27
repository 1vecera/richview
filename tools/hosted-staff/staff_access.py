#!/usr/bin/env python3
"""Issue a verified case grant or redeem it over the local RichView inspection listener."""

import argparse
import fcntl
import http.client
import json
import os
import re
import secrets
import sys
import tempfile
import time
from pathlib import Path


def check_id(value: str, pattern: str) -> str:
    """Validate an identifier before using it in a filename or request."""
    if not re.fullmatch(pattern, value):
        raise ValueError("Invalid identifier")
    return value


def publish_once(path: Path, data: bytes) -> None:
    """Publish a complete record without replacing a previous decision."""
    with tempfile.NamedTemporaryFile(dir=path.parent) as pending:
        os.fchmod(pending.fileno(), 0o640)
        pending.write(data)
        pending.flush()
        os.fsync(pending.fileno())
        os.link(pending.name, path)
        sync_directory(path.parent)


def sync_directory(path: Path) -> None:
    """Flush the directory entry that makes a record effective."""
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def issue_grant(root: Path, record: Path, minutes: int) -> str:
    """Bind one registrar-verified permission or order to an expiring case grant."""
    request = json.loads(record.read_bytes())
    fields = {
        "actor",
        "owner",
        "case",
        "evidence",
        "purpose",
        "basis",
        "sealed",
        "org",
        "slug",
        "version",
        "artifact",
    }
    if (
        not isinstance(request, dict)
        or set(request) != fields
        or not 1 <= minutes <= 60
    ):
        raise ValueError(
            "Supply the documented case fields and a lifetime of 1 to 60 minutes"
        )
    for name in ("actor", "owner", "case", "evidence"):
        check_id(request[name], r"[A-Za-z0-9_-]{1,128}")
    if request["owner"] == "service" or request["basis"] not in {
        "owner-permission",
        "legal-order",
    }:
        raise ValueError(
            "A verified person and a permission or binding order are required"
        )
    if (
        type(request["sealed"]) is not bool
        or request["sealed"]
        and request["basis"] != "legal-order"
    ):
        raise ValueError("Only a binding order may restrict disclosure")
    if (
        not isinstance(request["purpose"], str)
        or not request["purpose"].strip()
        or len(request["purpose"].encode()) > 240
    ):
        raise ValueError("Supply a short purpose the customer can read")
    check_id(request["org"], r"[a-z0-9]{8}")
    check_id(request["slug"], r"[a-z0-9]+(?:-[a-z0-9]+)*")
    check_id(
        request["version"],
        r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?",
    )
    if not request["purpose"].isprintable():
        raise ValueError("Use plain text for the purpose")
    target = request["artifact"]
    if not isinstance(target, dict):
        raise TypeError("Supply one artifact")
    if target.get("kind") in ("data", "file"):
        if (
            set(target) != {"kind", "path"}
            or not isinstance(target["path"], str)
            or not target["path"]
        ):
            raise ValueError("Name the exact report-relative file or data entry")
        path = target["path"]
        if (
            any(part in ("", "..", ".") for part in path.split("/"))
            or any(char in path for char in "\\:?#%")
            or not path.isprintable()
        ):
            raise ValueError("Use a report-relative artifact path")
    elif set(target) != {"kind"} or target["kind"] not in ("source", "html"):
        raise ValueError("Unknown artifact")
    grant = secrets.token_hex(16)
    issued = int(time.time())
    publish_once(
        root / "grants" / grant,
        json.dumps(
            dict(request, issued=issued, expires=issued + minutes * 60)
        ).encode(),
    )
    return grant


def read_artifact(grant: str, port: int, credential: Path, destination: Path) -> None:
    """Save one complete response privately without overwriting a local file."""
    check_id(grant, r"[a-f0-9]{32}")
    if credential.stat().st_mode & 0o077:
        raise ValueError("Restrict the personal credential file to its owner")
    token = check_id(credential.read_text().strip(), r"[a-f0-9]{64}")
    if destination.exists():
        raise ValueError("Choose a new output file before using the grant")
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=60)
    try:
        with tempfile.NamedTemporaryFile(dir=destination.parent) as pending:
            connection.request(
                "POST",
                "/staff/inspections/" + grant,
                headers={"Authorization": "Bearer " + token},
            )
            response = connection.getresponse()
            if (
                response.status != 200
                or response.getheader("Content-Type") != "application/octet-stream"
            ):
                raise ValueError(
                    f"Inspection refused or unavailable (HTTP {response.status})"
                )
            expected = int(response.getheader("Content-Length") or "-1")
            while block := response.read(65536):
                pending.write(block)
            if pending.tell() != expected:
                raise ValueError(
                    "Incomplete response. Check the case history before requesting another grant"
                )
            pending.flush()
            os.fsync(pending.fileno())
            os.link(pending.name, destination)
            sync_directory(destination.parent)
    finally:
        connection.close()


def main() -> None:
    """Run the inspector client or a separate registrar filesystem operation."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("/var/lib/richview-staff"))
    commands = parser.add_subparsers(dest="command", required=True)
    enroll = commands.add_parser("enroll")
    enroll.add_argument("actor")
    enroll.add_argument("fingerprint")
    disable = commands.add_parser("disable")
    disable.add_argument("fingerprint")
    issue = commands.add_parser("issue")
    issue.add_argument("record", type=Path)
    issue.add_argument("--minutes", type=int, default=15)
    for name in ("revoke", "disclose"):
        commands.add_parser(name).add_argument("grant")
    read = commands.add_parser("read")
    read.add_argument("grant")
    read.add_argument("--credential", type=Path, required=True)
    read.add_argument("--out", type=Path, required=True)
    read.add_argument("--port", type=int, default=8083)
    args = parser.parse_args()
    if args.command == "read":
        read_artifact(args.grant, args.port, args.credential, args.out)
        return
    root = args.root
    with (root / "authority.lock").open("rb") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if args.command in ("enroll", "disable"):
            path = root / "identities" / check_id(args.fingerprint, r"[a-f0-9]{64}")
            if args.command == "enroll":
                publish_once(
                    path, check_id(args.actor, r"[A-Za-z0-9_-]{1,128}").encode()
                )
            else:
                path.unlink()
                sync_directory(path.parent)
        elif args.command == "issue":
            print(issue_grant(root, args.record, args.minutes))
        else:
            grant = check_id(args.grant, r"[a-f0-9]{32}")
            record = json.loads((root / "grants" / grant).read_bytes())
            if args.command == "disclose" and not (
                record["sealed"] and record["basis"] == "legal-order"
            ):
                raise ValueError(
                    "Only a withheld legal order needs disclosure clearance"
                )
            folder = "disclosed" if args.command == "disclose" else "revoked"
            publish_once(root / folder / grant, b"confirmed\n")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, TypeError, http.client.HTTPException) as error:
        sys.exit(str(error))
