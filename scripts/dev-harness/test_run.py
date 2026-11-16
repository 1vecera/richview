"""Failure-oriented checks for evidence identity, retention and owned process cleanup."""

import hashlib
import importlib.util
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("harness", Path(__file__).with_name("run.py"))
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


class HarnessTests(unittest.TestCase):
    def test_direct_harness_stages_preview_then_builds_and_checks_embedded_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory).resolve()
            root, target, evidence = base / "checkout", base / "shared", base / "evidence"
            root.mkdir()
            target.mkdir()
            for path in harness.SUITES["site"]:
                script = root / "scripts" / path
                script.parent.mkdir(parents=True, exist_ok=True)
                script.write_text("")
            args = SimpleNamespace(evidence=evidence, suite="site", browser="chromium", check=[], drill=False)
            source = {"head": "a" * 40, "files": {}, "sha256": "source"}
            calls = []

            def check_output(argv, **kwargs):
                if argv[:2] == ["cargo", "metadata"]:
                    return json.dumps({"workspace_members": [], "packages": []})
                return "/fake/sysroot" if argv[:2] == ["rustc", "--print"] else "version"

            def execute(argv, cwd, env, output, timeout):
                calls.append((argv, env.copy()))
                if argv[:2] == ["cargo", "build"] and "--message-format=json" in argv:
                    names = ("richview", "richview-sql-worker", "richview-editor")
                    if "--features" not in argv:
                        names += ("richview-build", "richview-bench")
                    for name in names:
                        output.write(
                            json.dumps(
                                {
                                    "reason": "compiler-artifact",
                                    "manifest_path": str(root / "Cargo.toml"),
                                    "executable": str(target / "release" / name),
                                    "fresh": False,
                                }
                            )
                            + "\n"
                        )
                if argv[:2] == ["cargo", "run"]:
                    output.write(f"Browser + offline previews: {root / 'dist/web'}\n")
                return 0

            with (
                patch.object(harness, "ROOT", root),
                patch.object(harness.Path, "home", return_value=base / "fake-home"),
                patch.dict(
                    os.environ,
                    {"CARGO_TARGET_DIR": str(target), "RV_CARGO_LOCK_HELD": "1", "RV_GATE_HOLDER": "123"},
                    clear=False,
                ),
                patch.object(harness, "git", return_value=str(root / ".git")),
                patch.object(harness, "source_snapshot", return_value=source),
                patch.object(harness, "invalidate_workspace", return_value=[]),
                patch.object(
                    harness,
                    "artifact_owner",
                    side_effect=lambda binary, entry, target, git_head: {"binary": str(binary), "entrypoint": entry},
                ),
                patch.object(harness, "artifacts", return_value={"identity": "fixed"}),
                patch.object(harness, "_preview_manifest", return_value={"mode": "local-preview"}),
                patch.object(harness, "_embedded_runtime", return_value="verified"),
                patch.object(harness.platform, "platform", return_value="test-host"),
                patch.object(harness.subprocess, "check_output", side_effect=check_output),
                patch.object(harness, "execute", side_effect=execute),
            ):
                self.assertEqual(harness.run_harness(args), 0)
            receipt = json.loads((evidence / "run.json").read_text())
            self.assertEqual(receipt["status"], "passed")
            self.assertEqual(receipt["build_mode"], "local-preview")
            self.assertTrue(all("RV_CARGO_LOCK_HELD" not in env and "RV_GATE_HOLDER" not in env for _, env in calls))
            commands = [argv for argv, _ in calls]
            stages = [(index, argv) for index, argv in enumerate(commands) if argv[:2] == ["cargo", "run"]]
            self.assertEqual(len(stages), 2)
            self.assertTrue(all(argv[-2:] == ["--assets-only", "--local-preview"] for _, argv in stages))
            embedded = [
                argv for argv in commands if "richview-app/embedded-runtime,richview-editor/embedded-runtime" in argv
            ]
            self.assertEqual(len(embedded), 1)
            self.assertLess(stages[0][0], commands.index(embedded[0]))
            self.assertLess(commands.index(embedded[0]), stages[1][0])
            self.assertTrue(any("wasm-site" in argv for argv in commands))
            self.assertTrue(any("wasm-release" in argv for argv in commands))
            self.assertTrue(
                any(
                    argv[:2] == ["node", str(root / "scripts/verify-site.mjs")] and env["RICHVIEW_DEV_PREVIEW"] == "1"
                    for argv, env in calls
                )
            )
            self.assertTrue(any(argv[0].endswith("richview-bench") and "--write-fixtures" in argv for argv in commands))

    def test_suite_verifiers_exist(self):
        for suite, paths in harness.SUITES.items():
            for path in paths:
                with self.subTest(suite=suite, path=path):
                    self.assertTrue((harness.ROOT / "scripts" / path).is_file(), path)

    def test_native_entrypoints_belong_to_current_workspace(self):
        for binary, entrypoint in harness.NATIVE_ENTRYPOINTS.items():
            with self.subTest(binary=binary):
                self.assertTrue((harness.ROOT / entrypoint).is_file(), entrypoint)

    def test_preview_manifest_requires_local_preview_and_the_current_commit(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            path = target / "release/richview-build-manifest.json"
            path.parent.mkdir()
            record = {"schema": 1, "mode": "local-preview", "inputs": {"commit": "current"}}
            path.write_text(json.dumps(record))
            self.assertEqual(harness._preview_manifest(target, "current")["mode"], "local-preview")
            with self.assertRaisesRegex(RuntimeError, "another source commit"):
                harness._preview_manifest(target, "different")
            for mode in ("full", "assets"):
                record["mode"] = mode
                path.write_text(json.dumps(record))
                with self.assertRaisesRegex(RuntimeError, "local-preview manifest"):
                    harness._preview_manifest(target, "current")

    def test_embedded_runtime_must_match_the_staged_bytes_and_commit(self):
        with tempfile.TemporaryDirectory() as directory:
            public = Path(directory)
            (public / "richview_runtime.js").write_bytes(b"js")
            (public / "richview_runtime_bg.wasm.gz").write_bytes(b"wasm")
            runtime = hashlib.sha256(b"jswasm").hexdigest()[:12]
            version = f"richview (commit 1234567890, runtime {runtime})"
            with patch.object(harness.subprocess, "check_output", return_value=version):
                self.assertEqual(harness._embedded_runtime(public / "richview", public, "1234567890abc", {}), version)
                with self.assertRaisesRegex(RuntimeError, "does not embed"):
                    harness._embedded_runtime(public / "richview", public, "different", {})
                (public / "richview_runtime.js").write_bytes(b"changed")
                with self.assertRaisesRegex(RuntimeError, "does not embed"):
                    harness._embedded_runtime(public / "richview", public, "1234567890abc", {})

    def test_compiler_records_must_belong_to_checkout_and_cover_required_binaries(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "build.log"
            record = {
                "reason": "compiler-artifact",
                "manifest_path": str(root / "Cargo.toml"),
                "executable": str(root / "richview"),
                "fresh": False,
            }
            log.write_text(json.dumps(record) + "\n")
            with patch.object(harness, "ROOT", root):
                self.assertEqual(len(harness._compiler_artifacts(log, {"richview"})), 1)
                with self.assertRaisesRegex(RuntimeError, "all native executables"):
                    harness._compiler_artifacts(log, {"richview", "richview-editor"})
                record["manifest_path"] = str(root.parent / "foreign/Cargo.toml")
                log.write_text(json.dumps(record) + "\n")
                with self.assertRaisesRegex(RuntimeError, "another worktree"):
                    harness._compiler_artifacts(log, {"richview"})

    def test_foreign_artifact_is_rejected_even_with_a_local_entrypoint(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.object(harness, "ROOT", Path(directory).resolve() / "own"),
        ):
            root = harness.ROOT
            (root / "src").mkdir(parents=True)
            source = root / "src/main.rs"
            source.write_text("local")
            binary = Path(directory) / "app"
            binary.write_bytes(b"binary")
            depinfo = binary.with_suffix(".d")
            depinfo.write_text(f"{binary}: {source} {Path(directory) / 'peer/src/lib.rs'}\n")
            with self.assertRaisesRegex(RuntimeError, "another source tree"):
                harness.artifact_owner(binary, "src/main.rs")
            depinfo.write_text(f"{binary}: {source}\n")
            self.assertEqual(harness.artifact_owner(binary, "src/main.rs")["source_dependencies"], 1)

    def test_artifact_owner_allows_only_this_git_head_and_known_generated_cargo_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory).resolve()
            root, target = base / "own", base / "target"
            source, git_head = root / "src/main.rs", base / "git/own/HEAD"
            generated = [
                target / "release/build/richview-app-0123456789abcdef/out/runtime_bytes.rs",
                target / "release/build/richview-editor-fedcba9876543210/out/doc_examples.rs",
            ]
            root.mkdir()
            for path, anchor in zip(generated, (git_head, root / "docs/public/charts"), strict=True):
                path.parent.mkdir(parents=True)
                (path.parent.parent / "output").write_text(f"cargo:rerun-if-changed={anchor}\n")
            binary = base / "app"
            binary.write_bytes(b"binary")
            depinfo = binary.with_suffix(".d")
            with patch.object(harness, "ROOT", root):
                depinfo.write_text(f"{binary}: {source} {git_head} {' '.join(map(str, generated))}\n")
                self.assertEqual(
                    harness.artifact_owner(binary, "src/main.rs", target, git_head)["source_dependencies"], 4
                )
                for foreign in (
                    base / "git/peer/HEAD",
                    target / "release/build/richview-app-0123456789abcdef/out/other.rs",
                    target / "release/build/other-0123456789abcdef/out/runtime_bytes.rs",
                    base / "peer/src/lib.rs",
                ):
                    depinfo.write_text(f"{binary}: {source} {foreign}\n")
                    with self.assertRaisesRegex(RuntimeError, "another source tree"):
                        harness.artifact_owner(binary, "src/main.rs", target, git_head)
                for path in generated:
                    output = path.parent.parent / "output"
                    original = output.read_text()
                    for invalid in (
                        "",
                        f"cargo:rerun-if-changed={base / 'peer/src'}\n",
                        original + f"cargo:rerun-if-changed={base / 'peer/src'}\n",
                    ):
                        output.write_text(invalid)
                        depinfo.write_text(f"{binary}: {source} {path}\n")
                        with self.assertRaisesRegex(RuntimeError, "another source tree"):
                            harness.artifact_owner(binary, "src/main.rs", target, git_head)
                    output.write_text(original)

    def test_invalidation_changes_only_workspace_entrypoint_mtime(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.object(harness, "ROOT", Path(directory).resolve() / "own"),
        ):
            root = harness.ROOT
            root.mkdir()
            entry, peer = root / "main.rs", Path(directory) / "peer.rs"
            for path in (entry, peer):
                path.write_text("unchanged bytes")
                os.utime(path, (1, 1))
            before = harness.digest(entry)
            metadata = {
                "workspace_members": ["own"],
                "packages": [
                    {"id": "own", "targets": [{"src_path": str(entry)}]},
                    {"id": "dependency", "targets": [{"src_path": str(peer)}]},
                ],
            }
            self.assertEqual(harness.invalidate_workspace(metadata), ["main.rs"])
            self.assertEqual(harness.digest(entry), before)
            self.assertGreater(entry.stat().st_mtime_ns, 1_000_000_000)
            self.assertEqual(peer.stat().st_mtime_ns, 1_000_000_000)
            metadata["workspace_members"].append("dependency")
            with self.assertRaisesRegex(RuntimeError, "this checkout"):
                harness.invalidate_workspace(metadata)

    def test_source_identity_includes_dirty_untracked_and_deleted_source(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(harness, "ROOT", Path(directory)):
            root = Path(directory)

            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)

            git("init", "--quiet")
            (root / "source.rmd").write_text("original")
            (root / ".gitignore").write_text("tmp/\n")
            git("add", ".")
            git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "fixture")
            before = harness.source_snapshot()
            (root / "tmp").mkdir()
            (root / "tmp/ignored.json").write_text("generated")
            harness.verify_identity(before, harness.source_snapshot(), "Source")
            for action in (
                lambda: (root / "source.rmd").write_text("changed"),
                lambda: (root / "new.rmd").write_text("untracked"),
                lambda: (root / "source.rmd").unlink(),
            ):
                action()
                after = harness.source_snapshot()
                self.assertEqual(before["head"], after["head"])
                with self.assertRaisesRegex(RuntimeError, "Source changed"):
                    harness.verify_identity(before, after, "Source")
                before = after

    def test_retention_excludes_old_results_bundles_and_stores(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(harness, "ROOT", Path(directory)):
            root = Path(directory)
            folder = root / "tmp/verification/journey-one"
            folder.mkdir(parents=True)
            (folder / "old.json").write_text("old")
            os.utime(folder / "old.json", (1, 1))
            started = time.time_ns()
            (folder / "result.json").write_text('{"status":"failed"}')
            (folder / "report.html").write_text("offline bundle")
            (folder / "store").mkdir()
            (folder / "store/private.json").write_text("private report")
            with (folder / "oversized.png").open("wb") as stream:
                stream.truncate(8 * 1024**2 + 1)
            destination = root / "retained"
            rows = harness.retain_evidence(destination, started)
            self.assertEqual([row["path"] for row in rows], ["captures/journey-one/result.json"])
            self.assertEqual((destination / rows[0]["path"]).read_text(), '{"status":"failed"}')

    def test_success_reaps_leftover_helper_even_if_it_ignores_term(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            helper = "import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); print('ready',flush=True); time.sleep(60)"
            parent = (
                "import subprocess,sys; "
                f"p=subprocess.Popen([sys.executable,'-c',{helper!r}],stdout=subprocess.PIPE); "
                "p.stdout.readline(); print(p.pid,flush=True)"
            )
            log = root / "process.log"
            with log.open("w") as output:
                code = harness.execute([sys.executable, "-c", parent], root, os.environ, output, 10)
            self.assertEqual(code, 0)
            pid = int(log.read_text().strip())
            state = subprocess.run(
                ["ps", "-p", str(pid), "-o", "stat="], check=False, capture_output=True, text=True
            ).stdout.strip()
            self.assertTrue(not state or state.startswith("Z"), state)

    def test_timeout_is_preserved_after_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (root / "timeout.log").open("w") as output, self.assertRaises(subprocess.TimeoutExpired):
                harness.execute([sys.executable, "-c", "import time; time.sleep(60)"], root, os.environ, output, 0.1)

    def test_denied_group_probe_does_not_mask_timeout_after_group_exits(self):
        actual_signal = os.killpg

        def denied_probe(group, sig):
            if sig == 0:
                raise PermissionError("probe denied")
            return actual_signal(group, sig)

        with tempfile.TemporaryDirectory() as directory, patch.object(harness.os, "killpg", denied_probe):
            root = Path(directory)
            with (root / "timeout.log").open("w") as output, self.assertRaises(subprocess.TimeoutExpired):
                harness.execute([sys.executable, "-c", "import time; time.sleep(60)"], root, os.environ, output, 0.1)

    def test_surviving_group_signal_denial_is_not_ignored(self):
        with (
            patch.object(harness.os, "killpg", side_effect=PermissionError("signal denied")),
            patch.object(harness.subprocess, "check_output", return_value=" 12345 S\n"),
            self.assertRaises(PermissionError),
        ):
            harness.signal_group(12345, signal.SIGTERM)

    def test_exited_zombie_group_is_not_treated_as_a_running_helper(self):
        child = subprocess.Popen([sys.executable, "-c", "pass"], start_new_session=True)
        try:
            deadline = time.monotonic() + 10
            while True:
                state = subprocess.run(
                    ["ps", "-p", str(child.pid), "-o", "stat="], check=False, capture_output=True, text=True
                ).stdout.strip()
                if state.startswith("Z"):
                    break
                self.assertLess(time.monotonic(), deadline, "Child did not reach an exited zombie state")
                time.sleep(0.02)
            self.assertFalse(harness.group_exists(child.pid))
            harness.signal_group(child.pid, signal.SIGTERM)
        finally:
            child.wait()


if __name__ == "__main__":
    unittest.main()
