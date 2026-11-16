#!/usr/bin/env python3
"""Check the fixed local performance budgets; missing/invalid evidence fails closed."""

import argparse
import hashlib
import json
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--evidence-dir", type=Path, default=ROOT / "tmp/performance"
    )
    parser.add_argument(
        "--output", type=Path, help="Defaults to EVIDENCE_DIR/gates.json"
    )
    args = parser.parse_args()
    directory = args.evidence_dir
    sources, documents, gates, integrity = {}, {}, [], []
    for name, aliases in (
        ("rust", ("rust.json",)),
        ("native", ("native.json", "native-surfaces.json")),
        ("browser", ("browser.json",)),
    ):
        path = next(
            (directory / alias for alias in aliases if (directory / alias).is_file()),
            directory / aliases[0],
        )
        try:
            raw = path.read_bytes()
            documents[name] = json.loads(raw)
            sources[name] = {
                "file": path.name,
                "sha256": hashlib.sha256(raw).hexdigest(),
            }
        except (OSError, ValueError) as error:
            documents[name] = {}
            sources[name] = {"file": path.name, "error": str(error)}

    def number(value):
        if (
            isinstance(value, bool)
            or not isinstance(value, (int, float))
            or not math.isfinite(value)
            or value < 0
        ):
            raise ValueError("Expected a finite nonnegative measurement")
        return value

    def gate(name, endpoint, limit, unit, read, operator="<="):
        row = {
            "name": name,
            "endpoint": endpoint,
            "operator": operator,
            "limit": limit,
            "unit": unit,
        }
        try:
            value = number(read())
            row.update(
                value=value,
                passed=value <= limit if operator == "<=" else value >= limit,
            )
        except (KeyError, IndexError, TypeError, ValueError) as error:
            row.update(value=None, passed=False, error=str(error))
        gates.append(row)

    def find(name, group, **fields):
        matches = [
            row
            for row in documents[name][group]
            if all(row.get(key) == value for key, value in fields.items())
        ]
        if len(matches) != 1:
            raise ValueError(
                f"Expected one {name}.{group} row matching {fields}, found {len(matches)}"
            )
        return matches[0]

    for workload, limit in (
        ("small", 5),
        ("six-types", 10),
        ("flagship", 35),
        ("dense", 250),
    ):
        gate(
            f"native.{workload}",
            "Warm native report p95",
            limit,
            "ms",
            lambda w=workload: find("rust", "results", workload=w)["p95_ms"],
        )
    for workload, limit in (("small", 150), ("flagship", 250), ("dense", 400)):
        gate(
            f"cli.{workload}",
            "Process-cold complete offline CLI p95",
            limit,
            "ms",
            lambda w=workload: find(
                "native", "cli", workload=w, runtime="offline_runtime"
            )["cold_process_wall"]["p95_ms"],
        )
    for workload, limit in (("small", 10), ("flagship", 50), ("dense", 400)):
        gate(
            f"editor.{workload}",
            "Warm preview RPC total p95; cached external runtime",
            limit,
            "ms",
            lambda w=workload: find("native", "editor", workload=w, route="preview")[
                "total_latency"
            ]["p95_ms"],
        )
    for workload, sequential, concurrent, throughput in (
        ("small", 200, 500, 10),
        ("dense", 600, 1500, 4),
    ):
        for clients, limit in ((1, sequential), (4, concurrent)):
            gate(
                f"http.{workload}.{clients}-clients",
                "Complete gzip loopback response + client decode p95",
                limit,
                "ms",
                lambda w=workload, c=clients: find(
                    "native",
                    "http",
                    workload=w,
                    concurrency=c,
                    requested_encoding="gzip",
                )["total_latency"]["p95_ms"],
            )
        gate(
            f"http.{workload}.throughput",
            "Four-client complete gzip loopback aggregate throughput",
            throughput,
            "requests/s",
            lambda w=workload: find(
                "native", "http", workload=w, concurrency=4, requested_encoding="gzip"
            )["requests_per_second"],
            ">=",
        )

    browser = documents["browser"]
    gate(
        "browser.theme.first-all-charts",
        "20 warm themes event to first frame of all 18 charts p95",
        100,
        "ms",
        lambda: browser["warm_20_flagship_theme_changes"]["event_to_all_chart_frames"][
            "p95_ms"
        ],
    )
    gate(
        "browser.theme.latest-all-charts",
        "20 warm themes latest chart frames observed after two RAF p95 (original endpoint)",
        100,
        "ms",
        lambda: browser["warm_20_flagship_theme_changes"][
            "event_to_settled_all_chart_frames"
        ]["p95_ms"],
    )
    gate(
        "browser.small-edit",
        "20 warm small source edits first live chart p95",
        300,
        "ms",
        lambda: browser["warm_20_small_source_edits"]["input_to_first_live_frame"][
            "p95_ms"
        ],
    )
    gate(
        "browser.dense-edit",
        "Complete dense worker source edit to first live chart, single event",
        1000,
        "ms",
        lambda: browser["dense_source_live_edit"]["input_to_first_live_frame_ms"],
    )

    def native_peak_bytes():
        match = re.fullmatch(r"VmHWM:\s*(\d+)\s+kB\s*", documents["rust"]["peak_rss"])
        if not match:
            raise ValueError(
                "Expected Linux /proc VmHWM in KiB, not inherited getrusage RSS"
            )
        return int(match.group(1)) * 1024

    gate(
        "native.peak-memory",
        "Complete native report workload sequence Linux VmHWM",
        256 * 1024 * 1024,
        "bytes",
        native_peak_bytes,
    )
    gate(
        "browser.delivered-wasm",
        "Complete actual gzip-delivered Wasm bytes",
        int(1.5 * 1024 * 1024),
        "bytes",
        lambda: browser["wasm"]["delivered_gzip_bytes"],
    )

    def check(name, read):
        try:
            passed = read() is True
            row = {"name": name, "passed": passed}
        except (KeyError, IndexError, TypeError, ValueError) as error:
            row = {"name": name, "passed": False, "error": str(error)}
        integrity.append(row)

    check(
        "native capture completed with stable artifacts",
        lambda: (
            documents["native"]["status"] == "complete"
            and documents["native"]["artifact_signatures_stable"] is True
        ),
    )
    check(
        "native engine capture has stable artifacts",
        lambda: documents["rust"]["provenance"]["artifact_signatures_stable"] is True,
    )
    check(
        "browser behavioral assertions passed without errors",
        lambda: browser["status"] == "passed" and browser["errors"] == [],
    )
    check(
        "delivery encoding is actually gzip",
        lambda: browser["wasm"]["delivered_gzip_encoding"] == "gzip",
    )
    check(
        "native and browser captures use identical runtime bytes",
        lambda: (
            documents["native"]["artifacts"]["dist/web/richview_site_bg.wasm"]["sha256"]
            == browser["wasm"]["sha256"]
        ),
    )
    check(
        "dense browser worker receives the measured native fixture",
        lambda: (
            browser["dense_source_live_edit"]["source_sha256"]
            == documents["native"]["inputs"]["dense"]["source_sha256"]
        ),
    )

    def matching_native_inputs():
        provenance = documents["rust"]["provenance"]
        if "inputs" in provenance:
            return all(
                provenance["inputs"][name]["source_sha256"]
                == documents["native"]["inputs"][name]["source_sha256"]
                for name in ("small", "flagship", "dense")
            )
        recorded = provenance["artifacts"]
        fixtures = {
            Path(path).stem: value["sha256"]
            for path, value in recorded.items()
            if path.endswith(".rmd")
        }
        return all(
            fixtures[name] == documents["native"]["inputs"][name]["source_sha256"]
            for name in ("small", "flagship", "dense")
        )

    check(
        "native surface and engine captures use identical fixtures",
        matching_native_inputs,
    )

    passed = all(row["passed"] for row in gates + integrity)
    output = args.output or directory / "gates.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(
            {
                "status": "passed" if passed else "failed",
                "scope": "Fixed local budgets in docs/PERFORMANCE.md; all endpoints required. Node boundary, CSV edit, chart-kernel regression and browser RSS remain disclosures, not retroactive gates.",
                "sources": sources,
                "gates": gates,
                "integrity": integrity,
            },
            indent=2,
        )
        + "\n"
    )
    failures = [row for row in gates + integrity if not row["passed"]]
    print(
        f"Performance gates: {'PASS' if passed else 'FAIL'} ({len(gates)} budgets, {len(integrity)} integrity checks); {output}"
    )
    for row in failures:
        print(
            f"  {row['name']}: {row.get('error', str(row.get('value')) + ' ' + row.get('operator', '') + ' ' + str(row.get('limit', '')))}"
        )
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
