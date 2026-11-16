#!/usr/bin/env python3
# /// script
# requires-python = ">=3.13"
# dependencies = ["psutil==7.2.2; sys_platform == 'darwin'"]
# ///
"""Measure release native surfaces using task-local output and a loopback-only server.

Process-cold CLI samples start a new process every time. OS page cache is warm after
three unmeasured invocations; no privileged cache dropping or hosted calls occur.
HTTP samples include response transfer and expose client decode time separately.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import http.client
import json
import math
import os
import platform
import re
import selectors
import signal
import statistics
import subprocess
import sys
import threading
import time
from datetime import UTC, datetime
from pathlib import Path

if sys.platform == "darwin":
    import psutil

ROOT = Path(__file__).resolve().parents[1]
BENCH = ROOT / "tmp/performance"
CLI = ROOT / "target/release/richview"
SERVER = ROOT / "target/release/richview"
EDITOR = ROOT / "target/release/richview-editor"
PUBLIC = ROOT / "dist/web"


def percentile(values, p):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * p) - 1)]


def distribution(values):
    return {
        "count": len(values),
        "min_ms": min(values),
        "median_ms": statistics.median(values),
        "p95_ms": percentile(values, 0.95),
        "p99_ms": percentile(values, 0.99),
        "max_ms": max(values),
    }


def stat_signature(path):
    stat = path.stat()
    return {
        "bytes": stat.st_size,
        "mtime_ns": stat.st_mtime_ns,
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
    }


def binary_metadata(path):
    version = subprocess.check_output([str(path), "--version"], text=True).strip()
    return {
        "path": str(path.relative_to(ROOT)),
        "version": version,
        **stat_signature(path),
    }


def hardware():
    if sys.platform == "darwin":
        return {
            "cpu_model": subprocess.check_output(
                ["/usr/sbin/sysctl", "-n", "machdep.cpu.brand_string"], text=True
            ).strip(),
            "logical_cpus": psutil.cpu_count(),
            "physical_cores": psutil.cpu_count(logical=False),
            "affinity_cpus": None,
            "memory_total_kib": psutil.virtual_memory().total // 1024,
            "os": platform.platform(),
            "architecture": platform.machine(),
            "python": platform.python_version(),
            "cpu_governor": None,
            "load_average": list(os.getloadavg()),
        }
    text = Path("/proc/cpuinfo").read_text()
    cpu = next(
        (
            line.partition(":")[2].strip()
            for line in text.splitlines()
            if line.startswith("model name")
        ),
        None,
    )
    physical = set()
    for section in text.split("\n\n"):
        fields = dict(
            line.split(":", 1) for line in section.splitlines() if ":" in line
        )
        fields = {k.strip(): v.strip() for k, v in fields.items()}
        if "physical id" in fields and "core id" in fields:
            physical.add((fields["physical id"], fields["core id"]))
    governor = Path("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
    memory = dict(
        line.split(":", 1) for line in Path("/proc/meminfo").read_text().splitlines()
    )
    return {
        "cpu_model": cpu,
        "logical_cpus": os.cpu_count(),
        "physical_cores": len(physical) or None,
        "affinity_cpus": len(os.sched_getaffinity(0)),
        "memory_total_kib": int(memory["MemTotal"].split()[0]),
        "os": platform.platform(),
        "architecture": platform.machine(),
        "python": platform.python_version(),
        "cpu_governor": governor.read_text().strip() if governor.exists() else None,
        "load_average": list(os.getloadavg()),
    }


def process_snapshot(pid):
    if sys.platform == "darwin":
        try:
            process = psutil.Process(pid)
            with process.oneshot():
                memory = process.memory_info()
                cpu = process.cpu_times()
            # macOS exposes current RSS here, not /proc's lifetime VmHWM or IO counters.
            return {
                "rss_kib": memory.rss / 1024,
                "peak_rss_kib": None,
                "user_seconds": cpu.user,
                "system_seconds": cpu.system,
                "read_bytes": None,
                "write_bytes": None,
                "read_chars": None,
                "write_chars": None,
            }
        except (psutil.Error, OSError):
            return None
    try:
        status = dict(
            line.split(":", 1)
            for line in Path(f"/proc/{pid}/status").read_text().splitlines()
            if ":" in line
        )
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        io = dict(
            line.split(":", 1)
            for line in Path(f"/proc/{pid}/io").read_text().splitlines()
        )
        return {
            "rss_kib": int(status["VmRSS"].split()[0]),
            "peak_rss_kib": int(status["VmHWM"].split()[0]),
            "user_seconds": int(fields[11]) / os.sysconf("SC_CLK_TCK"),
            "system_seconds": int(fields[12]) / os.sysconf("SC_CLK_TCK"),
            "read_bytes": int(io["read_bytes"]),
            "write_bytes": int(io["write_bytes"]),
            "read_chars": int(io["rchar"]),
            "write_chars": int(io["wchar"]),
        }
    except (OSError, KeyError, ValueError):
        return None


class MemorySampler:
    def __init__(self, pid):
        self.pid = pid
        self.samples = []
        self.done = threading.Event()
        self.thread = threading.Thread(target=self.run, daemon=True)

    def run(self):
        while not self.done.is_set():
            snapshot = process_snapshot(self.pid)
            if snapshot:
                self.samples.append(snapshot["rss_kib"])
            self.done.wait(0.02)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *args):
        self.done.set()
        self.thread.join()


def summarize(samples, elapsed, before=None, after=None, memory=None):
    result = {
        "requests": len(samples),
        "wall_seconds": elapsed,
        "requests_per_second": len(samples) / elapsed,
        "wire_latency": distribution([s["wire_ms"] for s in samples]),
        "total_latency": distribution([s["total_ms"] for s in samples]),
        "decode_latency": distribution([s["decode_ms"] for s in samples]),
        "response_wire_bytes": distribution_bytes([s["wire_bytes"] for s in samples]),
        "response_decoded_bytes": distribution_bytes(
            [s["decoded_bytes"] for s in samples]
        ),
        "samples": samples,
    }
    if before and after:
        result["server_process_before"] = before
        result["server_process_after"] = after
        result["server_delta"] = {
            k: after[k] - before[k]
            if before[k] is not None and after[k] is not None
            else None
            for k in [
                "user_seconds",
                "system_seconds",
                "read_bytes",
                "write_bytes",
                "read_chars",
                "write_chars",
            ]
        }
    if memory:
        result["server_sampled_rss_kib"] = {
            "min": min(memory),
            "max": max(memory),
            "median": statistics.median(memory),
        }
    return result


def distribution_bytes(values):
    return {"min": min(values), "median": statistics.median(values), "max": max(values)}


def command_sample(command, run_dir):
    stdout_file = run_dir / "last-cli-stdout.json"
    stderr_file = run_dir / "last-cli-stderr.txt"
    started = time.perf_counter_ns()
    with stdout_file.open("wb") as stdout, stderr_file.open("wb") as stderr:
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr, cwd=ROOT)
        _, status, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(status)
    elapsed = (time.perf_counter_ns() - started) / 1e6
    if process.returncode:
        raise RuntimeError(
            f"CLI exited {process.returncode}: {stderr_file.read_text()[:1000]} {stdout_file.read_text()[:1000]}"
        )
    result = json.loads(stdout_file.read_text())
    checking = command[1] == "check"
    if not result.get("ok") and not (
        checking and result.get("status") in {"clean", "issues"}
    ):
        raise RuntimeError(f"CLI emitted failure: {result}")
    if any(d.get("level") == "error" for d in result.get("diagnostics", [])):
        raise RuntimeError(f"CLI rendered source errors: {result['diagnostics']}")
    return {
        "wall_ms": elapsed,
        "peak_rss_kib": usage.ru_maxrss / 1024 if sys.platform == "darwin" else usage.ru_maxrss,
        "user_seconds": usage.ru_utime,
        "system_seconds": usage.ru_stime,
        "interactive": False if checking else result.get("interactive"),
        "output_bytes": Path(result["output"]).stat().st_size if "output" in result else 0,
    }


def cli_case(name, fixture, with_runtime, count, warmups, run_dir):
    public_dir = PUBLIC if with_runtime else run_dir / "no-runtime"
    public_dir.mkdir(parents=True, exist_ok=True)
    mode = "offline_runtime" if with_runtime else "static_check"
    output = fixture.parent / "Rendered" / f"{name}-{mode}.html"
    command = [
        str(CLI),
        "generate" if with_runtime else "check",
        str(fixture),
        "--style",
        "scientific",
        "--format",
        "json",
    ]
    if with_runtime:
        command += ["--output", str(output)]
    for _ in range(warmups):
        command_sample(command, run_dir)
    parent_before = process_snapshot(os.getpid())
    samples = [command_sample(command, run_dir) for _ in range(count)]
    assert all(sample["interactive"] == with_runtime for sample in samples)
    return {
        "workload": name,
        "runtime": mode,
        "iterations": count,
        "warmups": warmups,
        "command": command,
        "cold_process_wall": distribution([s["wall_ms"] for s in samples]),
        "wait4_max_rss_kib": distribution_bytes([s["peak_rss_kib"] for s in samples]),
        "python_parent_before": parent_before,
        "python_parent_after": process_snapshot(os.getpid()),
        "memory_note": (
            "Darwin wait4 ru_maxrss is converted from bytes to KiB. Process snapshots expose current RSS, "
            "not a lifetime high-water counter. These records do not satisfy the Linux VmHWM gate."
            if sys.platform == "darwin"
            else "Linux wait4 ru_maxrss may retain a high-water floor inherited from the Python parent's fork/exec. "
            "Parent /proc VmHWM is recorded for context. These values do not establish exact absolute CLI memory usage."
        ),
        "output_bytes": samples[-1]["output_bytes"],
        "samples": samples,
    }


def connection(host, port):
    return http.client.HTTPConnection(host, port, timeout=60)


def editor_case(name, fixture, source, count, warmups, run_dir):
    """Time the real native JSONL boundary without a VS Code host or browser."""
    runtime_url = "http://127.0.0.1:9"
    stderr_path = run_dir / f"editor-{name}-stderr.txt"
    startup = time.perf_counter_ns()
    with stderr_path.open("wb") as stderr:
        process = subprocess.Popen(
            [str(EDITOR)],
            cwd=ROOT,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=stderr,
        )
        poller = selectors.DefaultSelector()
        poller.register(process.stdout, selectors.EVENT_READ)
        identifier = 0
        last_response_ns = startup

        def request():
            nonlocal identifier, last_response_ns
            identifier += 1
            payload = (
                json.dumps(
                    {
                        "jsonrpc": "2.0",
                        "id": identifier,
                        "method": "preview",
                        "params": {
                            "source": source,
                            "path": str(fixture),
                            "styles": ["scientific"],
                            "public_dir": str(PUBLIC),
                            "runtime_url": runtime_url,
                        },
                    },
                    ensure_ascii=False,
                ).encode()
                + b"\n"
            )
            started = time.perf_counter_ns()
            process.stdin.write(payload)
            process.stdin.flush()
            chunks = []
            deadline = time.monotonic() + 60
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0 or not poller.select(remaining):
                    raise RuntimeError("Editor JSONL response timed out")
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    raise RuntimeError(
                        f"Editor stopped before response: {stderr_path.read_text()[:1000]}"
                    )
                chunks.append(chunk)
                if b"\n" in chunk:
                    assert chunk.endswith(b"\n") and chunk.count(b"\n") == 1
                    break
            received = time.perf_counter_ns()
            encoded = b"".join(chunks)
            response = json.loads(encoded)
            decoded = time.perf_counter_ns()
            last_response_ns = decoded
            assert response["id"] == identifier and "error" not in response, response
            result = response["result"]
            assert not any(d.get("level") == "error" for d in result["diagnostics"]), (
                result["diagnostics"]
            )
            assert result["metadata"]["_richview"]["interactive"] is True
            assert (
                runtime_url in result["html"]
                and "DecompressionStream" not in result["html"]
            )
            return {
                "wire_ms": (received - started) / 1e6,
                "decode_ms": (decoded - received) / 1e6,
                "total_ms": (decoded - started) / 1e6,
                "wire_bytes": len(encoded),
                "decoded_bytes": len(encoded),
                "request_bytes": len(payload),
                "html_bytes": len(result["html"].encode()),
                "chart_count": result["charts"],
            }

        try:
            cold = request()
            cold["startup_to_first_response_ms"] = (last_response_ns - startup) / 1e6
            cold["editor_process_after"] = process_snapshot(process.pid)
            for _ in range(warmups):
                request()
            before = process_snapshot(process.pid)
            with MemorySampler(process.pid) as memory:
                started = time.perf_counter()
                samples = [request() for _ in range(count)]
                elapsed = time.perf_counter() - started
            result = summarize(
                samples, elapsed, before, process_snapshot(process.pid), memory.samples
            )
            for key in list(result):
                if key.startswith("server_"):
                    result[key.replace("server_", "editor_", 1)] = result.pop(key)
            result.update(
                {
                    "workload": name,
                    "method": "JSONL",
                    "route": "preview",
                    "iterations": count,
                    "warmups": warmups,
                    "concurrency": 1,
                    "cold_first_response": cold,
                    "runtime_url": runtime_url,
                    "runtime_fetches": 0,
                    "html_bytes": distribution_bytes(
                        [sample["html_bytes"] for sample in samples]
                    ),
                    "scope": "Fresh persistent native editor process per workload, then sequential source previews over real stdin/stdout JSONL. Trusted loopback runtime URL is emitted into HTML only; no server, DNS or HTTP fetch occurs. Includes source resolution, parse/render, response serialization and pipe transfer; JSON decode separate. Excludes real VS Code host, webview load, Wasm init and browser painting.",
                }
            )
            return result
        finally:
            poller.close()
            process.stdin.close()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            process.stdout.close()
            if process.returncode != 0:
                raise RuntimeError(f"Editor failed: {stderr_path.read_text()[:1000]}")


def http_request(conn, method, path, body=None, encoding="identity", token=None):
    headers = {"Accept-Encoding": encoding}
    if body is not None:
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = f"Bearer {token}"
    started = time.perf_counter_ns()
    conn.request(method, path, body, headers)
    response = conn.getresponse()
    encoded = response.read()
    wire_end = time.perf_counter_ns()
    raw = (
        gzip.decompress(encoded)
        if response.getheader("content-encoding") == "gzip"
        else encoded
    )
    content_type = response.getheader("content-type", "")
    parsed = json.loads(raw) if "json" in content_type else None
    ended = time.perf_counter_ns()
    if response.status != 200:
        raise RuntimeError(f"HTTP {path} returned {response.status}: {raw[:400]!r}")
    if isinstance(parsed, dict) and "html" in parsed:
        if any(d.get("level") == "error" for d in parsed.get("diagnostics", [])):
            raise RuntimeError(
                f"HTTP render has source errors: {parsed['diagnostics']}"
            )
        if "_richview" in parsed.get("metadata", {}):
            assert parsed["metadata"]["_richview"]["interactive"] is True
    return {
        "wire_ms": (wire_end - started) / 1e6,
        "decode_ms": (ended - wire_end) / 1e6,
        "total_ms": (ended - started) / 1e6,
        "wire_bytes": len(encoded),
        "decoded_bytes": len(raw),
        "content_encoding": response.getheader("content-encoding", "identity"),
        "chart_count": parsed.get("metadata", {})
        .get("_richview", {})
        .get("chart_count")
        if isinstance(parsed, dict)
        else None,
    }, parsed


def read_case(host, port, pid, route, count, warmups, encoding):
    conn = connection(host, port)
    for _ in range(warmups):
        http_request(conn, "GET", route, encoding=encoding)
    before = process_snapshot(pid)
    with MemorySampler(pid) as memory:
        started = time.perf_counter()
        samples = [
            http_request(conn, "GET", route, encoding=encoding)[0] for _ in range(count)
        ]
        elapsed = time.perf_counter() - started
    conn.close()
    result = summarize(samples, elapsed, before, process_snapshot(pid), memory.samples)
    result.update(
        {
            "method": "GET",
            "route": route,
            "concurrency": 1,
            "warmups": warmups,
            "requested_encoding": encoding,
        }
    )
    return result


def save(evidence, output):
    temporary = output.with_suffix(".json.pending")
    temporary.write_text(json.dumps(evidence, indent=2) + "\n")
    temporary.replace(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare-only", action="store_true")
    parser.add_argument("--profile", choices=["final", "exploratory"], default="final")
    parser.add_argument("--cli-iterations", type=int, default=100)
    parser.add_argument("--editor-iterations", type=int, default=100)
    parser.add_argument("--http-sequential", type=int, default=20)
    parser.add_argument("--http-concurrent", type=int, default=40)
    parser.add_argument("--concurrency", type=int, default=4)
    parser.add_argument("--warmups", type=int, default=3)
    parser.add_argument("--output", type=Path, default=BENCH / "native-surfaces.json")
    args = parser.parse_args()
    args.output = args.output.resolve()
    fixtures = {
        name: BENCH / "fixtures" / name / "report.rmd"
        for name in ["small", "flagship", "dense"]
    }
    sources = {name: path.read_text() for name, path in fixtures.items()}
    resolved_data = {
        name: json.loads((path.parent / "render-options.json").read_text())["data"]
        for name, path in fixtures.items()
    }
    inventories = {
        name: sorted(
            asset.relative_to(path.parent).as_posix()
            for asset in path.parent.rglob("*")
            if asset.is_file() and "Rendered" not in asset.relative_to(path.parent).parts
        )
        for name, path in fixtures.items()
    }
    if args.prepare_only:
        print(
            json.dumps(
                {
                    "status": "prepared",
                    "fixtures": {
                        name: len(source.encode()) for name, source in sources.items()
                    },
                    "planned_cli_processes": 6 * (args.cli_iterations + args.warmups),
                    "planned_editor_requests": 3
                    * (args.editor_iterations + args.warmups + 1),
                    "planned_render_requests": 6
                    * (args.http_sequential + args.http_concurrent + args.warmups * 2),
                },
                indent=2,
            )
        )
        return
    runtime_artifacts = [
        PUBLIC / "richview_site.js",
        PUBLIC / "richview_site_bg.wasm",
        PUBLIC / "richview_site_bg.wasm.gz",
    ]
    assert (
        gzip.decompress(runtime_artifacts[2].read_bytes())
        == runtime_artifacts[1].read_bytes()
    ), "Precompressed Wasm differs from current build"
    assert (
        runtime_artifacts[2].stat().st_mtime_ns
        >= runtime_artifacts[1].stat().st_mtime_ns
    ), "Precompressed Wasm timestamp is stale"
    initial_signatures = {
        str(path): stat_signature(path)
        for path in [CLI, SERVER, EDITOR, *runtime_artifacts]
    }
    # Refuse racing artifact replacement; parent starts final runs after builds and TS baseline finish.
    time.sleep(0.5)
    assert initial_signatures == {
        str(Path(path)): stat_signature(Path(path)) for path in initial_signatures
    }, "Build artifacts changed before benchmark"
    run_dir = BENCH / f"native-run-{time.time_ns()}"
    run_dir.mkdir(parents=True)
    evidence = {
        "status": "running",
        "profile": args.profile,
        "started_utc": datetime.now(UTC).isoformat(),
        "hardware": hardware(),
        "software": {
            "cli": binary_metadata(CLI),
            "server": binary_metadata(SERVER),
            "editor": {
                "path": str(EDITOR.relative_to(ROOT)),
                "protocol": "JSONL JSON-RPC2.0",
                **stat_signature(EDITOR),
            },
        },
        "artifacts": {
            str(path.relative_to(ROOT)): initial_signatures[str(path)]
            for path in runtime_artifacts
        },
        "inputs": {
            name: {
                "path": str(fixtures[name].relative_to(ROOT)),
                "source_bytes": len(source.encode()),
                "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                "chart_count": len(re.findall(r"^```chart\s*$", source, re.MULTILINE)),
                "style": "scientific",
            }
            for name, source in sources.items()
        },
        "method": {
            "cli": f"New process each sample; {args.warmups} warmups warm only OS cache. Wall time includes process startup, resource/style/runtime resolution, Rust rendering, verification and base64 packaging of precompressed Wasm, and HTML write. Static variant omits runtime packaging. Raw wait4 maxRSS is labeled conservatively because of possible inherited parent high-water floor.",
            "http": f"Persistent standard-library HTTP connections to isolated loopback server. Requests contain same inline-data source and scientific style. {args.http_sequential} sequential and {args.http_concurrent} requests at {args.concurrency} clients per workload/encoding. Wire latency includes complete response reception; decode and total latency expose client gzip/JSON cost. Throughput is observed end-to-end client throughput, not maximum server capacity.",
            "memory": (
                "macOS current process RSS sampled every20ms through psutil7.2.2, with user/system time deltas. "
                "Lifetime RSS and IO counters are unavailable and remain null. Sampler overhead included. "
                "Darwin wait4 maxRSS bytes are converted to KiB; the Linux VmHWM acceptance gate remains unverified."
                if sys.platform == "darwin"
                else "Server actual /proc RSS sampled every20ms, with /proc VmHWM snapshots; user/system time and IO deltas "
                "before/after. Memory sampler overhead included. CLI wait4 ru_maxrss may inherit parent's fork/exec "
                "high-water floor; Python parent VmHWM is recorded and CLI RSS is labeled conservatively."
            ),
            "isolation": "Task-local output/store, explicit127.0.0.1 bind, synthetic token used only for local report seeding. No hosted network calls, desktop automation, cache dropping or external publication.",
        },
        "run_dir": str(run_dir.relative_to(ROOT)),
        "cli": [],
        "editor": [],
        "http": [],
        "storage": [],
    }
    save(evidence, args.output)
    for name, fixture in fixtures.items():
        for with_runtime in [False, True]:
            result = cli_case(
                name, fixture, with_runtime, args.cli_iterations, args.warmups, run_dir
            )
            evidence["cli"].append(result)
            save(evidence, args.output)
            print(
                json.dumps(
                    {
                        "phase": "cli",
                        "workload": name,
                        "runtime": result["runtime"],
                        "median_ms": result["cold_process_wall"]["median_ms"],
                        "p95_ms": result["cold_process_wall"]["p95_ms"],
                    }
                ),
                flush=True,
            )
    for name, fixture in fixtures.items():
        result = editor_case(
            name, fixture, sources[name], args.editor_iterations, args.warmups, run_dir
        )
        evidence["editor"].append(result)
        save(evidence, args.output)
        print(
            json.dumps(
                {
                    "phase": "editor",
                    "workload": name,
                    "cold_first_response_ms": result["cold_first_response"][
                        "startup_to_first_response_ms"
                    ],
                    "median_ms": result["total_latency"]["median_ms"],
                    "p95_ms": result["total_latency"]["p95_ms"],
                }
            ),
            flush=True,
        )
    token = "isolated-native-benchmark-token"
    env = dict(os.environ, RICHVIEW_TOKEN=token)
    env.pop("RICHVIEW_PUBLIC_URL", None)
    server = subprocess.Popen(
        [
            str(SERVER),
            "server",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--data-dir",
            str(run_dir / "store"),
        ],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )
    try:
        poller = selectors.DefaultSelector()
        poller.register(server.stdout, selectors.EVENT_READ)
        assert poller.select(30), "Server startup timeout"
        line = server.stdout.readline()
        match = re.search(r"http://127\.0\.0\.1:(\d+)", line)
        assert match, f"Unexpected server startup: {line}"
        host, port = "127.0.0.1", int(match[1])
        evidence["server"] = {
            "pid": server.pid,
            "bind": host,
            "ephemeral_port": port,
            "startup": process_snapshot(server.pid),
        }
        # `POST /api/render` is gone (FEA-SF-17); the report host has no render workload.
        conn = connection(host, port)
        flagship_version = None
        for i in range(12):
            name = "flagship" if i == 0 else "small"
            source = sources[name]
            payload = json.dumps(
                {
                    "source": source,
                    "styles": ["scientific"],
                    "data": resolved_data[name],
                    "slug": f"benchmark-{i}",
                    "new": True,
                    "reportFolder": {
                        "root": f"benchmark-{i}",
                        "source": "report.rmd",
                        "files": inventories[name],
                    },
                },
                ensure_ascii=False,
            ).encode()
            _, publication = http_request(
                conn, "POST", "/api/reports", payload, token=token
            )
            assert publication.get("ok") is True and publication.get("created") is True
            if i == 0:
                flagship_version = publication["version"]
        conn.close()
        assert flagship_version
        evidence["storage_fixture"] = {
            "reports": 12,
            "flagship_slug": "benchmark-0",
            "version": flagship_version,
        }
        for route in [
            "/api/reports",
            "/api/reports/benchmark-0",
            f"/r/benchmark-0/v/{flagship_version}",
            f"/raw/benchmark-0/v/{flagship_version}",
        ]:
            result = read_case(
                host,
                port,
                server.pid,
                route,
                args.http_sequential,
                args.warmups,
                "gzip",
            )
            evidence["storage"].append(result)
            save(evidence, args.output)
            print(
                json.dumps(
                    {
                        "phase": "storage",
                        "route": route,
                        "median_ms": result["total_latency"]["median_ms"],
                        "read_chars": result["server_delta"]["read_chars"],
                    }
                ),
                flush=True,
            )
        evidence["server"]["final"] = process_snapshot(server.pid)
    finally:
        server.send_signal(signal.SIGINT)
        try:
            server.wait(timeout=15)
        except subprocess.TimeoutExpired:
            server.kill()
            server.wait()
        if server.returncode not in [0, -signal.SIGINT]:
            raise RuntimeError(
                f"Server failed during benchmark: {server.stderr.read()[:1000]}"
            )
    final_signatures = {
        str(Path(path)): stat_signature(Path(path)) for path in initial_signatures
    }
    assert final_signatures == initial_signatures, (
        "Build artifacts changed during benchmark; rerun"
    )
    assert all(
        hashlib.sha256(path.read_bytes()).hexdigest()
        == evidence["inputs"][name]["source_sha256"]
        for name, path in fixtures.items()
    ), "Fixtures changed during benchmark; rerun"
    evidence.update(
        {
            "status": "complete",
            "finished_utc": datetime.now(UTC).isoformat(),
            "hardware_after": hardware(),
            "artifact_signatures_stable": True,
        }
    )
    save(evidence, args.output)
    print(
        json.dumps(
            {"status": "complete", "output": str(args.output.relative_to(ROOT))}
        ),
        flush=True,
    )


if __name__ == "__main__":
    main()
