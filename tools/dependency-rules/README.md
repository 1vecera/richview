# Dependency rules

The ARC-01 checker verifies canonical Cargo package names against `rules.json`, rejects cycles and unassigned workspace crates, and limits chart/core dependencies by normal, dev and build kind. Renames, optional dependencies and target conditions do not exempt an edge. Pure crates cannot add build scripts or local crates outside the workspace. The gate's existing `arch` step separately checks source I/O boundaries.

The API checker owns export shape, embedded resource identity and runtime entry names. This checker consumes its freshly verified `interface-v1` report and requires `status: "pass"` with no findings. Every observed indirect binding must match an explicit `{consumer, producer, contract}` rule and point to an allowed lower layer. Contract IDs identify public producer surfaces. Private source paths are not contract IDs.

`indirect` lists producer-approved public contracts and their observed consumers. ARC-07's `runtime.embed-files` has a named resource-only exception for the native app embedding the report runtime. It authorizes this one public contract, while the app's Cargo dependency on the runtime crate remains forbidden. No exception applies to pure chart/core or other consumer/producer/contract names. The build tool's runtime binding follows its ordinary allowed direction. Missing API verification fails; the API checker still verifies the exact registered filenames and bindings.

```js
import {checkDependencies} from "./check.mjs";
const report = checkDependencies({metadata, rules, apiReport});
```

`metadata` is current `cargo metadata --format-version 1 --no-deps --offline` output. `apiReport` is the API checker's report for the same checkout and revision, obtained during the current gate run. The caller owns freshness. `checkDependencies` performs no I/O and returns:

```json
{"schema":"interface-v1","status":"pass","findings":[],"checked":{"crates":11,"indirect":0}}
```

Failures use `status: "fail"` and findings with `code`, `path` and `message`. Unknown schemas, malformed metadata, failed or contradictory API reports fail. Partial bindings from failed API verification never authorize dependencies. `checked` counts inspected workspace crates and successfully verified indirect bindings; it does not describe API snapshot coverage.

The CLI reads current Cargo metadata and a fresh API report file supplied by the gate. It prints one JSON document and exits 0 for pass or 1 for any failure, including invalid arguments or missing input. Paths are relative to `--root` (the current directory by default). Source the configured build environment on Claw Mini so all Cargo commands use the shared target and shim.

```sh
node tools/dependency-rules/check.mjs check --root . --api-report tmp/api-current.json
node --test tools/dependency-rules/test/*.test.mjs
```

The optional `--rules <json>` path selects a policy file. Gate integrations use this checkout's reviewed `rules.json`. `graph.mjs` supplies the one graph engine; the existing rv-gate layer-check entry point delegates to it. Gate registry, fast selection and coverage mappings remain organizer-owned.
