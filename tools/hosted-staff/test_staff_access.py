"""Check local grant issuance and private inspection output with synthetic records."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import staff_access


class StaffAccessTest(unittest.TestCase):
    def test_case_record_is_complete_once_and_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "grants").mkdir()
            record = root / "permission.json"
            grant = {
                "actor": "staff-a",
                "case": "case-1",
                "evidence": "permission-1",
                "owner": "owner",
                "purpose": "Read the synthetic source",
                "basis": "owner-permission",
                "sealed": False,
                "org": "aaaa0001",
                "slug": "sample",
                "version": "1.0.0",
                "artifact": {"kind": "source"},
            }
            record.write_text(json.dumps(grant))
            grant_id = staff_access.issue_grant(root, record, 15)
            stored = json.loads((root / "grants" / grant_id).read_bytes())
            self.assertEqual(stored["expires"] - stored["issued"], 900)
            self.assertEqual(stored["artifact"], {"kind": "source"})
            with self.assertRaises(FileExistsError):
                staff_access.publish_once(root / "grants" / grant_id, b"replacement")
            grant["sealed"] = True
            record.write_text(json.dumps(grant))
            with self.assertRaises(ValueError):
                staff_access.issue_grant(root, record, 15)
            with self.assertRaises(ValueError):
                staff_access.issue_grant(root, record, 61)

    def test_inspection_saves_only_a_complete_response_to_a_new_private_file(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            credential, output = root / "person", root / "output"
            credential.write_text("a" * 64)
            credential.chmod(0o600)
            response = Mock(status=200)
            response.getheader.side_effect = lambda name: {
                "Content-Type": "application/octet-stream",
                "Content-Length": "7",
            }[name]
            with patch.object(staff_access.http.client, "HTTPConnection") as connect:
                connect.return_value.getresponse.return_value = response
                response.read.side_effect = [b"short", b""]
                with self.assertRaises(ValueError):
                    staff_access.read_artifact("a" * 32, 8083, credential, output)
                self.assertFalse(output.exists())
                response.read.side_effect = [b"example", b""]
                staff_access.read_artifact("a" * 32, 8083, credential, output)
                self.assertEqual(output.stat().st_mode & 0o777, 0o600)
                self.assertEqual(output.read_bytes(), b"example")
                with self.assertRaises(ValueError):
                    staff_access.read_artifact("a" * 32, 8083, credential, output)
                self.assertEqual(connect.call_count, 2)
                connect.assert_called_with("127.0.0.1", 8083, timeout=60)
