# Public interfaces and indirect resources

This checker compares producer-approved Rust interfaces and resource bindings with checked snapshots. Its `interface-v1` report lets the gate distinguish an interface change from a private implementation edit. Cargo still owns compilation and type checking; the dependency checker owns allowed component directions.

The checker is developed separately from the chart and core facades. Producer owners approve the manifest entries and commit their snapshots after those facades land. An absent manifest or snapshot is a failure, never an empty success. The checker does not discover an entire crate and bless its private internals.

## Commands

```sh
node tools/api-contracts/check.mjs snapshot --root . --manifest tools/api-contracts/contracts.json
node tools/api-contracts/check.mjs snapshot --root . --manifest tools/api-contracts/contracts.json --contract core.document
node tools/api-contracts/check.mjs check --root . --manifest tools/api-contracts/contracts.json --base origin/dev
node --test tools/api-contracts/test/*.test.mts
```

`snapshot` prints `{path, snapshot}` entries for review; `--contract` prints just that contract's canonical snapshot, ready to save to a scratch file. It does not overwrite the accepted snapshot. `check` verifies every current declaration and binding against its snapshot, then reports changes from the Git base when one is supplied. Missing files, unsupported declaration forms and mismatched bindings fail explicitly. `--contract` cannot select a partial check.

Node follows the root `.node-version`. JavaScript/TypeScript resource bindings use the already pinned root TypeScript parser; install the existing dependencies with `npm ci --ignore-scripts`. No new dependency is introduced.

## Manifest

The manifest has `schema: "interface-v1"` and a nonempty `contracts` array. Each contract names a stable `id`, a `producer` crate, a repository-relative `snapshot` path and its declared consumers.

A Rust contract names the facade's `source`, its `crateRoot` and its direct public `module`. The crate root must expose that module with `pub mod name;`, loading the registered `name.rs` or `name/mod.rs` source. Making the module private, unlinking it or redirecting it fails. Supplementary `declarations` select explicitly exported types or functions from their defining files, so reexports cannot hide a changed signature. Only the intended public surface enters the snapshot. Comments, function bodies and private items do not; public fields, variants, signatures, aliases and reexport targets do. Wildcard exports and declaration forms the checker cannot represent are refused.

```json
{
  "schema": "interface-v1",
  "contracts": [
    {
      "id": "core.document",
      "producer": "richview-core",
      "kind": "rust",
      "module": "richview_core::document",
      "source": "crates/richview-core/src/document.rs",
      "crateRoot": "crates/richview-core/src/lib.rs",
      "snapshot": "tools/api-contracts/snapshots/core-document.json",
      "consumers": ["richview-app", "richview-editor"],
      "declarations": [
        {"source": "crates/richview-core/src/lib.rs", "items": ["Document", "RenderOptions"]}
      ]
    }
  ]
}
```

This illustrates the format; the producer must approve the actual export/type list. Public inherent methods and trait implementations of selected types also enter the snapshot. Losing `Default` or changing an associated type is an interface change, while changing a method body is not. Private field types are omitted; tuple positions and construction privacy remain visible.

A resource contract names its producer `source` and consumer bindings. A binding names its `consumer`, source file and either a literal Rust include or a runtime entry name. Includes must resolve to the declared producer file. Entry names must occur as literals in their consumers. Resource bytes and observed bindings enter the snapshot, so a moved, renamed or changed asset cannot silently retain an old contract.

```json
{
  "id": "report.style",
  "producer": "richview-core",
  "kind": "resource",
  "source": "assets/report.css",
  "snapshot": "tools/api-contracts/snapshots/report-style.json",
  "bindings": [
    {"consumer": "richview-editor", "source": "consumer/lib.rs", "include": {"macro": "include_str", "path": "../assets/report.css"}}
  ]
}
```

These are illustrative paths. Rust include bindings require literal `include_str!` or `include_bytes!` paths. Resource sources are actual assets; do not hash a Rust implementation file as a substitute for its public interface.

Runtime names can be checked before the runtime is staged. An `entries` contract records producer-approved names and their literal bindings, without a resource `source` or any substitute runtime bytes:

```json
{
  "id": "runtime.report.loader",
  "producer": "richview-runtime",
  "kind": "entries",
  "entries": ["/report-runtime.js"],
  "snapshot": "tools/api-contracts/snapshots/runtime-report-loader.json",
  "bindings": [
    {"consumer": "richview-build", "source": "producer/names.rs", "entry": "/report-runtime.js"},
    {"consumer": "richview-editor", "source": "consumer/loader.cjs", "entry": "/report-runtime.js"}
  ]
}
```

Entry bindings accept `.rs`, `.js`, `.cjs`, `.mjs` and `.ts` consumers; comments, regular expressions and interpolated templates cannot satisfy them. Every declared name must have an observed binding. Rust-specific string escapes the checker cannot decode cannot satisfy a binding. Macro-generated facades, public nested modules and public extern blocks require an explicit supported facade rather than an opaque private-body snapshot. Actual runtime staging and execution remain with native output guards and the release suite.

All paths stay inside the repository, including through symlinks. Contract and item IDs must be unique. Sorting and token normalization make formatting-only edits stable.

## Machine-readable seam

`check.mjs` exports `checkContracts({root, manifest, base})` for the gate and `snapshotContract(...)` for snapshot preparation. The CLI emits one JSON result with these fields:

```json
{
  "schema": "interface-v1",
  "status": "pass",
  "checked": ["core.document", "runtime.report.loader"],
  "findings": [],
  "changes": [],
  "indirect": [
    {
      "consumer": "richview-editor",
      "producer": "richview-runtime",
      "contract": "runtime.report.loader"
    }
  ]
}
```

`indirect` contains sorted, deduplicated triples for observed cross-component bindings. It carries public contract IDs, not private paths as policy identifiers. File and line hints belong in findings. A missing or mismatched binding fails this checker; an unlisted or forbidden producer direction fails the dependency checker.

`status` is exactly `pass` or `fail`. Every finding is fatal; v1 has no warning findings. Consumers reject an unknown schema/status, any status other than `pass`, or any nonempty findings array. A failed report may retain partial observations for diagnostics, which cannot be used as a complete dependency-policy input. The organizer obtains a fresh report for the same checkout/revision it gates.

Ordinary Cargo dependencies stay with the dependency graph. Declaring an API consumer does not fabricate an observed indirect binding.

`changes` compares canonical interfaces with the base manifest and snapshots. Updating a current snapshot does not hide a change from the gate's dependent-check selection. A newly registered contract is `added`; a changed surface is `changed`; a contract removed from the current manifest is `removed`. A registered base contract without its snapshot fails as incomplete history. No base is needed to verify that current source still matches its reviewed snapshot.

Runtime execution, serialization round trips and compiler-backed consumer contract tests remain with the producer and consumer suites. A passing source snapshot does not establish behavioral compatibility.

A pass covers only the registered, producer-approved contracts. `core.document` is an additive document facade; existing root style, YAML, registry and specification utilities remain outside that contract. It does not justify reducing the core's affected-dependent checks. Supplementary declarations are curated by the producer rather than inferred through arbitrary Rust name resolution; compiler checks remain mandatory.

`runtime.embed-files` pins the native embedded input filenames at their builder and app compile-time consumers. It does not hash staged runtime content or establish hosted output compatibility; the build manifest and runtime release checks own content, provenance and execution.

The reusable `publicSurface` helper in `lib/rust-surface.mjs` accepts `{moduleDeclarations: true}` for a producer's root export inventory. This explicit option retains direct external `pub mod name;` declarations alongside reexports and definitions. The default facade extractor still rejects modules. Inline module bodies, `#[path]` redirects and module-level generated exports remain unsupported.

This option inventories declarations in the supplied source; it does not load module bodies or emit a whole-root Git-base change report. Core's always-selected `registry.rs::core_public_exports_match_the_reviewed_snapshot` owns its reviewed root and curated utility check. Registry macros remain an explicit coverage gap. Keep affected-dependent Rust checks conservative until the remaining coverage and selection work is complete.
