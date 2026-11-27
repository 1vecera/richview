#!/usr/bin/env python3
# /// script
# requires-python = ">=3.13"
# ///
"""Build and check one source snapshot through the shared Cargo shim."""

import argparse
import hashlib
import json
import os
import platform
import re
import shlex
import shutil
import signal
import subprocess
import sys
import time
import traceback
from datetime import UTC, datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NATIVE_ENTRYPOINTS = {
    "richview-build": "crates/richview-build/src/main.rs",
    "richview": "crates/richview-app/src/bin/richview.rs",
    "richview-sql-worker": "crates/richview-app/src/bin/richview-sql-worker.rs",
    "richview-editor": "crates/richview-editor/src/main.rs",
}
SUITES = {
    "smoke": ["dev-harness/journey.ts", "verify-report-runtime.ts"],
    "site": ["verify-browser.ts", "verify-site.ts", "verify-docs.ts"],
    "integration": [
        "dev-harness/journey.ts",
        "verify-native.ts",
        "verify-editor.cts",
        "verify-browser.ts",
        "verify-report-runtime.ts",
        "verify-site.ts",
    ],
}


def digest(path):
    """Hash file bytes without loading native binaries into memory."""
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT).decode().strip()


def source_snapshot():
    """Include uncommitted source; a commit ID alone cannot identify a worktree build."""
    paths = (
        subprocess.check_output(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT)
        .decode()
        .split("\0")
    )
    files = {}
    for name in sorted(set(paths) - {""}):
        path = ROOT / name
        if path.is_symlink():
            files[name] = "symlink:" + os.readlink(path)
        else:
            files[name] = digest(path) if path.exists() else "deleted"
    return {
        "head": git("rev-parse", "HEAD"),
        "branch": git("branch", "--show-current"),
        "files": files,
        "sha256": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
    }


def artifacts(target, native_names=NATIVE_ENTRYPOINTS):
    """Cover every staged host surface, plus the native binaries actually executed."""
    paths = [target / "release" / name for name in native_names]
    paths.append(target / "release/richview-build-manifest.json")
    for directory in ("dist/web", "packages/vscode/bin"):
        paths.extend(path for path in (ROOT / directory).rglob("*") if path.is_file())
    required = ROOT / "dist/web/richview_site_bg.wasm"
    if not required.is_file():
        raise RuntimeError("Build did not stage the browser runtime in this worktree")
    return {str(path): {"bytes": path.stat().st_size, "sha256": digest(path)} for path in sorted(set(paths))}


def verify_identity(before, after, label):
    if before != after:
        raise RuntimeError(
            f"{label} changed during verification; discard this result and rebuild under the shared lock"
        )


def _preview_manifest(target: Path, source_head: str) -> dict[str, str]:
    """Verify the local preview mode and source commit in the builder's manifest."""
    path = target / "release/richview-build-manifest.json"
    record = json.loads(path.read_text())
    if record.get("schema") != 1 or record.get("mode") != "local-preview":
        raise RuntimeError("Builder did not produce a local-preview manifest")
    if record.get("inputs", {}).get("commit") != source_head:
        raise RuntimeError("Local-preview manifest belongs to another source commit")
    return {"path": str(path), "sha256": digest(path), "mode": record["mode"]}


def _compiler_artifacts(log: Path, required: set[str]) -> list[dict[str, object]]:
    """Check Cargo's executable records against the checkout and required names."""
    records = []
    for line in log.read_text().splitlines():
        if not line.startswith("{"):
            continue
        record = json.loads(line)
        if record.get("reason") == "compiler-artifact" and record.get("executable"):
            if not Path(record["manifest_path"]).is_relative_to(ROOT):
                raise RuntimeError("Native executable was selected from another worktree")
            records.append({key: record[key] for key in ("manifest_path", "executable", "fresh")})
    if not required.issubset({Path(record["executable"]).name for record in records}):
        raise RuntimeError("Cargo did not identify all native executables")
    return records


def _embedded_runtime(binary: Path, public: Path, source_head: str, env: dict[str, str]) -> str:
    """Verify the native CLI names this commit and the staged embedded runtime."""
    hasher = hashlib.sha256()
    for name in ("richview_runtime.js", "richview_runtime_bg.wasm.gz"):
        hasher.update((public / name).read_bytes())
    version = subprocess.check_output([str(binary), "--version"], cwd=ROOT, env=env, text=True).strip()
    if f"(commit {source_head[:10]}, runtime {hasher.hexdigest()[:12]})" not in version:
        raise RuntimeError(f"Native CLI does not embed this checkout's staged runtime: {version}")
    return version


def invalidate_workspace(metadata):
    """Rebuild local targets when Cargo's shared mtime cache belongs to another checkout.

    Touch only this workspace's target entrypoints, not dependencies or peer files.
    Cargo otherwise considers older checkout bytes fresh against newer shared outputs.
    """
    members = set(metadata["workspace_members"])
    paths = {
        Path(target["src_path"]).resolve(strict=True)
        for package in metadata["packages"]
        if package["id"] in members
        for target in package["targets"]
    }
    if not paths or any(not path.is_relative_to(ROOT) for path in paths):
        raise RuntimeError("Workspace target entrypoints must belong to this checkout")
    for path in paths:
        path.touch()
    return sorted(str(path.relative_to(ROOT)) for path in paths)


def artifact_owner(binary, entrypoint, target=None, git_head=None):
    """Reject foreign dep-info before executing an artifact with a compiled-in root."""
    depinfo = binary.with_suffix(".d")
    line = depinfo.read_text().splitlines()[0]
    dependencies = [Path(path).resolve() for path in shlex.split(line.split(": ", 1)[1])]

    def owned(path):
        if path.is_relative_to(ROOT) or (git_head is not None and path == git_head.resolve()):
            return True
        if target is None or not path.is_relative_to(target / "release/build"):
            return False
        parts = path.relative_to(target / "release/build").parts
        if len(parts) != 3 or parts[1] != "out" or git_head is None:
            return False
        for crate, name in (("app", "runtime_bytes.rs"), ("editor", "doc_examples.rs")):
            if not re.fullmatch(rf"richview-{crate}-[0-9a-f]{{16}}", parts[0]) or parts[2] != name:
                continue
            try:
                lines = (path.parent.parent / "output").read_text().splitlines()
            except OSError:
                return False
            prefix = "cargo:rerun-if-changed="
            inputs = []
            for line in lines:
                if line.startswith(prefix):
                    item = Path(line.removeprefix(prefix))
                    inputs.append((item if item.is_absolute() else ROOT / f"crates/richview-{crate}" / item).resolve())
            anchor = git_head.resolve() if crate == "app" else (ROOT / "docs/public/charts").resolve()
            return anchor in inputs and all(item.is_relative_to(ROOT) or item == git_head.resolve() for item in inputs)
        return False

    if ROOT / entrypoint not in dependencies or any(not owned(path) for path in dependencies):
        raise RuntimeError(f"Artifact belongs to another source tree: {binary}; refusing execution")
    return {
        "binary": str(binary),
        "sha256": digest(binary),
        "depinfo_sha256": digest(depinfo),
        "entrypoint": entrypoint,
        "source_dependencies": len(dependencies),
    }


def group_exists(group):
    try:
        os.killpg(group, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        pass
    # macOS denies signals to zombie-only groups. Zombies cannot run or write;
    # waiting for their parent to reap them must not fail an otherwise finished check.
    groups = subprocess.check_output(["ps", "-axo", "pgid=,stat="], text=True).splitlines()
    return any(
        parts[0] == str(group) and not parts[1].startswith("Z") for line in groups if len(parts := line.split()) == 2
    )


def signal_group(group, sig):
    try:
        os.killpg(group, sig)
    except ProcessLookupError:
        pass
    except PermissionError:
        if group_exists(group):
            raise


def execute(argv, cwd, env, output, timeout):
    """Reap the command's process group after it exits or times out."""
    with subprocess.Popen(
        argv, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT, start_new_session=True
    ) as child:
        try:
            return child.wait(timeout=timeout)
        finally:
            # Also reap helpers left behind by a verifier which crashed after starting a server.
            try:
                signal_group(child.pid, signal.SIGTERM)
            except PermissionError as error:
                error.add_note(f"Process group {child.pid}; direct child return code {child.returncode}")
                raise
            deadline = time.monotonic() + 3
            killed = False
            while True:
                child.poll()  # Reap the direct child even if only its helpers remain.
                if not group_exists(child.pid):
                    break
                if time.monotonic() >= deadline:
                    if killed:
                        raise RuntimeError(f"Process group {child.pid} still has live processes after SIGKILL")
                    signal_group(child.pid, signal.SIGKILL)
                    killed = True
                    deadline = time.monotonic() + 3
                time.sleep(0.05)
            child.wait()


def retain_evidence(destination, started_ns):
    """Retain compact verifier outputs, never copied distributions, stores or offline bundles."""
    retained = []
    for folder in (ROOT / "tmp/verification").glob("*"):
        if not folder.is_dir():
            continue
        for path in folder.iterdir():
            if not path.is_file() or path.stat().st_mtime_ns < started_ns:
                continue
            if path.suffix not in (".json", ".png", ".svg") or path.stat().st_size > 8 * 1024**2:
                continue
            relative = path.relative_to(ROOT / "tmp/verification")
            copied = destination / "captures" / relative
            copied.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, copied)
            retained.append({"path": str(copied.relative_to(destination)), "sha256": digest(copied)})
    return retained


def run_harness(args):
    """Build with the shared shim and reject source or artifact drift around checks."""
    target_setting = os.environ.get("CARGO_TARGET_DIR")
    if not target_setting:
        raise RuntimeError("Source the RichView environment to select the shared Cargo target")
    target = Path(target_setting).resolve(strict=True)
    evidence = args.evidence.resolve()
    common = Path(git("rev-parse", "--path-format=absolute", "--git-common-dir")).parent
    if evidence.is_relative_to(common) or evidence.is_relative_to(target):
        raise RuntimeError("Evidence must be outside the repository, worktrees and shared target")
    evidence.mkdir(parents=True, exist_ok=False)
    os.chmod(evidence, 0o700)
    started_ns = time.time_ns()
    receipt = {
        "schema": 1,
        "status": "running",
        "started_at": datetime.now(UTC).isoformat(),
        "root": str(ROOT),
        "target": str(target),
        "suite": args.suite,
        "browser": args.browser,
        "platform": platform.platform(),
        "commands": [],
        "scope": "local preparation; no deployment or merge",
        "build_mode": "local-preview",
    }

    def save():
        temporary = evidence / "run.json.tmp"
        temporary.write_text(json.dumps(receipt, indent=2) + "\n")
        temporary.replace(evidence / "run.json")

    # Endpoint/credential overrides cannot redirect the existing verifiers outside their local stores.
    reserved = {"RV_CARGO_LOCK_HELD", "RV_GATE_HOLDER", "RV_SHARED_TARGET", "RV_DISK_FLOOR_GB"}
    env = {key: value for key, value in os.environ.items() if not key.startswith("RICHVIEW_") and key not in reserved}
    env["RICHVIEW_VERIFY_PUBLIC_DIR"] = str(ROOT / "dist/web")
    env["RICHVIEW_PUBLIC_DIR"] = str(ROOT / "dist/web")
    env["RICHVIEW_VERIFY_BROWSER"] = args.browser
    env["RICHVIEW_DEV_PREVIEW"] = "1"
    scripts = list(dict.fromkeys([*SUITES[args.suite], *args.check]))
    if args.drill and "dev-harness/journey.ts" not in scripts:
        scripts.insert(0, "dev-harness/journey.ts")
    needs_fixtures = "verify-browser.ts" in scripts

    def command(name, argv, expected=0, signal=None):
        if shutil.disk_usage(target).free < 15 * 1024**3:
            raise RuntimeError("Less than 15 GiB free; stop and report headroom, do not clean shared caches")
        log = evidence / f"{name}.log"
        row = {"name": name, "argv": argv, "expected_exit": expected, "status": "running", "log": log.name}
        receipt["commands"].append(row)
        save()
        print(f"RUN {name} ({log})", flush=True)
        begin = time.monotonic()
        with log.open("w") as output:
            try:
                code = execute(argv, ROOT, env, output, timeout=3600 if argv[0] == "cargo" else 900)
                row["exit_code"] = code
                row["status"] = "passed" if code == expected else "failed"
            except subprocess.TimeoutExpired:
                row["status"] = "timed_out"
            except BaseException as error:
                row["status"] = "interrupted" if isinstance(error, KeyboardInterrupt) else "failed"
                row["error"] = f"{type(error).__name__}: {error}"
                raise
            finally:
                row["seconds"] = round(time.monotonic() - begin, 3)
                save()
        if signal and signal not in log.read_text():
            row["status"] = "failed"
            row["missing_failure_signal"] = signal
        save()
        print(f"{row['status'].upper()} {name}: {row['seconds']}s", flush=True)
        return row["status"] == "passed"

    try:
        cargo = shutil.which("cargo", path=env["PATH"])
        if not cargo:
            raise RuntimeError("Cargo is not on PATH; source the RichView environment first")
        shim = Path.home() / "tmp/richview-rewrite/bin/cargo"
        shared_target = Path.home() / "tmp/richview-rewrite/target"
        if shim.is_file() and (Path(cargo).resolve() != shim.resolve() or target != shared_target.resolve()):
            raise RuntimeError("Source the RichView environment to use its Cargo shim and shared target")
        receipt["cargo"] = {"path": str(Path(cargo).resolve()), "sha256": digest(Path(cargo))}
        if sys.platform == "darwin":
            sysroot = subprocess.check_output(["rustc", "--print", "sysroot"], cwd=ROOT, env=env, text=True).strip()
            env["DYLD_FALLBACK_LIBRARY_PATH"] = str(Path(sysroot) / "lib")
        link = ROOT / "target"
        if not link.exists() and not link.is_symlink():
            link.symlink_to(target, target_is_directory=True)
        if link.resolve() != target:
            raise RuntimeError("Worktree target does not point at the shared target; do not replace another cache")
        receipt["source"] = source_snapshot()
        git_head = Path(git("rev-parse", "--absolute-git-dir")) / "HEAD"

        def owner(binary, entrypoint):
            return artifact_owner(binary, entrypoint, target, git_head)

        receipt["tools"] = {
            tool: subprocess.check_output(argv, cwd=ROOT, env=env, text=True).strip()
            for tool, argv in {
                "rustc": ["rustc", "--version"],
                "node": ["node", "--version"],
                "wasm_bindgen": ["wasm-bindgen", "--version"],
            }.items()
        }
        metadata = json.loads(
            subprocess.check_output(
                ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], cwd=ROOT, env=env
            )
        )
        receipt["invalidated_workspace_targets"] = invalidate_workspace(metadata)
        save()
        if args.suite == "integration":
            for name, argv in [
                ("harness-tests", [sys.executable, "scripts/dev-harness/test_run.py"]),
                ("fmt", ["cargo", "fmt", "--all", "--check"]),
                (
                    "clippy-native",
                    ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
                ),
                (
                    "clippy-wasm",
                    [
                        "cargo",
                        "clippy",
                        "-p",
                        "richview-site",
                        "--target",
                        "wasm32-unknown-unknown",
                        "--locked",
                        "--",
                        "-D",
                        "warnings",
                    ],
                ),
                ("rust-tests", ["cargo", "test", "--workspace", "--locked"]),
            ]:
                command(name, argv)
        packages = ["richview-build", "richview-app", "richview-editor", "richview-site"]
        if needs_fixtures:
            packages.append("richview-bench")
        native_command = ["cargo", "build", "--release", "--locked", "--message-format=json"]
        for package in packages:
            native_command.extend(["-p", package])
        if not command("build-native", native_command):
            raise RuntimeError("Native build failed; no leftover artifact will be executed")
        required = set(NATIVE_ENTRYPOINTS)
        if needs_fixtures:
            required.add("richview-bench")
        receipt["bootstrap_artifacts"] = _compiler_artifacts(evidence / "build-native.log", required)
        # Cargo JSON can name this manifest while selecting a stale shared binary.
        bootstrap_owners = [
            owner(target / "release" / name, entrypoint) for name, entrypoint in NATIVE_ENTRYPOINTS.items()
        ]
        wasm_specs = (
            ("wasm-site", "richview-site", "richview_site", "crates/richview-site/src/lib.rs"),
            ("wasm-release", "richview-runtime", "richview_runtime", "crates/richview-runtime/src/lib.rs"),
        )
        wasm_owners = []
        for profile, package, module, entrypoint in wasm_specs:
            if not command(
                f"build-{profile}",
                [
                    "cargo",
                    "build",
                    "--profile",
                    profile,
                    "--locked",
                    "--target",
                    "wasm32-unknown-unknown",
                    "-p",
                    package,
                    "--lib",
                ],
            ):
                raise RuntimeError(f"{profile} build failed")
            wasm_owners.append(owner(target / "wasm32-unknown-unknown" / profile / f"{module}.wasm", entrypoint))

        def verify_wasm():
            current = [
                owner(target / "wasm32-unknown-unknown" / profile / f"{module}.wasm", entrypoint)
                for profile, _, module, entrypoint in wasm_specs
            ]
            verify_identity(wasm_owners, current, "Wasm artifact ownership")

        verify_identity(
            bootstrap_owners,
            [owner(target / "release" / name, entrypoint) for name, entrypoint in NATIVE_ENTRYPOINTS.items()],
            "Bootstrap artifact ownership",
        )
        stage = [
            "cargo",
            "run",
            "--release",
            "--locked",
            "-p",
            "richview-build",
            "--",
            "--assets-only",
            "--local-preview",
        ]
        verify_wasm()
        if not command("stage-bootstrap", stage):
            raise RuntimeError("Bootstrap staging failed; no embedded binary will be built")
        verify_wasm()
        if not command(
            "build-embedded-native",
            [
                "cargo",
                "build",
                "--release",
                "--locked",
                "--message-format=json",
                "-p",
                "richview-app",
                "-p",
                "richview-editor",
                "--features",
                "richview-app/embedded-runtime,richview-editor/embedded-runtime",
            ],
        ):
            raise RuntimeError("Embedded native build failed; no leftover artifact will be executed")
        embedded = {"richview", "richview-sql-worker", "richview-editor"}
        receipt["native_artifacts"] = _compiler_artifacts(evidence / "build-embedded-native.log", embedded)
        embedded_owners = [owner(target / "release" / name, NATIVE_ENTRYPOINTS[name]) for name in sorted(embedded)]
        verify_wasm()
        if not command("stage-assets", stage):
            raise RuntimeError("Asset staging failed; verifiers were not run against leftover assets")
        expected_output = f"Browser + offline previews: {ROOT / 'dist/web'}"
        if expected_output not in (evidence / "stage-assets.log").read_text():
            raise RuntimeError("Builder did not stage this worktree's browser distribution")
        receipt["build_manifest"] = _preview_manifest(target, receipt["source"]["head"])
        verify_identity(
            embedded_owners,
            [owner(target / "release" / name, NATIVE_ENTRYPOINTS[name]) for name in sorted(embedded)],
            "Embedded artifact ownership",
        )
        verify_wasm()
        receipt["artifact_owners"] = [
            owner(target / "release/richview-build", NATIVE_ENTRYPOINTS["richview-build"]),
            *embedded_owners,
            *wasm_owners,
        ]
        receipt["embedded_runtime"] = _embedded_runtime(
            target / "release/richview", ROOT / "dist/web", receipt["source"]["head"], env
        )
        if needs_fixtures:
            bench = owner(target / "release/richview-bench", "crates/richview-bench/src/main.rs")
            receipt["artifact_owners"].append(bench)
            if not command("fixtures", [str(target / "release/richview-bench"), "--write-fixtures", "--fixtures-only"]):
                raise RuntimeError("Browser fixtures were not generated")
            verify_identity(
                bench,
                owner(target / "release/richview-bench", "crates/richview-bench/src/main.rs"),
                "Fixture generator ownership",
            )
        receipt["artifacts"] = artifacts(target, required)
        verify_identity(receipt["source"], source_snapshot(), "Source")
        save()

        def verify_inputs():
            verify_identity(receipt["source"], source_snapshot(), "Source")
            verify_identity(receipt["artifacts"], artifacts(target, required), "Build artifacts")

        if args.drill:
            verify_inputs()
            command(
                "regression-missing-runtime",
                ["node", "scripts/dev-harness/journey.ts", "--disable-runtime"],
                expected=1,
                signal="HARNESS_RUNTIME_MISSING",
            )
            verify_inputs()
        for index, script in enumerate(scripts):
            path = (ROOT / "scripts" / script).resolve(strict=True)
            if not path.is_relative_to(ROOT / "scripts") or path.suffix not in (".mjs", ".cjs", ".ts", ".mts", ".cts"):
                raise RuntimeError("Extra checks must be existing Node verifiers inside scripts/")
            verify_inputs()
            command(f"{index:02d}-{path.stem}", ["node", str(path)])
            verify_inputs()
        verify_inputs()
        receipt["status"] = "passed" if all(row["status"] == "passed" for row in receipt["commands"]) else "failed"
    except (Exception, KeyboardInterrupt) as error:  # noqa: BLE001 — retain the failure and return nonzero.
        receipt["status"] = "failed"
        receipt["error"] = f"{type(error).__name__}: {error}"
        receipt["traceback"] = traceback.format_exc()
        print(receipt["error"], file=sys.stderr)
    finally:
        try:
            receipt["captures"] = retain_evidence(evidence, started_ns)
        except Exception as error:  # noqa: BLE001 — retention failure must make the whole run fail.
            receipt["status"] = "failed"
            receipt["retention_error"] = f"{type(error).__name__}: {error}"
        receipt["finished_at"] = datetime.now(UTC).isoformat()
        save()
    print(f"{receipt['status'].upper()}: {evidence / 'run.json'}", flush=True)
    return 0 if receipt["status"] == "passed" else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path, required=True, help="New directory outside the repository")
    parser.add_argument("--suite", choices=SUITES, default="smoke")
    parser.add_argument("--browser", choices=("chromium", "firefox", "webkit"), default="chromium")
    parser.add_argument(
        "--check", action="append", default=[], help="Additional verifier relative to scripts/; repeatable"
    )
    parser.add_argument(
        "--drill", action="store_true", help="Prove missing-runtime regression fails before the normal journey"
    )
    args = parser.parse_args()
    if not args.evidence.is_absolute():
        parser.error("--evidence must be an absolute path outside the repository")
    os.umask(0o077)

    def interrupted(signum, frame):
        raise KeyboardInterrupt(f"Signal {signum}")

    signal.signal(signal.SIGTERM, interrupted)
    return run_harness(args)


if __name__ == "__main__":
    sys.exit(main())
