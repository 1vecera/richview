# Contributing to RichView

RichView renders Rich Markdown (`.rmd`) into sourced, editable reports. Rust owns parsing, validation, data transforms, chart layout, SVG rendering, diagnostics, browser interaction, authoring and version storage; the same report engine runs natively and in WebAssembly.

## Report a problem

[Open an issue](https://github.com/1vecera/richview/issues) with the smallest `.rmd` that shows the problem, what you expected and the output of `richview --version`. A report that renders badly is a bug: attach a screenshot.

## Before you start

RichView's goals, rules and boundaries are written down in [`docs/lockdown/`](docs/lockdown/README.md): start with [GOALS](docs/lockdown/GOALS.md), [RULES](docs/lockdown/RULES.md) and [BOUNDARIES](docs/lockdown/BOUNDARIES.md). A change that adds, changes or removes scope should say which decision it touches, and a larger change deserves an issue first. [`docs/guide/`](docs/guide/README.md) is the design documentation.

`main` is the default branch: branch from it and open pull requests against it. Contributions are accepted under the repository's MIT licence ([LICENSING.md](LICENSING.md)).

## Set up

Install the pinned toolchain: Rust 1.99.0 with the `wasm32-unknown-unknown` target (both come from `rust-toolchain.toml`), `wasm-bindgen-cli` 0.2.129, Node (the version in `.node-version`) and Playwright 1.65.0. `Cargo.lock` fixes library versions. The product is Rust: the build uses Node only to bundle the VS Code adapter, and Node and Playwright are otherwise development tools for the checks.

```sh
cargo install wasm-bindgen-cli --version 0.2.129 --locked
npm ci --ignore-scripts
cargo run --release --locked -p richview-build -- --repository-public      # stages dist/web, packages and the editor
```

Then run the preview server on the built site:

```sh
target/release/richview server --data-dir tmp/reports --port 8080
```

and the CLI on a report:

```sh
target/release/richview init tmp/my-report
target/release/richview generate tmp/my-report/report.rmd --format json
target/release/richview check tmp/my-report/report.rmd --format json
target/release/richview serve tmp/my-report/report.rmd
target/release/richview learn
target/release/richview mcp
```

`richview server` runs the authenticated publication server and its report store; its sign-in page is at `http://127.0.0.1:8080/`. The browser playground, chart gallery and showcase belong to the static website built into `dist/web/`.

## Check your change

Run these before you push. Choose checks that cover your change, and run all of them for a larger one.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p richview-site --target wasm32-unknown-unknown --locked -- -D warnings
cargo test --workspace --locked
node tools/rv-gate/rv-gate.mjs fast      # tests and lints for the crates you changed
```

`node tools/rv-gate/rv-gate.mjs steps` lists every step of the repository's gate. Performance measurements need an otherwise quiet CPU; their scopes, budgets and reproduction commands are in [PERFORMANCE.md](docs/PERFORMANCE.md).

Browser and editor host checks use Playwright and Node only as test tools: `scripts/verify-*.mjs` and `scripts/verify-*.cjs`. Install the pinned browsers with `npx playwright install chromium firefox webkit`. Functional suites that use `scripts/browser-harness.mjs` accept `RICHVIEW_VERIFY_BROWSER=chromium|firefox|webkit`; Chromium is the default. For example:

```sh
RICHVIEW_VERIFY_BROWSER=firefox node scripts/verify-browser-api.mjs
```

Each run records its engine and version in `browser-environment.json`. The fixed visual matrix, timing budgets, PDF checks and trusted CDP pinch injection stay on Chromium; the other engines run the same data, keyboard, pointer, source-editing and export contracts. WebKit currently clears native textarea undo history when the live preview iframe navigates. The browser suite reproduces this in an independent textarea and iframe control and records the limitation; native undo and redo assertions run where that capability is available. All engines still verify full-source paging, editing, clipboard and mixed-line-ending preservation.

[FEATURES.md](docs/FEATURES.md) maps each contract to its implementation and verification. Building and testing locally publishes nothing: deployment, releases and hosted publication are separate, explicit actions.

## Source map

| Responsibility | Location |
| --- | --- |
| Chart engine: contracts, scales, geometry, interaction policy | `richview-chart/` |
| Format, schemas, style cascade, renderer, knowledge | `crates/richview-core/` |
| CLI, resolver, HTTP host, report store, MCP | `crates/richview-app/` |
| Browser runtime: charts, controls, playground, landing, showcase | `crates/richview-site/` |
| Editor service | `crates/richview-editor/` |
| Build orchestrator and benchmarks | `crates/richview-build/`, `crates/richview-bench/` |
| RichView Publish builder and Worker | `crates/richview-publish/`, `cloudflare/publish/` |
| VS Code adapter | `packages/vscode/` |
| Site pages, styles and fonts | `assets/` |
| Gate, lockdown app | `tools/rv-gate/`, `tools/lockdown-app/` |

Generated `wasm-bindgen` bindings and runtime bootstraps connect Rust to JavaScript hosts. `packages/vscode/extension.cjs` is a thin activation adapter for VS Code APIs, source transport and SecretStorage credentials; it delegates authoring, validation and rendering to a persistent native Rust service. Core and engine stay pure: no filesystem, network, process, environment or clock access. [RUST-ARCHITECTURE.md](docs/RUST-ARCHITECTURE.md) covers extension points, and the [format specification](docs/RMD-SPEC.md) and the schema registry move together.

## Rules the code keeps

- The schema registry (`crates/richview-core/src/registry.rs`) is the one definition of tags, fields, chart types and diagnostic codes. The reference tables in [RMD-SPEC.md](docs/RMD-SPEC.md) are generated from it: edit the registry, then run `RICHVIEW_BLESS=1 cargo test -p richview-core --test spec_docs`. Prose explains the vocabulary but never lists it.
- Diagnostics carry level, code, message, path, line and tag, and every surface returns them unchanged.
- Authors customise only through validated vocabulary: no raw HTML, CSS, JavaScript, engine specs or SVG from any source or style file. Safety is never traded.
- Readers never get more precision than the author chose.
- Preserve source lines, unknown metadata and fences, exact values, units and missing observations.
- Visual and motion changes follow [docs/guide/styles.md](docs/guide/styles.md). A person or a review judges them, because tests alone don't establish visual quality. Preserve the packaged font declarations and their notices.

## Packaging notes

Native binary distributions must include the generated `target/release/font-licenses/` directory. The VSIX carries the same notices, and the container includes them under `/app/public/assets/showcase/fonts/licenses/`.

The `richview` binary embeds the report runtime, so it works from any working directory. Keep the font notices alongside the binary; a `font-licenses` link to `<prefix>/share/richview/font-licenses/` is sufficient.

## Releasing the website

Deploying richview.ai is a maintainer action that needs explicit authorization. The staging procedure is in [docs/WEBSITE-RELEASE.md](docs/WEBSITE-RELEASE.md).
