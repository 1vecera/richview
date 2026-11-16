# RichView native host

`richview-app` is the native host of the pure `richview-core` engine: the `richview` command line, the stdio MCP server, the report host (`richview server`, RichView Cowork), the resolver, the report store and the asset uploader.

Every surface calls one operation layer (`src/ops.rs`): `validate`, `check`, `generate`, `publish`, `learn` and `init` return the same JSON object and fail with the same typed diagnostics whichever surface asked (ARC-08). A surface only decodes its input, calls the operation and encodes the result.

```sh
richview init my-report
richview check my-report/report.rmd
richview generate my-report/report.rmd          # writes my-report/Rendered/report.html
richview serve my-report/report.rmd
richview learn chart-contract
richview mcp
RICHVIEW_TOKEN=local-token richview server --data-dir tmp/store --port 8080
RICHVIEW_TOKEN=local-token richview publish my-report/report.rmd --server http://127.0.0.1:8080
```

## The report runtime

The binary embeds the report runtime at build time (ARC-07), so `generate` works from any working directory and no flag or environment variable selects a runtime from disk. `cargo run --release -p richview-build` stages the runtime and then builds the binaries with the `embedded-runtime` feature. Bootstrap builds omit that feature and refuse all exports with `report.output.runtime` and instructions to stage the runtime. A build with the feature fails if the staged files are missing. Resolver and validation tests run without runtime assets; tests that export or publish use the real staged runtime in the release tier. `richview --version` prints the build commit and the first twelve characters of the runtime hash.

## Output

Diagnostics print to standard error as `<file>:<line> <level> <code>: <message>`; identical `info` lines collapse into one with a count. The result line goes to standard output. `--format json` prints one object with camelCase keys and `ok`, `status`, `output` and `diagnostics` on every result (TYP-68). Exit code 0 means no error, 1 an error, 2 a wrong call.

## Tests

`cargo test -p richview-app` runs the unit and contract tests. They contact no hosted service.
