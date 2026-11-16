# Performance budgets

The release and full gates run [the perf step](../tools/rv-gate/perf.mjs) and its [budget definitions](../tools/rv-gate/lib/perf.mjs). Measurements identify the source and runtime hashes; an older capture does not establish a new build's performance. Historical captures remain in [merged baseline `3c07128`](https://github.com/richview-universe/richview-v3/tree/3c07128b7139687b754f6e1b5848c2a46c31cfe5/docs/evidence).

Until launch, Daniel’s 9 November ruling makes all size limits advisory: the perf step records measurements and original limits and reports overages without failing. Missing or invalid measurements still fail; timing, instructions and memory remain binding. S27-980 tracks trimming and restoring the size gates.

Daniel changed these limits on 29 October 2026: “regression margins loosen, per chart type timing do not care, cli render time care, but double, cli memory peak 5x, layout 100ms, keep the rest thanks.” His measurement direction was “max 10 runs or sth”. This supersedes the earlier 8 ms control-layout limit and absolute-byte regression margins. His earlier “Rebase + quiet + 2 MB” decision still sets the site payload and accepts the size and memory growth since 28 October.

| Measure | Accepted limit |
| --- | --- |
| Complete offline CLI p95: small / flagship / dense (PRF-11) | 300 / 500 / 800 ms |
| Complete CLI peak physical footprint: small / flagship / dense (PRF-40) | 320 / 480 / 1280 MiB |
| Control layout p95, desktop and phone 4× CPU (FEA-BR-36) | 100 ms |
| Control input to two animation frames p95 | 100 ms |
| WebKit frame p95 / maximum (PRF-21) | 8 / 16.7 ms |
| Entire initial site payload, gzip -9 (PRF-30) | 2,000,000 bytes |
| Report runtime payload, gzip -9 (PRF-31) | 2 MiB |
| Exported HTML, small / flagship / dense, excluding runtime and media (PRF-32) | 224 KiB / 768 KiB / 2.5 MiB |

Other documented endpoint limits are unchanged; their separate harness evidence is required before claiming them:

| Endpoint | Existing limit |
| --- | --- |
| Warm native report p95: small / six types / flagship / dense | 5 / 10 / 35 / 250 ms |
| Warm editor preview RPC p95: small / flagship / dense | 10 / 50 / 400 ms |
| Complete gzip HTTP p95, one client: small / dense | 200 / 600 ms |
| Complete gzip HTTP p95, four clients: small / dense | 500 / 1,500 ms |
| Four-client HTTP throughput: small / dense | At least 10 / 4 requests/s |
| Warm 9-chart theme switch, first / latest chart-frame p95 | 100 / 100 ms |
| Small source edit to first chart p95 | 300 ms |
| Complete dense worker source edit to first chart | 1,000 ms |

PRF-05 also compares each measurement with the accepted baseline in the gate home's `perf-baseline.json`. Wasm, HTML and shipped binary sizes may grow by 10%; CLI instruction counts by 20%; CLI footprint by 25%. Size growth beyond its margin is advisory until launch; instruction and footprint growth beyond their margins fails independently of the absolute limits. Per-chart-type native and mount timings are optional disclosures and never fail this gate; the full run omits them.

The next full run records its own clean build's sizes and CLI footprints as the new baseline under Daniel's dated authorization. The record names its exact commit and receipt, keeps the previous values, and consumes the authorization once. Instruction and per-type baseline values remain intact. Later runs enforce the instruction and footprint margins and report size overages as advisory until launch; ordinary runs cannot accept further growth.

## Measurement

Each measure uses at most ten measured repetitions after one warmup. CLI instructions and footprint use the median of seven measured renders. CLI p95 and every control case use ten samples; nearest-rank p95 is the slowest of those ten. The WebKit frame probe uses ten measured interaction windows after one warmup. Optional per-type measurements also stay below ten repetitions.

The perf step takes the benchmark lock and the shared quiet hold. New builds and browser checks wait; existing work drains. It waits at most two minutes, with at most 30 seconds for load to settle after builds and browsers drain. If the host cannot become quiet, it measures anyway and records the load and remaining processes. Timing and budget failures remain failures under load. When a timing limit fails while the 5-minute load is over 8, the hold stays up to 15 more minutes while the step waits for that load to fall under 6, and the capture after the wait decides; the receipt keeps every capture with its 1- and 5-minute loads. Sizes are compressed by the host's `gzip -9 -n`, and the receipt names that gzip and its version. Control cases retain both desktop and phone profiles, actual runtime state-change assertions, source hashes and GPU metadata.

macOS `/usr/bin/time -l` supplies instruction counts and peak physical footprint. RSS, Linux `VmHWM`, browser timing and whole-process footprint measure different costs. The old Linux checker is retired and historical results are not a current full verdict.

The former Linux `VmHWM` 256 MiB row is superseded by PRF-40's reference-host footprint limits (raised fivefold on 29 October). The former 1.5 MiB delivered-Wasm row is superseded by PRF-30's whole-site payload and PRF-31's 2 MiB report-runtime payload.

## Reproduce

Build with the pinned toolchain and `cargo run --release --locked -p richview-build`, then run the full gate. On Claw Mini, source the shared `env.sh` first so builds use the shared target and its locks.

For a standalone capture against a coherent build:

```sh
node tools/rv-gate/perf.mjs --bin target/release/richview --dist dist/web --out tmp/performance
```

This measures sizes, CLI costs and timing. `--stages types` requests optional per-type disclosures. Do not use an old capture, a partial cohort or an unapproved baseline update to claim the current full passed.
