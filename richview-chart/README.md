# richview-chart

RichView's chart engine: the Rust code behind every Rich Markdown chart block. Reports, the CLI, the server and the browser runtime draw charts with it. It performs no I/O and has no command of its own; `richview` is the only command (OPN-68), and callers resolve data before they hand rows to the engine.

From the workspace root:

```bash
cargo test --locked -p richview-chart
cargo run --release --locked -p richview-chart --example benchmark
```

## How a chart is drawn

`block::ChartBlock::from_value` validates a chart block as core hands it over. `layout::layout(block, rows, theme, options)` turns the block, its rows, a width and the style's `layout::ChartTheme` into one `geometry::Geometry`: panels with their scales, marks, labels, annotations and observations, the legend, the summary and the checks (ARC-18). Every check measures that model. `vector` draws it as SVG for PDF export and share cards, and the report runtime (`crates/richview-runtime`) lays the same block out again at the reader's width and draws it with WebGL. `layout::layout_prepared` is the same layout with a `layout::Prepared`, which keeps the rows as read and the readouts as formatted for one data state: the runtime keeps one per state and hidden set, so a zoom or resize frame reads no row and formats no readout again (PRF-19). Core embeds each figure's block, rows and derived structures in a `payload::Payload` for the runtime.

- `reader_controls` resolves the playback, filter, metric and variant controls of a block to the block and rows of each reader state (FEA-BR-01 to FEA-BR-06).
- `derived` holds what a layout derives from exact values, so the runtime's layout from grid rows draws the same bins, quartiles and orderings (TYP-36).
- `hit` gives the keyboard order and the pointer targets over a geometry, and `interaction` places readouts and holds the chart actions (FEA-CH-39, FEA-CH-42, FEA-CH-43).
- `numbers`, `color`, `text`, `time`, `scale`, `axis`, `binning`, `box_summary`, `curve`, `labels` and `motion` are the parts the layouts share.

[ADDING-A-TYPE.md](ADDING-A-TYPE.md) adds a chart type. Each type has a golden fixture in `tests/fixtures/types/` or its contract file, and a `tests/<type>_contract.rs` that runs the contract every type shares (FEA-CH-75).

## Benchmark

The benchmark example lays out the chart blocks of the three frozen time-series examples in `docs/public/examples` (Mauna Loa CO₂, US consumer prices and 35,040 quarter-hours of German power prices) at 390 and 1,280 px over 100 zoom windows from one preparation per width, as the runtime lays out a zoom frame, and each window once more on its own (`oneShot`), as a new data state is laid out. It times nearest-observation hover and prints the median and 95th percentile as JSON (PRF-19). Reading the CSV belongs to core and is outside the timings, as are the painters, Wasm start and the DOM.
