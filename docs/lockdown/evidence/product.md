# Product inventory: RichView components and features

This is evidence for lockdown part 03 (components, `C-`) and part 04 (features, `F-`). It covers what exists on `origin/main` at `b4db233` (the merge of PR #61, 2026-10-16) in `github.com/1vecera/richview`, a private repository that the old `richview-universe/richview-v3` name redirects to (observed 2026-10-21). A separate section, organised by branch, lists features that exist only on the nine open draft PR branches. Every ID is a proposal until Daniel answers the parts.

**Method.** I read the code at `b4db233` and probed a local release build of that commit on 2026-10-21: CLI runs, a loopback `richview server`, and MCP `initialize` plus `tools/list`. I sent only read-only GET requests to the public hosts on 2026-10-21, and read PR bodies, branch logs and diffs with `gh` and `git`. I did not run Cargo tests, browser automation, PDF export, asset upload, the VS Code extension or the reel exporter, and sent no POST to the interest endpoint (see Gaps). Paths are relative to the repository at `b4db233` unless a branch is named; `path:N` means line N.

**Confidence.**
- `observed`: verified in code at the cited line or in the 2026-10-21 run.
- `stated`: written in a document, PR body or comment, but not verified.
- `inferred`: my reading across several sources.
- `observed; stated`: the feature's existence is verified in code, but its detailed rules come from the cited document.

In the tables, "run" means the local probe of the `b4db233` build on 2026-10-21, and "live" means a public GET request on 2026-10-21.

## Component index

| ID | Component | Paths | Responsibility | Confidence |
| --- | --- | --- | --- | --- |
| `C-RICHVIEW-CHART` | Chart engine | `richview-chart/` | Chart math, SVG geometry, readability diagnostics, the calendar time-series engine, the browser chart-view API and the `richview-chart` CLI, shared by the native and Wasm hosts. | observed |
| `C-RICHVIEW-CORE` | Core | `crates/richview-core/` | Pure `.rmd` parser, validator and HTML renderer: rich blocks, style cascade, fonts, motion CSS and the `learn` knowledge bundle. Performs no filesystem or network I/O (`crates/richview-core/src/lib.rs:1-9`). | observed |
| `C-RICHVIEW-APP` | Native application | `crates/richview-app/` | Native host with the binaries `richview` and `richview-server`. It groups the seven sub-components below. | observed |
| `C-APP-CLI` | CLI | `crates/richview-app/src/cli.rs` | The `richview` command set. | observed |
| `C-APP-RESOLVER` | Resolver and exporter | `crates/richview-app/src/resolver.rs` | Inventories report folders, resolves local data, styles and media, and writes `Rendered/` output. | observed |
| `C-APP-PDF-EXPORT` | PDF printer | `crates/richview-app/src/export_pdf.rs` | Prints a rendered report to PDF with a local Chromium-family browser. | observed in code, not run |
| `C-APP-HTTP-SERVER` | HTTP server | `crates/richview-app/src/server.rs` | Axum host for the render, publish, read, preview and static-site routes. | observed |
| `C-APP-REPORT-STORE` | Report store | `crates/richview-app/src/storage.rs` | Append-only filesystem store of published versions. | observed |
| `C-APP-MCP` | MCP server | `crates/richview-app/src/mcp.rs` | MCP over stdio JSON-RPC, with seven tools and two resources. | observed |
| `C-APP-ASSET-UPLOAD` | Asset uploader | `crates/richview-app/src/asset_upload/` | Uploads data and media to an S3-API bucket and prints integrity-pinned references. | observed in code, not run |
| `C-RICHVIEW-WEB` | Browser host | `crates/richview-web/`, `assets/` | Rust compiled to Wasm (chart interaction, playground, landing, showcase, reel and the JS API) plus the static site assets. | observed |
| `C-WEB-WASM-API` | Wasm JS API | `crates/richview-web/src/browser.rs:67-517` | JS-callable render, parse, check, chart and style functions, the settle hook and page boot. | observed |
| `C-WEB-CHART-HOST` | Live chart chrome | `crates/richview-web/src/browser/charts.rs`, `crates/richview-web/src/browser/charts/` | Mounts live chart views and provides their controls, actions and expansion. | observed in code |
| `C-WEB-PLAYGROUND` | Playground | `assets/playground.html`, `crates/richview-web/src/browser/authoring.rs`, `crates/richview-web/src/source_buffer.rs` | In-browser `.rmd` editor with live preview at `/playground` and `/app`. | observed |
| `C-WEB-LANDING` | Landing page | `assets/landing/`, `crates/richview-web/src/browser/homepage.rs`, `crates/richview-web/src/browser/landing.rs` | Homepage with the live two-file demo, the update-signup form and the scroll stage. | observed |
| `C-WEB-CHART-EXAMPLES` | Chart examples page | `richview-chart/web/` | `/charts/`: three large time-series examples with an inline chart editor. | observed |
| `C-WEB-SHOWCASE` | Showcase | `assets/showcase/`, `crates/richview-web/src/browser/showcase.rs` | The flagship report under 23 visual treatments, with compare, tour, style drawer and citations. | observed |
| `C-WEB-REEL` | Reel | `assets/reel/`, `crates/richview-web/src/reel.rs`, `crates/richview-web/src/browser/reel.rs`, `crates/richview-web/src/bin/richview-reel.rs` | A 15-second animated product reel, its player and its frame/soundtrack exporter. | observed in code |
| `C-DOCS-SITE` | Documentation site | `docs/public/`, `crates/richview-build/src/documentation.rs`, `assets/docs/docs.css` | `/docs/` guides rendered by RichView itself. | observed |
| `C-RICHVIEW-EDITOR` | Editor server | `crates/richview-editor/` | Native JSON-RPC language and preview server for editors. | observed in code |
| `C-VSCODE-EXTENSION` | VS Code extension | `packages/vscode/` | Thin adapter (`richview-vscode` 3.0.0-alpha.1, publisher `richview`) over the editor binary. | observed in code, not run |
| `C-RICHVIEW-BUILD` | Build orchestrator | `crates/richview-build/` | Builds the binaries, Wasm, npm package contents, the staged website `dist/web`, the docs, the flagship and the review pages. | observed |
| `C-RICHVIEW-BENCH` | Benchmark | `crates/richview-bench/` | Benchmarks native render latency and peak memory. | observed in code |
| `C-PKG-CORE` | npm `@richview/core` | `packages/core/` | Generated JS package over the Wasm core, version 3.0.0-alpha.0. | observed (manifest) |
| `C-PKG-CHARTS` | npm `@richview/charts` | `packages/charts/` | Generated JS chart package that depends on core. | observed (manifest) |
| `C-CLOUDFLARE-TEASER` | Cloudflare Worker | `cloudflare/` | Worker `richview-teaser`: serves `dist/web` on richview.ai and stores update-signup consent in D1. | observed |
| `C-SKILL` | Agent skill | `skill/SKILL.md`, `skill/reference/anti-slop.md` | Instructions for agents that write and publish RichView reports. | observed |
| `C-LLMS-TXT` | llms.txt | `llms.txt` | Short product summary for agents. | observed |
| `C-EXAMPLES` | Example reports | `examples/`, `assets/landing/demo/` | The flagship report, tutorial reports and the homepage demo source. | observed |
| `C-CONTAINER-DEPLOY` | Container and Fly config | `Dockerfile`, `packages/server/fly.toml` | Native server image and Fly app definition. | observed |
| `C-CI` | CI and release artifacts | `.github/workflows/ci.yml`, GitHub releases | Push and pull-request gates, plus one prerelease. | observed |
| `C-AGENT-HARNESS` | Agent definitions (tooling, not product) | `.claude/agents/` | An orchestrator plus eight specialists (`richview_sub_core`, `_cli`, `_server`, `_vscode`, `_coder`, `_review`, `_qa`, `_tester`). The definitions use Cargo commands and were last changed in 92732c4 (2026-10-11). | observed |

The branch-only components are indexed in their PR sections: `C-VOICE-DASHBOARD` (#64), `C-GALLERIES` (#65), `C-APP-CONNECTORS` (#66), `C-DEV-HARNESS` (#68) and `C-RICHVIEW-PUBLISH` (#70).

## C-RICHVIEW-CHART: chart engine

**Responsibility.** The README describes the crate as "RichView's shared Rust chart renderer. Native reports, the CLI and the browser use the same chart math and SVG geometry" (`richview-chart/README.md:3`). The crate is version 0.1.0, MIT-licensed and `publish = false` (`richview-chart/Cargo.toml`). The workspace pins Rust 1.99 and edition 2024 (`Cargo.toml`). Observed.

**Public surface.**
- `.rmd` entry point: `render_rmd_chart(chart, resolved_rows, width, resolved_theme_json)` (`richview-chart/README.md:26`).
- Rendering functions: `render_svg` and `Info` (`richview-chart/src/lib.rs:59-96`).
- Temporal JSON `Spec`, with the fields `title`, `description`, `data`, `time`, `y`, `series`, `panes`, `transform`, `view`, `notes`, `annotations`, `interaction`, `source`, `demo` and `aspect` (`richview-chart/src/spec.rs:11-51`).
- Browser views: `ChartView(host, payload_json)` and `mount_chart`, exposing `showPreset`, `setTransform`, `setZero`, `toggleSeries`, `relayout`, `toSVG`, `update` and `destroy` (`richview-chart/README.md:56`).
- Events and probes: `rvc:mount`, `rvc:diagnostics`, `rvc:frame`, `measure_chart(host)`, `runtime_stats()` and `Chart::source_csv()` (`richview-chart/README.md:30,58,68,72`).
- Binary `richview-chart` with `validate`, `render` and `from-rmd` (`richview-chart/src/bin/richview-chart.rs`).
- About 31 contract test files (`richview-chart/tests/`).

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-CHART-LINE` | Line chart | `type: line`. X is categorical, numeric or ISO dates; when every X value is a calendar string, the chart goes through the calendar engine. `style.line.smoothing` takes `straight`, `monotone` (the default, with no overshoot) or `current` (0.75-strength handles). The alternative `style.curve` takes `monotone`, `linear`, `step`, `step_before` or `step_after`, and `style.curvature` runs from 0 to 1. Direct end labels fall back to a legend when complete names do not fit. | `crates/richview-core/src/parser.rs:308-327`; `docs/RMD-SPEC.md:92-94,129,148`; `richview-chart/README.md:36-38` | observed; stated |
| `F-CHART-AREA` | Area chart | `type: area`. Shares the line chart's curve controls and keeps zero on the value axis. The temporal engine adds stacked areas, panes and negative fill colours. | `docs/RMD-SPEC.md:148`; `richview-chart/README.md:34` | observed; stated |
| `F-CHART-BAR` | Bar chart | `type: bar` with `style.orientation` set to `auto` (default), `vertical` or `horizontal`. Bars have a flat baseline and optional rounding at the value end from the theme's `chart.barRadius`, capped at 15% of thickness. Each category/series slot takes one finite value. Repeated observations reject the panel with `chart.ambiguous-bar-slot`. | `docs/RMD-SPEC.md:91,96,150` | observed; stated |
| `F-CHART-SCATTER` | Scatter chart | `type: scatter` with an optional `encoding.label` for point names. Authored annotations take priority, then sparse points; dense names appear only when collision-free. A one-signed percentage axis is trimmed to zero only when every point keeps 4 px of clearance. | `docs/RMD-SPEC.md:85,174` | observed; stated |
| `F-CHART-BUBBLE` | Bubble sizing | A scatter with `encoding.size` draws areas proportional to finite non-negative values, with a 16 px radius for the largest positive value. Zero has no area but stays inspectable. Invalid sizes omit the bubble and produce diagnostics. An outlined 16 px reference circle carries the field name and exact value. | `docs/RMD-SPEC.md:84,170-172` | observed; stated |
| `F-CHART-PIE` | Pie chart | `type: pie`. Labels sit around the disk when complete names and percentages fit; otherwise a wrapped key is used. Categories are never combined or abbreviated. Hovering a key entry inspects the slice. Point and text annotations anchor to named slices; Cartesian annotation kinds produce a diagnostic. | `docs/RMD-SPEC.md:139`; `richview-chart/README.md:26` | observed; stated |
| `F-CHART-HEATMAP` | Heatmap | `type: heatmap`, where `encoding.color` carries the cell value and `axes.color` formats it. Colour comes from the theme's three-stop OKLCH `chart.ramp`, and there is no colour bar. Missing markers render as neutral `n/a` cells. Duplicate coordinates reject the panel. Rows are at least 1.6 lines tall, and the grid transposes when that lets every column be named. | `docs/RMD-SPEC.md:83,88,154-160,166` | observed; stated |
| `F-CHART-HISTOGRAM` | Histogram | `type: histogram` with a numeric `encoding.x`. `binning` accepts `count` (1-256, defaulting to Sturges), `extent`, `edges` (2-257) and `measure` (`count`, `proportion` or `density`). The chart is vertical only and the measure axis starts at zero. Annotations use the derived measure coordinate. | `docs/RMD-SPEC.md:102-115` | observed; stated |
| `F-CHART-BOXPLOT` | Box plot | `type: boxplot` with a numeric `y` and an optional categorical `x`, `color` and `facet`. Quartiles use R-7. Whiskers end at the most extreme observed values inside the 1.5×IQR fences, and outliers are drawn individually. Exclusions are counted per reason. | `docs/RMD-SPEC.md:117-121` | observed; stated |
| `F-CHART-INTERVAL` | Interval chart | `type: interval` with a categorical `x`, required `encoding.lower` and `encoding.upper`, and an optional estimate in `y`. Reversed bounds and duplicate category/group pairs are errors. No confidence level or distribution is inferred. | `docs/RMD-SPEC.md:123-127`; `richview-chart/README.md:28` | observed; stated |
| `F-CHART-FACETS` | Small multiples | `encoding.facet` splits the chart into panels, each with its own child runtime, Table and CSV. Category colours are shared across panels. `toSVG()` exports the whole grid in its current state. | `docs/RMD-SPEC.md:86,135`; `richview-chart/README.md:60` | observed; stated |
| `F-CHART-ANNOTATIONS` | Annotations | Kinds are `point` and `text` (both need text; text accepts `rotate`), plus `range`, `line` and `band`. Labels search around their anchors with clearance from marks and existing labels, and placement never moves data. Optional text that cannot be placed is omitted with provenance. | `docs/RMD-SPEC.md:180-188` | observed; stated |
| `F-CHART-AXES` | Axis formatting | `axes.x` and `axes.y` take `label`, `format` and `suffix`. `axes.x.locale` accepts `en-US` (default), `en-GB`, `cs-CZ`, `de-DE` or `ja-JP`; `axes.y.min` and `axes.y.max` fix the value range. Formats cover currency, percent (×100, one `%`) and compact numbers, with scientific notation past the thresholds. Value axes end on labelled ticks. Bars, areas and percentage axes keep zero. | `crates/richview-core/src/parser.rs:391-399`; `docs/RMD-SPEC.md:87,144,148,176-178` | observed; stated |
| `F-CHART-CATEGORY-LABELS` | Category label fitting | Category names are never ellipsised or silently dropped. The layout tries one line first, then a balanced wrap of at most two lines, then every k-th label for ordered categories such as years, quarters, months, days or numbered names. | `docs/RMD-SPEC.md:146` | stated |
| `F-CHART-RATIO` | Responsive aspect ratio | Charts have no pixel height. `style.ratio` takes a number or `W:H`. The default is 16:9 for line, area, bar, heatmap, histogram, boxplot and interval charts, 4:3 for scatter and 1:1 for pie. The SVG scales to the column. | `docs/RMD-SPEC.md:90,100` | observed; stated |
| `F-CHART-PALETTES` | Categorical palettes | Each builtin palette has eight OKLCH colours. The first four reach 3:1 against the chart surface, and neighbouring colours differ by ΔE ≥15 for normal vision and ≥8 under protan, deutan and tritan simulation. `style.palette` names a palette; a style's `chart.palette` can be a name or an array. | `richview-chart/src/rmd_render.rs:1182-1189`; `crates/richview-core/tests/palettes.rs`; `docs/RMD-SPEC.md:89,168` | observed |
| `F-CHART-DECALS` | Hatch decals | When a style sets `chart.decals`, bars, their legend swatches and pie sectors use six repeating hatch patterns as a secondary cue. | `docs/RMD-SPEC.md:137,142` | stated |
| `F-CHART-MISSING-DATA` | Visible gaps | Missing observations break lines and areas with butt caps. A break narrower than 3 px gets a 1 px rug mark (`rvc-gap`), and a gap diagnostic counts these marks. Heatmap missing values show as `n/a`. | `docs/RMD-SPEC.md:152,154` | stated |
| `F-CHART-TIMESERIES-SPEC` | Calendar time-series engine | The JSON spec takes `data` (CSV text, rows or columns), `time.field`, and either `series` or `panes`. Marks are line, area or step, with stacks, negative fills, exact gaps and direct labels. Axes support scaling, units, percent and currency, bounds and a secondary conversion of the same quantity. Time values can be ISO dates, UTC offsets or declared Unix s/ms. Duplicate timestamps are rejected, and gaps follow the typical spacing or `max_gap`. | `richview-chart/README.md:32-44`; `richview-chart/src/spec.rs:11-51` | observed; stated |
| `F-CHART-TRANSFORMS` | Rebase and year-over-year | `TransformKind` is `None`, `Rebase` or `Yoy`. Rebase uses the preceding valid observation, YoY compares calendar dates, and missing bases stay missing. These transforms are reachable only from JSON specs: the `.rmd`-to-spec bridge sets no transforms, ranges or navigator. | `richview-chart/src/spec.rs:206-214`; `richview-chart/src/rmd.rs:254-272`; `richview-chart/README.md:40` | observed |
| `F-CHART-DECIMATION` | Dense-series reduction | Large temporal views use disclosed per-pixel mean/range aggregation, and smaller views use M4 first/last/min/max sampling. Exact observations stay separate for hover and tables. | `richview-chart/README.md:40`; `docs/RMD-SPEC.md:192` | stated |
| `F-CHART-EXACT-READOUTS` | Exact readouts | Hover and Table keep the retained decimal source text, rounding half to even. Geometry and transforms use binary64. | `richview-chart/README.md:48-50`; `docs/RMD-SPEC.md:194-196` | stated |
| `F-CHART-SOURCE-CSV` | Original-data CSV | `Chart::source_csv()` exports the original observations regardless of sorting, unit scaling or transforms. Authored CSV keeps its bytes. | `richview-chart/README.md:72` | stated |
| `F-CHART-NATIVE-DIAGNOSTICS` | Native readability checks | Native diagnostics report emitted mark geometry, estimated label heights and SVG-constrained label widths, and record the provenance of omitted labels. A report adds the info `chart.font-bounds-estimated`, or the warning `chart.unmeasured` when there is no geometry. | `richview-chart/README.md:30`; `crates/richview-core/src/render.rs:700-712`; run (info diagnostic present) | observed |
| `F-CHART-BROWSER-MEASUREMENT` | Browser glyph measurement | After layout and fonts settle, the browser measures real glyph rectangles, stores them in `data-rvc-diagnostics` and dispatches `rvc:diagnostics`. `measure_chart(host)` returns the findings. | `richview-chart/README.md:30` | stated |
| `F-CHART-INTERACTION-POLICY` | Declared interaction policy | `InteractionSpec` declares `hover`, `zoom` (default true), `ranges`, `navigator`, `transforms` and `zero_toggle`. An `.rmd` temporal chart gets zoom and hover; its overview appears only after a zoom. | `richview-chart/src/spec.rs:256-278`; `richview-chart/src/rmd.rs:270-271` | observed |
| `F-CHART-BROWSER-API` | Chart view API | `ChartView`/`mount_chart` take a `time-series`, `rmd` or `facets` payload. Views update keyed SVG nodes in place. `rvc:mount` fires before the first paint, `rvc:frame` reports layout and DOM-write time, and `runtime_stats()` and `destroy` support lifetime checks. Invalid updates leave the existing view intact. | `richview-chart/README.md:56-68` | stated |
| `F-CHART-CLI` | `richview-chart` binary | `validate <spec.json>` reports JSON-pointer paths with hints. `render <spec.json> --width N` writes SVG. `from-rmd chart.json --csv data.csv` writes a spec. The native CLI resolves `data.url` relative to the spec. | `richview-chart/README.md:16-24`; `richview-chart/src/bin/richview-chart.rs` | observed in code |

## C-RICHVIEW-CORE: parser, renderer and styles

**Responsibility.** Core turns `.rmd` source plus already-resolved data and styles into one self-contained HTML document with diagnostics. It must stay importable in the browser and perform no I/O (`crates/richview-core/src/lib.rs:1-9`; `docs/RMD-SPEC.md:5`). Observed.

**Public surface.**
- Entry points: `parse`, `render`, `validate_publish`, `validate_report_folder` and `report_relative_path`, plus the compatibility exports (`crates/richview-core/src/lib.rs:27-51`).
- `RenderOptions`: `width` (720), `runtime_js`, `runtime_url`, `runtime_wasm`, `codec` (raw or gzip), `theme`, `report_card`, `output` and `page` (`crates/richview-core/src/lib.rs:119-165`).
- `Diagnostic`: `level`, `code`, `message`, `path`, `line?`, `tag?` and `details?` (`crates/richview-core/src/lib.rs:58-71`).
- Bundled assets: five style YAMLs, font CSS, `font-manifest.json`, 11 font licences, `report.css` and `print.css` (`crates/richview-core/assets/`).

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-CORE-PARSE` | Rich Markdown parsing | Strips a BOM and reads frontmatter from `---` to a closing `---` or `...`. Fences are recognised by CommonMark rules, including longer fences. Rich fences must be YAML mappings. Unknown tags stay as ordinary code blocks. An invalid rich fence stays visible as code and gets a diagnostic. | `crates/richview-core/src/parser.rs:14-171`; `docs/RMD-SPEC.md:9,52-56` | observed |
| `F-CORE-FRONTMATTER` | Document metadata | Fields: `title` (draft fallback "Untitled report"), `description` (rendered as the lede), `version`, `subtitle`, `author`, `date`, `slug`, `tags`, `theme` (`auto` by default, or `light`/`dark`), ordered `styles`, a title image (`title_image`, `title-image` or `image`) and `prolog`. Unknown keys are preserved. Local drafts need no metadata. | `crates/richview-core/src/parser.rs:205-266`; `docs/RMD-SPEC.md:38-50`; `crates/richview-core/src/render.rs:228-274` | observed |
| `F-CORE-NO-HEADER-DATA` | Header data removed | A frontmatter `data:` block is a hard `frontmatter.data` error, and the migration message says to move data into each block. | `crates/richview-core/src/parser.rs:265`; `docs/RMD-SPEC.md:47,265` | observed |
| `F-CORE-RICH-TAGS` | Nine rich fence tags | The tags are `chart`, `table`, `big`, `metric`, `callout`, `image`, `embed`, `audio` and `video`. `metric` is an alias of `big`. | `crates/richview-core/src/parser.rs:8-10`; `crates/richview-core/src/blocks.rs:210` | observed |
| `F-CORE-CHART-VALIDATION` | Chart fence schema | Checks the nine chart types, a `data` mapping, string channels (`x`, `y`, `lower`, `upper`, `color`, `size`, `label`, `facet`), the axis locale, and the palette, curve, smoothing, curvature, orientation and ratio values. A chart with no title still renders, with a fallback title and a `block.title` warning. | `crates/richview-core/src/parser.rs:139-141,305-400` | observed |
| `F-CORE-BLOCK-DATA` | Block-local data | Each chart or table `data` must use exactly one of `inline`, `csv` or `json`; otherwise it fails with a `data.local` error. `data.id`, `data.url` and `data.sql` are also errors. Hosts resolve `csv` and `json` files into `RenderOptions.data`. | `crates/richview-core/src/parser.rs:689-693`; `docs/RMD-SPEC.md:198-236` | observed |
| `F-CORE-STATIC-CHART-FRAMES` | Static phone and desktop frames | Each chart is rendered natively twice: at a 358 px phone column and at the theme's desktop column (`layout.report-max-width`, default 980, minus 48). CSS shows the nearer frame, split at the geometric mean of the two widths. Charts are visible without JavaScript. | `crates/richview-core/src/render.rs:127,714-745` | observed |
| `F-CORE-TABLE` | Table block | `table` with `data`, `columns`, `sort`, `total` and `max_rows` (default 20). Output has sortable header buttons (`data-rv-sort`), a totals row and overflow rows inside `<details>`. A JSON payload lets the browser re-render, and print shows every row. Cells show the host for URLs, plus date and id classes. Table authoring style (row mappings or alternatives) is still pending under S27-580. | `crates/richview-core/src/blocks.rs:354-471,577+`; `docs/RMD-SPEC.md:269-296`; `docs/TABLE-AUTHORING-CHOICE.md` | observed |
| `F-CORE-BIG-NUMBER` | Big number / metric | `big` or `metric` with `value`, `unit`, `delta` (shown as %), `trend` (`up`, `down` or `flat`) and `format` (`currency`, `currency_compact`, `percent`, `number` or `number_compact`). | `crates/richview-core/src/blocks.rs:241-337`; `crates/richview-core/src/parser.rs:607` | observed |
| `F-CORE-CALLOUT` | Callout | `callout` requires `title`. `kind` is `up`, `down`, `neutral` (default) or `hold`. | `crates/richview-core/src/parser.rs:296,615-618`; `crates/richview-core/src/blocks.rs:211-232` | observed |
| `F-CORE-MEDIA` | Image, audio and video | Fields: `src`, `width`, `height`, `fit` (`contain`, `cover` or `fill`), `poster` and `caption`. Unsafe sources are blocked with `security.media`. Network-dependent and external references get `assets.network-dependent` and `assets.external-reference` diagnostics. PDF output prints the poster or a text card. | `crates/richview-core/src/blocks.rs:648-764`; `crates/richview-core/src/parser.rs:640-643` | observed |
| `F-CORE-EMBED` | Sandboxed embed | HTTPS iframe with a default host allowlist (youtube.com, youtube-nocookie.com, vimeo.com, gist.github.com and codepen.io). An author `allow:` list replaces that default. Only default hosts get `allow-same-origin`. Height defaults to 420. A blocked URL renders a notice and a `security.embed` warning. PDF output shows a link card. | `crates/richview-core/src/blocks.rs:789-869` | observed |
| `F-CORE-MARKDOWN` | Markdown extensions | Tables, strikethrough, task lists, footnotes, definition lists, math as MathML and smart typography. | `crates/richview-core/src/markdown.rs:11-16,474-493`; `docs/RMD-SPEC.md:325-334` | observed |
| `F-CORE-WIKILINKS-QUOTES` | Wikilinks and pull quotes | `[[slug\|label]]` links to `/r/<slug>`. A `[!quote]` block becomes a pull quote with an author. | `crates/richview-core/src/markdown.rs:496-530` | observed |
| `F-CORE-SOURCE-NOTES` | Block source notes | An emphasised "Source:" or "Note:" paragraph directly after a block becomes that block's `rv-source` note. | `crates/richview-core/src/markdown.rs:385-438` | observed |
| `F-CORE-CODE-HIGHLIGHT` | Code highlighting | Highlights Rich Markdown, YAML, shell and plain text. | `crates/richview-core/src/highlight.rs:10-30` | observed |
| `F-CORE-HTML-DOCUMENT` | Self-contained HTML report | One HTML file with a header (title, subtitle, lede, title image, prolog), the body and a footer reading "Made with RichView" that links to https://richview.ai. `metadata._richview` records `generator`, `source`, `block_count`, `chart_count`, `status`, `offline`, `interactive` and `chart_diagnostics`. | `crates/richview-core/src/render.rs:228-274,353,373` | observed |
| `F-CORE-RUNTIME-EMBED` | Embedded interactive runtime | The Wasm runtime is embedded as base64 (raw, or gzip unpacked with `DecompressionStream`), or referenced through `runtime_url` for previews. Run: a three-block report rendered to 2,314,593 bytes with `interactive: true`. | `crates/richview-core/src/render.rs:888+`; `crates/richview-core/src/lib.rs:119-165`; run | observed |
| `F-CORE-REPORT-CARD` | Render report card | An optional `<aside>` (enabled by `report_card` or `--report-card`, and always on in editor previews) with status `clean` or `issues`. It shows "N charts checked against emitted geometry" (plus "font bounds estimated" when relevant) or "N findings across M charts and report source", followed by a ledger of the diagnostics. | `crates/richview-core/src/render.rs:857-887` | observed |
| `F-CORE-STYLE-CASCADE` | Ordered style cascade | An ordered list of builtin names or loaded `.rstyle.yaml` sources is merged shallowly, last one wins. Tokens and layout become `--rv-*` custom properties plus component CSS. Caller styles come after frontmatter styles. With no style at all, the default is `brutalist`. | `crates/richview-core/src/style.rs`; `crates/richview-core/src/render.rs:104`; `docs/RMD-SPEC.md:249-264,335-354` | observed |
| `F-CORE-BUILTIN-STYLES` | Builtin styles | `sleek` (dark), `sleek-dark` (alias of `sleek`), `sleek-light`, `scientific`, `brutalist` and `hand-drawn`. `editorial` is deprecated and maps to `brutalist` with a warning. The style YAML headers name the fonts and intent (for example "studio's BASE template" for brutalist). | `crates/richview-core/src/style.rs:16-45,204-213`; `crates/richview-core/assets/*.rstyle.yaml` | observed |
| `F-CORE-FONTS` | Bundled fonts | Five bundled font catalogs are embedded natively. RichView Serif, Sans, Code and Mono derive from Source Serif 4, Source Sans 3, Source Code Pro and IBM Plex Mono. A missing face produces `style.fonts-unavailable`. | `crates/richview-core/src/style.rs:292,310-318`; `docs/public/styles.md:9` | observed |
| `F-CORE-MOTION` | Chart entrance motion | Data marks animate on entrance: a line sweeps in 700 ms, areas share the sweep, bars take 500 ms with a 40 ms stagger capped at 320 ms, pies 700 ms, scatter 400 ms and labels 200 ms. Report text never animates. `motion.enabled` is the only style key, retired keys produce warnings, and reduced motion renders the settled state. | `docs/MOTION.md:7,24-25,65` | stated |
| `F-CORE-PRINT` | Print styles | `print.css` handles printing. `sleek` and `sleek-dark` print as `sleek-light`, and tables print every row. | `crates/richview-core/src/style.rs:872`; `crates/richview-core/assets/print.css` | observed |
| `F-CORE-PDF-OPTIONS` | PDF render mode | `Output::Pdf` with page size `A4` or `Letter` and 20/18/22 mm margins. PDF output substitutes media posters, text cards and link cards. | `crates/richview-core/src/lib.rs:167-211`; `docs/RMD-SPEC.md:237-248` | observed |
| `F-CORE-DIAGNOSTICS` | Diagnostic contract | `{level, code, message, path, line?, tag?, details?}` with levels `error`, `warning` and `info`. Every CLI, server, MCP and editor surface returns this shape. | `crates/richview-core/src/lib.rs:58-71`; `docs/RMD-SPEC.md:355-365` | observed |
| `F-CORE-LIMITS` | Size limits | Source up to 16 MiB; rendered report up to 64 MiB. | `crates/richview-core/src/lib.rs:54-56` | observed |
| `F-CORE-PUBLISH-VALIDATION` | Publication checks | A missing `title` or a non-semver `version` is an error. A missing `description` is only a warning. | `crates/richview-core/src/parser.rs:715-751`; `docs/RMD-SPEC.md:9` | observed |
| `F-CORE-REPORT-FOLDER` | Report-folder contract | One `.rmd` per folder, including in nested folders. `report_relative_path` rejects escapes. Output goes to `Rendered/`. | `crates/richview-core/src/folder.rs:97-185`; `README.md:32` | observed |
| `F-CORE-LEARN` | Knowledge bundle | Sections `grammar`, `chart-contract`, `style`, `workflow` and `anti-slop`. Served by `richview learn` and `richview_learn`, and mirrored in `skill/` and `llms.txt`. | `crates/richview-core/src/knowledge.rs:11-19` | observed |
| `F-CORE-ASSET-REFERENCE` | Integrity-pinned asset reference type | `AssetRef {url, bytes, sha256, type, visibility, duration, width, height}` plus `AssetUse` allowlists. No report block consumes it yet. | `crates/richview-core/src/asset.rs`; `docs/public/publishing.md:59` | observed |
| `F-CORE-COMPAT-API` | Compatibility API | `render_compat`, `parse_schema` and `create_publish_request`. The publish request derives the slug from the title (for example "i-stanbul-grows"). | `crates/richview-core/src/compat.rs` | observed |
| `F-CORE-AUTHORING-TABLES` | Authoring vocabulary | `CHART_TYPES`, key and value completion tables, and `with_primary_style`. The editor and playground use them. | `crates/richview-core/src/authoring.rs:62,139-148` | observed |

## C-RICHVIEW-APP: native application

**Responsibility.** The native host owns all filesystem, network and process work that core must not do (`docs/RUST-ARCHITECTURE.md:17-31`; `crates/richview-app/README.md`). Inputs are capped at 32 MiB (`crates/richview-app/src/lib.rs:14`). `default_public_dir` locates the staged runtime and site, from the installed layout or from `dist/web` (`crates/richview-app/src/assets.rs:6-28`; `README.md:36`). Run: `richview --version` prints `3.0.0-alpha.0`, and most subcommands have empty help descriptions.

### C-APP-CLI

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-APP-CLI-GENERATE` | `richview generate` (alias `g`) | Takes `<input.rmd>` and writes `<report folder>/Rendered/<stem>.html`. `-o` names a file inside `Rendered`, and a `.pdf` name implies PDF. Flags: `--theme`, repeated `--style`, `--public-dir`, `--allow-network` (no effect), `--format pretty` or `json`, `--report-card`, `--to report` or `pdf`, `--page a4` or `letter`, `--browser`, `--settle-timeout 30`. Run: `Rendered/report.html`, 2,314,593 bytes. | `crates/richview-app/src/cli.rs:61-98`; run | observed |
| `F-APP-CLI-VALIDATE` | `richview validate` | Parses and validates without rendering; `--format json` is available. Run: `{"blocks":3,"diagnostics":[],"ok":true}`. | `crates/richview-app/src/cli.rs:99-103`; run | observed |
| `F-APP-CLI-CHECK` | `richview check` | Renders, then reports chart geometry, clipping, collisions and dropped-label provenance. `--chart-width` defaults to 720 and accepts 240-4096. Run: clean. | `crates/richview-app/src/cli.rs:104-109,310`; run | observed |
| `F-APP-CLI-INIT` | `richview init` | `init [dir] --name report` creates `<dir>/<name>.rmd` from a template ("My report turns in Q3", `styles: [sleek]`, a big number, a bar chart and a callout) and never overwrites. | `crates/richview-app/src/cli.rs:110-114,329-349,588-629`; run | observed |
| `F-APP-CLI-LEARN` | `richview learn` | `learn [section]` prints the knowledge bundle or one section. | `crates/richview-app/src/cli.rs:115-117,350-363` | observed |
| `F-APP-CLI-PUBLISH` | `richview publish` | Takes `<input>`, `--server` (env `RICHVIEW_SERVER`, default `https://richview.uk`), `--token` (env `RICHVIEW_TOKEN`) and `--slug`. Requires HTTPS except on loopback, follows no redirects, times out after 30 s and redacts the token. Sends `{slug, source, styles, data, report_folder}`. Run: the first publish returned `created: true`, a repeat returned `created: false`. | `crates/richview-app/src/cli.rs:119-128,428-504`; run | observed |
| `F-APP-CLI-SERVE` | `richview serve` | Live preview of one `.rmd` on `127.0.0.1:4711` (`-p`, `--bind`). Uses a preview store at `<cwd>/tmp/richview-preview` and has no publish token. The page reloads when the source changes (see `F-APP-SERVER-PREVIEW`). | `crates/richview-app/src/cli.rs:129-136,372-394`; `README.md:48` | observed |
| `F-APP-CLI-SERVER` | `richview server` / `richview-server` | Runs the HTTP host with `PORT` 8080, `RICHVIEW_BIND` 127.0.0.1, `RICHVIEW_DATA_DIR` `./data`, a token, `RICHVIEW_PUBLIC_DIR` and `RICHVIEW_PUBLIC_URL`. The public URL must be a bare http(s) origin. | `crates/richview-app/src/cli.rs:150-192` | observed |
| `F-APP-CLI-MCP` | `richview mcp` | Starts the stdio MCP server (`C-APP-MCP`). | `crates/richview-app/src/cli.rs:118` | observed |
| `F-APP-CLI-ASSET` | `richview asset` | `put`, `verify` and `abort` (`C-APP-ASSET-UPLOAD`). | `crates/richview-app/src/cli.rs:141-145`; `crates/richview-app/src/asset_upload/mod.rs:67-106` | observed |
| `F-APP-CLI-JSON-ERRORS` | Machine-readable failures | With `--format json`, a failure prints `{"ok":false,"error","diagnostics"}` and exits with code 1. | `crates/richview-app/src/cli.rs:195-216` | observed |

### C-APP-RESOLVER

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-APP-RESOLVE-REPORT-FOLDER` | Report-folder inventory | Inventories at most 10,000 entries and rejects a symlinked root. Every reference must stay inside the folder, including through symlinks (`report.asset.escape`). | `crates/richview-app/src/resolver.rs:37-38,311-387` | observed |
| `F-APP-RESOLVE-DATA` | Local data | Reads block `csv` and `json` files into render data, with a 64 MiB aggregate cap. `sql` is an error. | `crates/richview-app/src/resolver.rs:12,70-184` | observed |
| `F-APP-RESOLVE-MEDIA` | Local media | Resolves images, audio, video, posters, and callout, prolog and body images, then embeds them. | `crates/richview-app/src/resolver.rs:70-184` | observed |
| `F-APP-RESOLVE-STYLES` | Style files | Frontmatter `styles` and `--style` paths resolve inside the report folder; builtin names pass through. Run: `--style ../x.rstyle.yaml` and an absolute path both fail with `report.asset.escape`. | `crates/richview-app/src/resolver.rs:37-38`; run | observed |
| `F-APP-RESOLVE-NETWORK` | Remote loading (documented, absent) | `--allow-network` is accepted but its parameter is unused (`_allow_network`) since dd61040 (PR #15, 2026-10-05). Run: an `https://` CSV with `--allow-network` fails with `report.asset.escape`. | `crates/richview-app/src/resolver.rs:204,249,272`; run | observed |
| `F-APP-EXPORT-RENDERED` | `Rendered/` export | Writes the HTML into `Rendered/` and copies the local assets it needs. Fails with `report.output.runtime` when the report has charts but the runtime is missing; there is no static-only fallback. | `crates/richview-app/src/resolver.rs:555-673`; `crates/richview-app/tests/installed_assets.rs:296-300` | observed |

### C-APP-PDF-EXPORT

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-APP-PDF-EXPORT` | Print to PDF | `--to pdf` (or the MCP `output: pdf`) drives a local Chromium, Chrome or Edge ≥131 (`--browser` or `RICHVIEW_BROWSER`). It uses a CDP pipe, a temporary profile and blocked network, and waits for `window.__richview.settled()` (or, without it, for fonts and every chart to settle). Timeouts are 15 s for launch, 30 s for settling (configurable) and 60 s for printing. Output is `Rendered/<name>.pdf` in A4 or Letter. Not run. | `crates/richview-app/src/export_pdf.rs:7,16-20,93,111-113,232`; `docs/RMD-SPEC.md:237-248`; `docs/public/publishing.md:21-31` | observed in code |

### C-APP-HTTP-SERVER

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-APP-SERVER-HEALTH` | Health check | `GET /health` returns `{"ok":true}`. Run: 200. | `crates/richview-app/src/server.rs:64-95`; run | observed |
| `F-APP-SERVER-RENDER-API` | Render API | `POST /api/render` takes `{source, report_folder (required), styles, data, theme}` and returns `{html, has_charts, diagnostics, renderDiagnostics, metadata}`. Errors: 400 without a source, 422 when rendering fails. The runtime is always embedded. Run: without `report_folder`, 422 `report.root.missing`; `# hi` with a folder rendered 2,366,305 bytes. | `crates/richview-app/src/server.rs:149-218`; run | observed |
| `F-APP-SERVER-PUBLISH-API` | Publish API | `POST /api/reports` with a bearer token. Returns 503 when no server token is configured and 401 for a wrong token (constant-time comparison). Requires a title and a semver version, renders server-side, stores the version and returns `{ok, slug, version, url, created, permalink}`. Run: 401 without a token, 503 on a tokenless server, success with a token. | `crates/richview-app/src/server.rs:220-324`; run | observed |
| `F-APP-SERVER-READ-API` | Report listing API | `GET /api/reports` returns a list of `{slug, version, frontmatter, updated_at}`. Also `/api/reports/{slug}`, `/api/reports/{slug}/versions` and `/api/reports/{slug}/v/{version}`. All reads are public. | `crates/richview-app/src/server.rs:64-95`; `crates/richview-app/src/storage.rs:176-290`; `docs/public/publishing.md:128` | observed |
| `F-APP-SERVER-REGENERATE` | Regenerate endpoint | `POST /api/reports/{slug}/v/{version}/regenerate` re-renders the stored source, folder, styles and data with the current engine and returns the render JSON. It writes nothing and has no authentication check. Run: 200 without a token. | `crates/richview-app/src/server.rs:401-424`; `crates/richview-app/tests/native_contract.rs:165-179,262-271`; run | observed |
| `F-APP-SERVER-REPORT-PAGES` | Published report pages | `GET /r/{slug}` serves the latest version with `Cache-Control: no-cache`. `GET /r/{slug}/v/{version}` is immutable (`public, max-age=31536000, immutable`). Run: both 200. | `crates/richview-app/src/server.rs:426-448`; run | observed |
| `F-APP-SERVER-RAW-SOURCE` | Raw source | `GET /raw/{slug}` and `/raw/{slug}/v/{version}` return `text/markdown`. Run: 200. | `crates/richview-app/src/server.rs:64-95,438-443`; run | observed |
| `F-APP-SERVER-STATIC-SITE` | Static site and aliases | Serves the public directory. `/` maps to `index.html`, `/app` and `/playground` to `playground.html`, and `/showcase` to `showcase.html`, which is never built. Runtime files (`richview_web.js`, `richview_web_bg.wasm`, `bootstrap.js`) get `Access-Control-Allow-Origin: *`. Run: `/`, `/playground`, `/app`, `/charts/`, `/docs/`, `/reel/` and `/showcase/openai-vs-anthropic/` return 200; `/showcase` and `/showcase/` return 404. | `crates/richview-app/src/server.rs:596-630`; run | observed |
| `F-APP-SERVER-PREVIEW` | Preview revision and reload | In `serve` mode, `GET /__revision` and the preview render routes back the live-reload page, which polls every 500 ms. Without a preview the route returns 404 (run). | `crates/richview-app/src/server.rs:478-569`; `crates/richview-web/src/browser/authoring.rs:1237-1282`; run | observed |
| `F-APP-SERVER-LEGACY-REDIRECT` | therichview.com redirect | A `Host` of `therichview.com` or `*.therichview.com` gets a 301 to `https://richview.uk` plus the path. Run: `/r/x` redirected to `https://richview.uk/r/x`. | `crates/richview-app/src/server.rs:107-122`; run | observed |
| `F-APP-SERVER-HARDENING` | Limits and headers | 32 MiB body limit. gzip/br compression, except for `application/gzip`. Headers `x-richview`, `X-Content-Type-Options: nosniff` and `Referrer-Policy`. | `crates/richview-app/src/server.rs:64-133` | observed |

### C-APP-REPORT-STORE

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-APP-STORE-IMMUTABLE-VERSIONS` | Immutable versions | Each version is stored as `<data>/<slug>/<version>/{source.rmd, rendered.html, meta.json, assets.json}` and is idempotent per `{slug, version}`: a repeat returns `created: false` and keeps the first content. Writes use an in-process mutex, an fs2 `.publish.lock` and a staging rename. Run: matches this layout. | `crates/richview-app/src/storage.rs:56-118`; run | observed |
| `F-APP-STORE-LATEST-POINTER` | Latest pointer | `latest.txt` is set to the newly created version even when its semver is lower (publication order, not highest semver). | `crates/richview-app/src/storage.rs:98-100`; `docs/public/publishing.md:126` | observed |
| `F-APP-STORE-RETENTION` | Version retention | Keeps the new version plus the 49 highest-semver others, so at most 50 per slug, and deletes the rest. | `crates/richview-app/src/storage.rs:13,101-111,245-268` | observed |
| `F-APP-STORE-IDENTIFIERS` | Slug and version rules | A slug is 1-80 characters from `[a-z0-9-_]`. A version is at most 64 bytes, semver-shaped, with no path separators. | `crates/richview-app/src/storage.rs:290-311` | observed |

### C-APP-MCP

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-APP-MCP-TRANSPORT` | MCP stdio server | JSON-RPC over stdio with a 32 MiB limit. Protocol 2025-06-18 is the default. An instructions string is sent on `initialize`, and tools carry annotations: `validate` and `check` are read-only, `publish` is open-world. Run: `tools/list` returns seven tools. | `crates/richview-app/src/mcp.rs:7-46,294-368`; run | observed |
| `F-APP-MCP-RENDER` | `richview_render` | Input is `source`, `input_path` or a `report_folder` inventory, plus `theme`, `styles`, `data`, `output_path`, `output` (`report` or `pdf`) and `page`. Writes to `Rendered/` and returns the path and diagnostics. A materialised render requires `input_path`. | `crates/richview-app/src/mcp.rs:8-17,90-292` | observed |
| `F-APP-MCP-VALIDATE` | `richview_validate` | Validates structure and rich-block contracts without rendering. | `crates/richview-app/src/mcp.rs:18-22` | observed |
| `F-APP-MCP-CHECK` | `richview_check` | Renders and inspects chart geometry with `chart_width` 240-4096. Readability findings count as success; only hard failures set `isError`. | `crates/richview-app/src/mcp.rs:24-28` | observed |
| `F-APP-MCP-PUBLISH` | `richview_publish` | `server` is required (there is no default), with optional `token` and `slug`. Publishes an immutable version and returns its URL. | `crates/richview-app/src/mcp.rs:30-34` | observed |
| `F-APP-MCP-LEARN` | `richview_learn` | `section` is one of `grammar`, `chart-contract`, `style`, `workflow` or `anti-slop`. | `crates/richview-app/src/mcp.rs:43` | observed |
| `F-APP-MCP-ASSET-TOOLS` | `richview_asset_put`, `richview_asset_verify` | MCP equivalents of `richview asset put` and `richview asset verify`. | `crates/richview-app/src/asset_upload/mod.rs:312-336` | observed |
| `F-APP-MCP-RESOURCES` | MCP resources | `richview://spec/rmd` returns the format spec. `richview://examples/hello-world` returns the `init` template, not `examples/hello-world`. | `crates/richview-app/src/mcp.rs:48-61` | observed |
| `F-APP-MCP-PROGRESS` | Progress notifications | Long operations emit MCP progress notifications. | `crates/richview-app/src/mcp.rs:371-409` | observed in code |

### C-APP-ASSET-UPLOAD

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-APP-ASSET-PUT` | Upload with integrity reference | Uploads a dataset, image, video or audio file to an S3-API bucket from a named profile (5 GiB default maximum). Files up to 16 MiB use a single PUT; larger files use multipart (parts ≥8 MiB, at most 9,000), with 15-minute signatures. Prints `{url, bytes, sha256, type, visibility, …}`. Added in 92732c4, PR #22, 2026-10-11. | `crates/richview-app/src/asset_upload/store.rs:13-43`; `crates/richview-app/src/asset_upload/upload.rs`; `docs/public/publishing.md:59` | observed in code |
| `F-APP-ASSET-VERIFY` | Verify stored object | Checks a stored object against its reference. Exit codes: 2 for invalid input, 3 for a failed transfer, 4 for a mismatch. | `crates/richview-app/src/asset_upload/mod.rs:28,67-106` | observed in code |
| `F-APP-ASSET-RESUME-ABORT` | Resumable journal and abort | Upload state is journalled under `$XDG_STATE_HOME/richview/uploads`. `abort` cancels a pending multipart upload. | `crates/richview-app/src/asset_upload/upload.rs`; `crates/richview-app/src/asset_upload/mod.rs:67-106` | observed in code |

## C-RICHVIEW-WEB: browser host and site

**Responsibility.** "Browser interaction, animation, authoring and DOM updates are Rust compiled to WebAssembly" (`richview-chart/README.md:3`). The same build stages the public site served by the native server and by the Cloudflare Worker (`README.md:17-21`). Observed.

### C-WEB-WASM-API

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-WEB-WASM-RENDER-API` | Render functions in JS | `render`, `parse`, `validate`, `check`, `diagnose`, `render_compat`, `createPublishRequest`, `render_source` and `NativeSchema`. These back `@richview/core`. | `crates/richview-web/src/browser.rs:67-378` | observed |
| `F-WEB-WASM-CHART-API` | Chart functions in JS | `renderChart`, `resolveRatio`, `nominalHeight`, `resolveChartTheme` and `chart_constants`. These back `@richview/charts`. | `crates/richview-web/src/browser.rs:67-378` | observed |
| `F-WEB-WASM-STYLE-LEARN-API` | Style and learn functions | `resolveStyles`, `customPropertiesToCss`, `learnBundle`, `learnSection`, `core_constants` and `provide_font_catalog`. | `crates/richview-web/src/browser.rs:67-378` | observed |
| `F-WEB-WORKER-RENDER` | Off-main-thread rendering | `worker_request` serves `worker.js`, which the playground uses for rendering. | `crates/richview-web/src/browser.rs:67-378`; `crates/richview-web/src/browser/authoring.rs:314-509` | observed |
| `F-WEB-SETTLE-HOOK` | Settle hook | `window.__richview = {output, settled(), pending()}`; `settled()` rejects after 30 s. PDF export and verifiers use it. | `crates/richview-web/src/browser.rs:385-425` | observed |
| `F-WEB-PAGE-BOOT` | Page boot | `start()` picks the page mode (authoring, landing, homepage or reel). It settles before print, honours reduced motion, re-lays out on theme, resize or font load, and mounts `[data-spec]` time-series slots and the chart editor. | `crates/richview-web/src/browser.rs:427-517` | observed |

### C-WEB-CHART-HOST

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-WEB-CHART-ZOOM-PAN` | Zoom and pan | Drag brushes a zoom window. Shift-drag or horizontal scroll pans. Pinch or Ctrl/Meta+wheel zooms around the pointer. Double-click or Escape resets, and there is a "Reset zoom" action. The hint reads "Drag to zoom · Shift-drag to pan · Pinch on touch". | `crates/richview-web/src/browser/charts.rs:820-985`; `richview-chart/README.md:66` | observed; stated |
| `F-WEB-CHART-OVERVIEW` | Overview navigator | An overview strip that can be moved and resized, with arrow keys, +/- and Home. `.rmd` charts show it only after a zoom. | `crates/richview-web/src/browser/charts.rs:820-966`; `richview-chart/src/rmd.rs:270-271` | observed |
| `F-WEB-CHART-SPEC-CONTROLS` | Range and transform controls | Range preset buttons, the transforms "Rebased = 100", "Year-over-year" and "Published level", and "Start at zero". They appear only when a JSON spec declares them, so they are never present on `.rmd` charts. | `crates/richview-web/src/browser/charts.rs:820-966`; `richview-chart/src/spec.rs:256-278` | observed |
| `F-WEB-CHART-INSPECTION` | Hover and keyboard inspection | Hover or tap shows exact readouts. Arrow keys step through observations and Shift steps ten. Enter selects a rebase point where offered. Long readouts scroll with Page Up and Page Down. | `richview-chart/README.md:66`; `docs/RMD-SPEC.md:121` | stated |
| `F-WEB-CHART-LEGEND` | Series legend controls | Legend entries hide or emphasise series, and one series always stays visible. Focus follows the control across redraws. | `richview-chart/README.md:62,66`; `docs/RMD-SPEC.md:164` | stated |
| `F-WEB-CHART-TABLE` | Table action | "Table" shows a paged table with every observation. | `richview-chart/src/svg.rs:77`; `crates/richview-web/src/browser/charts.rs:970-985` | observed |
| `F-WEB-CHART-SVG-DOWNLOAD` | SVG download | "SVG" downloads `chart.svg` of the current view (zoom, visible series, theme; the whole grid for facets), keeping the browser-measured glyph corrections. | `crates/richview-web/src/browser/charts.rs:970-985`; `richview-chart/README.md:60`; `docs/RMD-SPEC.md:98` | observed; stated |
| `F-WEB-CHART-CSV-DOWNLOAD` | CSV download | "CSV" downloads `chart.csv` with the original rows. | `crates/richview-web/src/browser/charts.rs:970-985`; `richview-chart/README.md:72` | observed |
| `F-WEB-CHART-EXPAND` | Expand | "Expand" opens a native `<dialog>` modal in landscape on phones and toggles `rv-chart--full` on desktop. `docs/FEATURES.md:9` documents this as current. | `crates/richview-web/src/browser/charts/expansion.rs:1-60` | observed |
| `F-WEB-CHART-OPTICAL-ALIGN` | Optical ink alignment | Aligns titles and first labels to the measured ink of the loaded glyphs. | `crates/richview-web/src/browser/charts/optical.rs`; `docs/RMD-SPEC.md:98` | observed; stated |
| `F-WEB-TABLE-SORT` | Report table sorting | Clicking a header button (`data-rv-sort`) re-sorts the table from its embedded JSON payload. | `crates/richview-core/src/blocks.rs:354-471`; `crates/richview-web/src/browser/authoring.rs:1283+` | observed |

### C-WEB-PLAYGROUND

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-WEB-PLAYGROUND-LIVE-RENDER` | Edit and preview | `/playground` and `/app` serve the same page. `#source` is the editor and `#preview` is an iframe sandboxed with `allow-scripts`. Rendering runs in `worker.js` with generation counters that drop stale results. Run: both routes return 200 locally and live. | `assets/playground.html`; `crates/richview-web/src/browser/authoring.rs:314-509`; run; live | observed |
| `F-WEB-PLAYGROUND-STYLE-SELECT` | Style selector | The style `<select>` covers builtins plus showcase themes from `/showcase/themes.json`. `editorial` maps to `brutalist`. | `crates/richview-web/src/browser/authoring.rs:947-1027` | observed |
| `F-WEB-PLAYGROUND-CHECK` | Check button | Runs the chart geometry check in the browser. | `assets/playground.html`; `crates/richview-web/src/browser/authoring.rs:314-509` | observed |
| `F-WEB-PLAYGROUND-EXPORT` | Copy and downloads | "Copy source", "Download .rmd" and "Download report". The downloaded report is a self-contained HTML file that embeds gzip Wasm; local file links are refused. | `crates/richview-web/src/browser/authoring.rs:843-946` | observed |
| `F-WEB-PLAYGROUND-LOAD-SOURCE` | Load by URL or example | `?src=<url>` fetches a source, and the `#example` input loads an example. Referenced files resolve relative to the source and must stay on the same origin under the report root. | `crates/richview-web/src/browser/authoring.rs:449-480,707-806` | observed |
| `F-WEB-PLAYGROUND-LARGE-SOURCE` | Bounded editing windows | Sources over 128 KiB or 512 lines are edited in 64 KiB windows. | `crates/richview-web/src/source_buffer.rs:7-9`; `README.md:21`; `docs/RUST-ARCHITECTURE.md:25` | observed |

### C-WEB-LANDING

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-WEB-LANDING-PAGE` | Homepage | Sections: hero, demo, what, elements, how, why, flagship and signup. Nav on main: Live example, Product, Workflow, Flagship report, Docs, Get updates. Title "RichView — a presentation layer for your agent". Run: `/` returns 28,159 bytes. | `assets/landing/index.html`; `scripts/verify-homepage.mjs:38`; run | observed |
| `F-WEB-LANDING-LIVE-DEMO` | Live two-file demo | At build time `assets/landing/demo/report.rmd` and `report.rstyle.yaml` are injected into the page. Five theme pills (sleek, sleek-light, scientific, brutalist, hand-drawn) rewrite the `styles:` line, and the demo re-renders after a 140 ms debounce. PR #58 removed the "Open full report", download and playground links. | `crates/richview-web/src/browser/homepage.rs:1-30,361,437`; `crates/richview-build/src/main.rs:385-396`; PR #58 | observed |
| `F-WEB-LANDING-SIGNUP` | Update signup form | Email, consent and a honeypot field are POSTed to `/api/interest` (`F-CF-INTEREST-SIGNUP`). | `crates/richview-web/src/browser/homepage.rs:599-637` | observed in code |
| `F-WEB-LANDING-SCROLL-STAGE` | Scroll choreography | The data mark animates through chapters, driven by the flagship's monthly Hacker News series. It needs `#rv-field` (canvas) and `#rv-stage` (svg); main's HTML has neither, so the code returns early and the feature is dormant on main. `data_notes` still fills provenance text. | `crates/richview-web/src/browser/landing.rs:1-5,135-222,2418-2430,2499-2507`; `assets/landing/index.html` (0 occurrences of `rv-field`) | observed |
| `F-WEB-LEGAL-PAGES` | Impressum and privacy | `impressum.html` and `privacy.html` are staged at the site root, with `_headers` for Cloudflare. | `assets/impressum.html`; `assets/privacy.html`; `crates/richview-build/src/main.rs:371-373` | observed |

### C-WEB-CHART-EXAMPLES

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-WEB-CHARTS-EXAMPLES` | `/charts/` examples | Three examples: Keeling CO2 with zoom; CPI with Year-over-year, Rebased = 100 and Published level; and a grid of 35,040 quarter-hours. Data comes from `richview-chart/web/data`. Run and live: 200. | `richview-chart/web/index.html`; `crates/richview-build/src/main.rs:417-436`; run; live | observed |
| `F-WEB-CHARTS-EDITOR` | "Edit this chart" | An inline editor for the JSON spec, the CSV and seven colour tokens, with "Download SVG". | `crates/richview-web/src/browser/authoring.rs:1159-1236` | observed in code |
| `F-WEB-CHARTS-THEME-BUTTONS` | Theme buttons | Editorial, Brutalist, Scientific and Sleek. Editorial maps to brutalist, so two buttons render the same treatment. | `richview-chart/web/index.html:2,17`; `crates/richview-web/src/browser/authoring.rs:994-1027` | observed |

### C-WEB-SHOWCASE

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-WEB-SHOWCASE-FLAGSHIP` | Flagship report page | `/showcase/openai-vs-anthropic/` shows the flagship rendered at build time inside showcase chrome, with `source-data.json`, `diagnostics.json` and an OG image. Run and live: 200. | `crates/richview-build/src/main.rs:444-481`; run; live | observed |
| `F-WEB-SHOWCASE-THEMES` | 23 visual treatments | `themes.json` holds three production themes (sleek, scientific, brutalist) and twenty studio variants v01-v20 (provenance `richview-universe/richview-design-system` `studio/variants.js` @ 8c6a8e0). It has no `sleek-light` or `hand-drawn`. `?theme=` selects one; the default is scientific. | `assets/showcase/themes.json`; `crates/richview-web/src/browser/showcase.rs:151-197`; `README.md:17` | observed |
| `F-WEB-SHOWCASE-COMPARE` | Side-by-side compare | `compare=` (key `c`) compares two treatments. | `crates/richview-web/src/browser/showcase.rs:805,1090` | observed |
| `F-WEB-SHOWCASE-TOUR` | Auto tour | `tour=1` (key `t`) advances every 5,400 ms and pauses under reduced motion. | `crates/richview-web/src/browser/showcase.rs:1138-1181` | observed |
| `F-WEB-SHOWCASE-SHARE-EMBED` | Share and embed | `chart=`, `embed=1` and share links. Embed mode disables compare and the drawer. | `crates/richview-web/src/browser/showcase.rs:151-197,1090-1092` | observed |
| `F-WEB-SHOWCASE-KEYBOARD` | Keyboard control | ←/→ step through themes, `c` toggles compare, `s` the style drawer and `t` the tour, Escape closes modals, and focus is trapped while a modal is open. | `crates/richview-web/src/browser/showcase.rs:1035-1101` | observed |
| `F-WEB-SHOWCASE-STYLE-DRAWER` | Style drawer | Shows a treatment's YAML, diff, palette and provenance, with copy. | `crates/richview-web/src/browser/showcase.rs:1212` | observed |
| `F-WEB-SHOWCASE-CITATION` | Citation cards | Source links in the report open citation cards. | `crates/richview-web/src/browser/showcase.rs:1353` | observed in code |
| `F-WEB-SHOWCASE-INSPECT` | "Inspect chart" lightbox | An "Inspect chart" button opens a chart lightbox. | `crates/richview-web/src/browser/showcase.rs:928,1411` | observed |
| `F-WEB-SHOWCASE-CHART-REVIEW` | Chart review pages | `/showcase/chart-review/` holds the standard and `bar-study-01` reports in five styles: 5 styles × 4 widths, 60 pages, 540 figures and 1,080 screenshots. | `crates/richview-build/src/main.rs:526`; `docs/CHART-REVIEW.md` | observed; stated |

### C-WEB-REEL

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-WEB-REEL-PLAYER` | `/reel/` player | 15 s at 60 fps and 120 BPM, in ten scenes: dot, type, mark, punch, chart, themes, field, cowork, gallery, resolve. Space plays or pauses, arrows step one frame and Shift+arrow one beat. `?t=` seeks and `?record=1` enters recording mode. Reduced motion starts paused. Run and live: 200. | `crates/richview-web/src/reel.rs`; `crates/richview-web/src/browser/reel.rs`; `assets/reel/README.md`; run; live | observed |
| `F-WEB-REEL-EXPORT` | Reel exporter | `richview-reel --frame SECONDS out.svg` and `--soundtrack out.wav`. `scripts/record-reel.py` produces the MP4. Not run. | `crates/richview-web/src/bin/richview-reel.rs`; `scripts/record-reel.py` | observed in code |

### C-DOCS-SITE

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-DOCS-GUIDES` | Six guides | `index`, `authoring`, `styles`, `tools`, `publishing` and `recovery`, rendered from `docs/public/*.md` by RichView itself. Each page has a top bar (Example report, Open playground), a sidebar, a table of contents and a pager, and "View Markdown source" publishes its `.md` alongside. Rendering fails when a page has diagnostics. Run and live: `/docs/` returns 200. | `crates/richview-build/src/documentation.rs:14-21,78-127`; run; live | observed |
| `F-DOCS-REFERENCE` | Format reference | `docs/RMD-SPEC.md` is copied to `/docs/reference/RMD-SPEC.md` and linked from the sidebar. | `crates/richview-build/src/documentation.rs` | observed |
| `F-DOCS-TUTORIAL-EXAMPLE` | Tutorial example | `examples/first-report` (`report.rmd`, `report.rstyle.yaml`, `data/orders.csv`) is copied to `/docs/example/` and rendered as `report`, `scientific`, `sleek`, `brutalist` and `hand-drawn` HTML. | `crates/richview-build/src/documentation.rs:128-157` | observed |
| `F-DOCS-HIGHLIGHT` | Highlighted code samples | Uses syntect with dark and light themes; `rmd` samples highlight as Markdown. | `crates/richview-build/src/documentation.rs:168-187` | observed |

## C-RICHVIEW-EDITOR and C-VSCODE-EXTENSION

**Responsibility.** `richview-editor` is a native JSON-RPC process that owns the editor intelligence. The VS Code extension is a thin adapter that spawns it (`README.md:64`; `docs/RUST-ARCHITECTURE.md:49`). Observed in code; the extension was not run.

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-EDITOR-RPC` | JSON-RPC over stdio | 24 MiB message limit. Methods: `snippets`, `snippet`, `format`, `outline`, `complete`, `validate`, `preview`, `export` and `publish`. | `crates/richview-editor/src/main.rs` | observed |
| `F-EDITOR-DIAGNOSTICS` | Live diagnostics | `validate` returns diagnostics with UTF-16 ranges and warns about unsupported keys without invalidating the report. | `crates/richview-editor/src/authoring.rs`; `docs/RMD-SPEC.md:56` | observed |
| `F-EDITOR-COMPLETION` | Completion | Completes keys and values from the core authoring tables. | `crates/richview-editor/src/authoring.rs:460`; `crates/richview-core/src/authoring.rs` | observed |
| `F-EDITOR-FORMAT` | Formatting | Changes only indentation and spaces. | `crates/richview-editor/src/authoring.rs:400` | observed |
| `F-EDITOR-OUTLINE` | Outline and folding | Provides the document outline and folding ranges, bounded at 10,000 items. | `crates/richview-editor/src/authoring/navigation.rs:185` | observed |
| `F-EDITOR-SNIPPETS` | Snippets | 13 snippets: the nine chart types (`rv-chart-<type>`), `rv-table`, `rv-big`, `rv-callout`, and `rv-frontmatter` with `styles: [scientific]`. There are no style snippets. | `crates/richview-editor/src/lib.rs`; `packages/vscode/snippets/rmd.json` | observed |
| `F-EDITOR-PREVIEW` | Preview render | Renders with the report card on and marks the title ✓, ▲ or ✕ by status. | `crates/richview-editor/src/main.rs` | observed |
| `F-EDITOR-EXPORT-PUBLISH` | Export and publish | `export` writes to `Rendered/`. `publish` requires a title and version. | `crates/richview-editor/src/main.rs` | observed |
| `F-VSCODE-LANGUAGE` | `.rmd` language support | Language `rmd` ("Rich Markdown") for `.rmd` files, with the TextMate grammar `text.rmd` and a language configuration. Activates on `onLanguage:rmd`. | `packages/vscode/package.json` | observed |
| `F-VSCODE-PREVIEW` | Live preview | "RichView: Open Live Preview" (an editor-title button) opens a webview that refreshes as you edit. Dependent files are watched with a 150 ms debounce. `richview.previewTheme` defaults to `auto`. | `packages/vscode/package.json`; `packages/vscode/extension.cjs` | observed |
| `F-VSCODE-LINT` | Problems integration | Diagnostics are debounced by 250 ms. | `packages/vscode/extension.cjs` | observed |
| `F-VSCODE-INSERT-BLOCKS` | Insert commands | Insert Chart, Table, Big-Number and Callout blocks. | `packages/vscode/package.json` | observed |
| `F-VSCODE-PROVIDERS` | Editor providers | Document symbols, folding, formatting and completion, all served by the editor binary. | `packages/vscode/extension.cjs` | observed |
| `F-VSCODE-PUBLISH` | Publish to server | Uses `richview.server` (default `https://richview.uk`) and stores the token per server in SecretStorage. Afterwards it offers "Copy URL" or "Open in browser". There is no keybinding. | `packages/vscode/package.json`; `packages/vscode/extension.cjs` | observed |
| `F-VSCODE-EXPORT-VALIDATE` | Export and validate commands | "RichView: Export to Rendered" and "RichView: Validate". | `packages/vscode/package.json` | observed |
| `F-VSCODE-BINARY-TRUST` | Native binary and trust | Spawns the packaged `richview-editor`, or the path in `richview.binary`. Untrusted workspaces are not supported. | `packages/vscode/package.json` (`capabilities.untrustedWorkspaces`); `packages/vscode/extension.cjs` | observed |

## C-RICHVIEW-BUILD, C-RICHVIEW-BENCH and the generated packages

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-BUILD-NATIVE` | Native binaries | Builds release binaries `richview`, `richview-server` and `richview-editor`, and copies the editor binary into the VS Code package. The release profile uses opt-level 3, LTO, `codegen-units=1`, `panic=abort` and `strip`. | `crates/richview-build/src/main.rs:1-140`; `Cargo.toml:12-17` | observed |
| `F-BUILD-WASM` | Wasm bundle | wasm-bindgen 0.2.129 (`no-modules`) targets `wasm32-unknown-unknown` and produces `dist/web/richview_web.js`, `richview_web_bg.wasm` and `.gz`, plus `richview-fonts.css`. The Wasm build overrides the release profile to opt-level `s`. | `crates/richview-build/src/main.rs:72-152,301-304`; `crates/richview-web/Cargo.toml:18` | observed |
| `F-BUILD-NPM` | Package contents | Generates `packages/core/dist` (Node `rust.cjs`, browser runtime-init, `index.js`, subpaths), `packages/charts/dist`, `bootstrap.js` and `worker.js`. | `crates/richview-build/src/main.rs:140-360` | observed |
| `F-BUILD-SITE` | Site staging | Stages the landing page with the injected demo, `playground.html`, `app.html`, `playground/` and `app/`; the examples (excluding `research/`, `tmp/` and caches, refusing symlinks); `/charts/`, `/reel/`, `/styles/`, `/fonts/`, the legal pages and `_headers`. | `crates/richview-build/src/main.rs:368-436` | observed |
| `F-BUILD-REPORTS` | Report staging | Builds the flagship showcase, `landing/generated/demo-<theme>.html` for every builtin plus `check.json`, `charts/themes.css` and the chart-review pages. | `crates/richview-build/src/main.rs:438-560` | observed |
| `F-BENCH-WORKLOADS` | Render benchmark | Workloads `small`, `six-types`, `dense` and `flagship`, run for `RICHVIEW_BENCH_ITERATIONS` iterations (default 100). Reports median, p95, min and VmHWM peak memory. `--write-fixtures` and `--fixtures-only` are available. Budgets are in `docs/PERFORMANCE.md`. | `crates/richview-bench/src/main.rs:1-80`; `docs/PERFORMANCE.md` | observed in code |
| `F-PKG-CORE-API` | `@richview/core` | Version 3.0.0-alpha.0 with exports `.`, `./parser`, `./renderer`, `./schemas` and `./runtime`. The types in `assets/node/core-api.d.ts` export only the sleek, scientific and brutalist style sources. Not on npm: `npm view` returned E404 on 2026-10-21. | `packages/core/package.json`; `assets/node/core-api.d.ts:2-53`; live npm | observed |
| `F-PKG-CHARTS-API` | `@richview/charts` | A chart package typed by `assets/node/charts-api.d.ts` that depends on core. Not on npm (E404, 2026-10-21). | `packages/charts/package.json`; `assets/node/charts-api.d.ts`; live npm | observed |

## C-CLOUDFLARE-TEASER

**Responsibility.** "Cloudflare host adapter: the website and report behavior remain in Rust/Wasm." The endpoint "stores opt-in only. It has no mail provider, send path or subscriber read API" (`cloudflare/interest-worker.mjs:1-2`). Deployment needs explicit authorization (`README.md:83-99`). Observed.

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-CF-STATIC-SITE` | richview.ai static site | Worker `richview-teaser`, routed on `richview.ai`, serves `./dist/web` with `run_worker_first` for `/api/*`. Other `/api/*` paths return 404 JSON. Live: `.html` URLs get a 307 to the extensionless path, and `/api/reports` returns 404. | `cloudflare/wrangler.toml`; `cloudflare/interest-worker.mjs:33-37`; live | observed |
| `F-CF-INTEREST-SIGNUP` | Update signup endpoint | `POST /api/interest` accepts JSON of at most 1 KiB with `email`, `consent: true` and an empty honeypot field. A non-POST gets 405 (live). A cross-origin request gets 403, and missing bindings give 503. The address is stored in the D1 table `subscribers(email, consented_at, consent_version)` with consent version `richview-updates-2026-10-05`, and the response is an identical 200 for new and existing addresses. | `cloudflare/interest-worker.mjs:3-59`; `cloudflare/migrations/0001_interest.sql`; live (GET 405) | observed |
| `F-CF-RATE-LIMIT` | Signup rate limit | The `SIGNUP_RATE_LIMIT` binding allows 10 attempts per 60 s per client; beyond that it returns 429 with `Retry-After: 60`. | `cloudflare/wrangler.toml`; `cloudflare/interest-worker.mjs:45-47` | observed |

## C-SKILL, C-LLMS-TXT and C-EXAMPLES

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-SKILL-AGENT-GUIDE` | Agent skill | Covers the format, chart contract, styles, publishing "only when explicitly requested" and the seven MCP tools, plus an anti-slop reference. Parts are stale: six chart types, three styles and named frontmatter data (see Contradictions). | `skill/SKILL.md:10-20`; `skill/reference/anti-slop.md` | observed |
| `F-LLMS-TXT-SUMMARY` | llms.txt | Short agent summary: grammar, chart types, styles, the 23-treatment showcase, publishing and MCP tools. Partly stale (see Contradictions). | `llms.txt:11-25` | observed |
| `F-EXAMPLES-FLAGSHIP` | Flagship report | `examples/openai-vs-anthropic` (v0.1.0, slug `openai-vs-anthropic`): 18 charts, a ledger of 643 dated sources and a snapshot dated 2 October 2026. It feeds the showcase and the landing stage data. | `examples/openai-vs-anthropic/report.rmd:1-25`; `examples/openai-vs-anthropic/README.md` | observed |
| `F-EXAMPLES-TUTORIALS` | Example set | `agent-output`, `authoring`, `boxplot`, `first-report`, `hello-world`, `histogram`, `interval` and `sales-report`, staged under `/examples/`. | `examples/`; `crates/richview-build/src/main.rs:416` | observed |
| `F-EXAMPLES-HOMEPAGE-DEMO` | Homepage demo source | The `assets/landing/demo/report.rmd` and `report.rstyle.yaml` pair shown on the landing page. | `assets/landing/demo/`; `crates/richview-build/src/main.rs:385-396` | observed |

## C-CONTAINER-DEPLOY and C-CI

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-DEPLOY-CONTAINER` | Server image | Built on `rust:1.99` and runs as non-root uid 10001 with `RICHVIEW_BIND=0.0.0.0`. | `Dockerfile` | observed |
| `F-DEPLOY-FLY` | Fly app definition | App `richview-v3` in region `ams` with a 10 GB `/data` volume, a `/health` check and auto-stop. The file says "Applying it is a separate, explicitly authorized deployment". It was added on 2026-05-21 (3c808bd, the TypeScript scaffold). | `packages/server/fly.toml:1-40` | observed |
| `F-CI-GATES` | CI gates | Runs on push and PR to `main` with Rust, Chromium, Firefox and WebKit gates. Uploads a chart-review artifact and has no deploy job. Hosted CI has been billing-blocked before; its current status was not checked. | `.github/workflows/ci.yml` | observed |
| `F-CI-PRERELEASE` | GitHub prerelease | `chart-refinements-2026-10-12` ("Chart refinements — 12 October 2026", target 5d47fbd8): a chart-story zip, core and charts tarballs, a linux-x64 CLI tar.gz, a linux-x64 VSIX and `SHA256SUMS`. What produced it is not recorded. | `gh release list` (2026-10-21) | observed |

## Features that exist only on open PR branches

All nine branches are draft PRs based on `b4db233`, with zero commits behind `main` (observed 2026-10-21 with `git merge-base` and `gh pr list`). Rows marked "observed" were read in the branch code; rows marked "stated" come from the PR body or branch docs and were not built or run.

### PR #62: playable charts (`origin/feat/s27-648-playable-charts` @a703897, +2,751 lines, S27-648)

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-CHART-CONTROLS-PLAYBACK` | Playback control | A chart `controls:` entry with `kind: playback` plays through a field (Gapminder-style), with play button, scrubber and an aria-live readout. Space plays. Reduced motion steps one frame. | `richview-chart/src/controls.rs`, `richview-chart/src/rmd_controls.rs`, `docs/CONTROLS.md` (branch) | observed (branch code); stated (behavior) |
| `F-CHART-CONTROLS-FILTER` | Filter control | `kind: filter` adds button or dropdown filtering of the chart's rows. | same | observed (branch code); stated |
| `F-CHART-CONTROLS-METRIC` | Metric switch | `kind: metric` switches the plotted measure. | same | observed (branch code); stated |
| `F-CHART-CONTROLS-VARIANT` | Chart-version selector | `kind: variant` selects between authored chart versions. | same | observed (branch code); stated |
| `F-WEB-CONTROL-EVENTS` | Control events and state | `Info.controls`, `render_rmd_chart_with_state`, and the DOM events `rv:control` and `rv:control-set`. Examples are `playable-gapminder` and `playable-line-indicator`. `verify-playable-controls.mjs` reports 20/20. | branch diff; PR #62 body | stated |

### PR #63: multi-tab reports (`origin/feat/s27-649-multi-tab-reports` @f1cb254, +781 lines, marked WIP, S27-649)

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-CORE-TABS` | Report tabs | Frontmatter `tabs: {default, export: all or active, panels: [{id, label, from}]}`, with `{#id}` heading anchors. The output stays static-first. New diagnostics: `frontmatter.tabs` and `tabs.anchor-missing`. | `docs/RMD-SPEC.md` Tabs section (branch); `crates/richview-core` (branch) | observed (branch code) |
| `F-WEB-TABS-NAV` | Tab navigation | `#tab=` deep links, roving tabindex, and the events `rv:panel-hidden` and `rv:panel-shown`. The example is `examples/sales-report-tabs`. The PR states its gates were not run. | `crates/richview-web/src/browser/tabs.rs` (branch); PR #63 body | observed (branch code); stated |

### PR #64: voice dashboard (`origin/s27-651-voice-dashboard` @81110e0, +3,174 lines, S27-651)

`C-VOICE-DASHBOARD` is a sibling-product demo crate, `crates/richview-voice-dashboard`.

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-VOICE-TRANSCRIBE` | Voice question capture | Server-side transcription with ElevenLabs Scribe. | branch crate; PR #64 body | stated |
| `F-VOICE-PLANNER` | Question planner | A rule-based planner maps the transcript to metrics and tiles. | branch crate | stated |
| `F-VOICE-TILES` | Dashboard tiles | Six tiles drawn with `render_rmd_chart_static`, with follow-up questions and cross-selection. | branch crate; PR #64 body | stated |
| `F-VOICE-ADAPTERS` | Data adapters | `offline-capture` (labelled as such, `release_authority=false`), `local-metricflow` and `remote-mcp` (OAuth). | branch crate | stated |
| `F-VOICE-SAVED` | Save and open | Saves a private JSON file with a SHA-256 hash and opens saved dashboards. `CloudStore` is marked Unavailable. | branch crate | stated |

### PR #65: galleries (`origin/s27-647-publish-galleries` @850032b, +1,142 lines, S27-647)

`C-GALLERIES` consists of `galleries/shell.html` and the `stage_galleries` build step.

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-GALLERY-TYPOGRAPHY` | `/typography/` | Typography gallery in five identities. | branch build; `verify-galleries.mjs` (branch) | stated |
| `F-GALLERY-CHARTS` | `/chart-gallery/` | Chart gallery in five identities, linked as "Gallery" on the landing page. | same | stated |
| `F-GALLERY-FAMOUS` | `/famous-charts/` | Famous-chart recreations in five identities. | same | stated |

The PR body says these were deployed to richview.ai, but all three routes return 404 live on 2026-10-21 (observed).

### PR #66: data connectors (`origin/feat/s27-650-data-connectors` @4ecef05, +2,731 lines, S27-650)

`C-APP-CONNECTORS` is `crates/richview-app/src/connectors/` on the branch.

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-APP-CONNECTORS-SQL` | SQL data source | Block `data.sql: {connector, query or inline, cache}` runs through DuckDB. | branch code; `examples/sql-report` (branch) | observed (branch code) |
| `F-APP-CONNECTORS-METRICFLOW` | dbt MetricFlow source | `data.metricflow: {connector, metrics}` runs through `mf query`. | branch code; `examples/metricflow-report` (branch) | observed (branch code) |
| `F-APP-CONNECTORS-CACHE` | Snapshot cache | `data.cache: {key, ttl, …}` with a SHA-256 `SnapshotStore`. | branch code | observed (branch code) |
| `F-APP-CONNECTORS-CONFIG` | Connector trust config | A per-report `report.connectors.yaml` plus a trusted `connectors.toml`, selected with `--connectors` or `RICHVIEW_CONNECTORS`. The PR states one browser verifier is flaky. | branch code; PR #66 body | observed; stated |

The branch example uses callout `kind: note`, which main's validator rejects (`crates/richview-core/src/parser.rs:615-618`; inferred).

### PR #67: chart and flagship polish (`origin/task/s27-654-chart-polish` @47d8848, +340 lines, in progress, S27-654)

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-CHART-YEAR-FORMAT` | Year axis format | `axes.x.format: year`. | branch diff | observed (branch code) |
| `F-CHART-SCATTER-HIGHLIGHT` | Scatter highlight | `encoding.highlight {by, include, priority}` with sparse default labels. | branch diff | observed (branch code) |
| `F-CORE-MEASURE-WIDE-BLOCKS` | Wide-block layout | `layout.measure-wide-blocks`; the flagship opts in. | branch diff | observed (branch code) |
| `F-WEB-CHART-MODAL-EXPAND` | One modal Expand | The same modal Expand on every device. Removes the phone-only modal, the desktop inline expand and the showcase's "Inspect chart". | branch diff; PR #67 body | observed; stated |
| `F-WEB-CHART-DOWNLOAD-MENU` | PNG download and menu | Adds PNG export, then merges PNG and SVG into one Download menu. Also adds labelled action buttons, a lighter tooltip and a rectangle clamp. | branch diff | observed (branch code) |
| `F-WEB-SHOWCASE-STYLE-PICKER` | Showcase picker | Five primary style pills plus an "Other" dropdown for the studio variants. | branch diff | observed (branch code) |

### PR #68: development harness (`origin/feat/s27-652-development-harness` @bba45ab, +946 lines, S27-652)

`C-DEV-HARNESS` consists of `scripts/dev-harness/` plus `docs/architecture-current.md` and `docs/development-harness.md`.

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-DEVHARNESS-JOURNEY` | End-to-end journey | `run.py` and `journey.mjs`, with tests, drive an evidenced journey through the built product. Worker replies are gated deterministically. | `scripts/dev-harness/` (branch) | observed (branch code) |
| `F-DEVHARNESS-BUILD-OWNERSHIP` | Shared-target build ownership | Checks which worktree built a shared Cargo target before trusting its output. | `scripts/dev-harness/` (branch); PR #68 body | stated |
| `F-DEVHARNESS-ARCHITECTURE-MAP` | Current-architecture document | A system map, an npm E404 observation from 18 October and an inventory of the prerelease. | `docs/architecture-current.md` (branch) | stated |

### PR #69: site round (`origin/task/s27-653-site` @5e69843, +650 lines, S27-653)

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-WEB-LANDING-MOTION-RESTORED` | Landing motion restored | Re-mounts `<canvas id="rv-field">` and `<svg id="rv-stage">` so the scroll choreography runs again, with resize and mobile fixes in `landing.rs`. | branch diff; PR #69 body | observed (branch code, live) |
| `F-WEB-LANDING-NAV-SIMPLIFIED` | Simplified nav | The nav becomes Flagship report · Chart gallery · Docs. | branch diff; live | observed |
| `F-DOCS-CHART-GALLERY` | Docs chart gallery | `docs/public/charts.md` becomes `/docs/charts.html`, a "Chart gallery" covering all nine types. Live: `/docs/charts` returns 200 with that title. | branch `docs/public/charts.md`; live | observed |

### PR #70: RichView Publish and RichView News (`origin/feat/s27-655-richview-publish` @3f5c35f, +7,243 lines, S27-655)

`C-RICHVIEW-PUBLISH` includes the `crates/richview-publish` crate, a `cloudflare/publish` gateway (Cloudflare Access, private R2, a Durable Object, the editor UI, policy and repository), `wrangler.publish.toml`, `.github/workflows/publish.yml`, `publications/news/`, `scripts/publish/` and `docs/publish/`. The branches `publish/news` and `publish-edit/*` exist on origin.

| ID | Feature | Behavior, inputs and outputs | Where | Confidence |
| --- | --- | --- | --- | --- |
| `F-PUBLISH-SITE-CONFIG` | Publication config | `site.yaml` allows at most 20 sections, 12 nav links and 4 lead items. Items carry `publish:` metadata. | `publications/news/site.yaml`; `docs/publish/` (branch) | observed (branch code) |
| `F-PUBLISH-LAYOUTS-THEMES` | Layouts and themes | Broadsheet and Dispatch layouts; Paper and Ink themes. | `crates/richview-publish` (branch) | observed (branch code) |
| `F-PUBLISH-FEEDS-SEARCH` | Feeds and search | RSS, sitemap and site search. | branch code | stated |
| `F-PUBLISH-MEMBER-EDITION` | Member edition | A protected member edition behind Cloudflare Access. | branch code; PR #70 body | stated |
| `F-PUBLISH-EDITORIAL-DESK` | Browser editorial desk | A browser desk turns edits into Git PRs on `publish-edit/*`, each independently reviewed. | branch code; PR #70 body | stated |
| `F-PUBLISH-RELEASES` | Immutable releases and rollback | Immutable releases with a hide ledger and rollback. Caps: 1,000 items, 20 MiB per object and 256 MiB per release. | branch code; `docs/publish/` (branch) | stated |
| `F-PUBLISH-NEWS-SITE` | RichView News | `news.richview.ai` is live: 200, "Latest perspectives — RichView News" (observed 2026-10-21). | live | observed |

## Live deployments versus main (2026-10-21)

- **richview.ai** is served by the Cloudflare Worker `richview-teaser`, but its content matches PR #69, not main. The landing page has `rv-field`, `rv-stage` and a "Chart gallery" nav (28,225 bytes), and `/docs/charts` returns "Chart gallery". `/playground`, `/app`, `/charts/`, `/docs/`, `/reel/` and `/showcase/openai-vs-anthropic/` return 200. `/showcase`, `/health` and `/api/reports` return 404. A GET to `/api/interest` returns 405. PR #65's `/typography/`, `/chart-gallery/` and `/famous-charts/` return 404. The `workers.dev` host returns 200.
- **news.richview.ai** serves RichView News from the unmerged PR #70: 200, 13,754 bytes.
- **richview.uk** is behind Cloudflare and serves the v2-era TypeScript marketing SPA: "The presentation layer for AI agents", links to `richview-v2`, "source-available under Elastic License 2.0" and "Seven chart types… ECharts". `/health` and `/api/reports` return the SPA's HTML with status 200. This is still the default publish server of the CLI and VS Code.
- **therichview.com** returns 404, while the native server still redirects that host to richview.uk.
- **richview-v3.fly.dev** is live and serves an older build. `/health` returns `{"ok":true}`, and pages carry the header `x-richview: v3`. The landing title is the v2-era "RichView — The presentation layer for AI agents" (12,733 bytes). `/api/reports` publicly lists one report, `v3-launch`, updated 2026-05-21. `/playground`, `/charts/`, `/docs/`, `/reel/` and the runtime files return 404. I infer it dates from the 2026-05-21 TypeScript scaffold (3c808bd), not from the Rust server on main.
- **npm**: `@richview/core` and `@richview/charts` are not published (E404). **GitHub**: one prerelease, `chart-refinements-2026-10-12`.

## Contradictions

- **allow-network**: `crates/richview-app/README.md:35` and `docs/RUST-ARCHITECTURE.md:19` describe opt-in remote HTTPS loading with DNS pinning. In the code, `--allow-network` is ignored (`crates/richview-app/src/resolver.rs:204,249,272`, unused since dd61040 / PR #15, 2026-10-05). Run: a remote CSV fails with `report.asset.escape`. Decision: implement remote loading, or drop the flag and the docs.
- **style-path-authority**: `crates/richview-app/README.md:31` and `docs/RUST-ARCHITECTURE.md:19` say explicit `--style` files are "caller-authorized" paths. The code confines them to the report folder (`crates/richview-app/src/resolver.rs:37-38`), and an absolute or `../` style failed in the run. Decision: which rule the rewrite keeps.
- **missing-runtime**: `crates/richview-app/README.md:14` says that without the runtime "charts remain native rendered SVG… interactive: false". The code fails the export instead (`crates/richview-app/src/resolver.rs:631-633`; `crates/richview-app/tests/installed_assets.rs:296-300`: "Missing runtime must fail portable export, never fall back").
- **regenerate-auth**: `docs/public/publishing.md:53,126` says "Authentication is required for publication and regeneration". `POST …/regenerate` has no auth check (`crates/richview-app/src/server.rs:401-424`), the contract tests call it without a token (`crates/richview-app/tests/native_contract.rs:165-179`), and the run returned 200. It writes nothing but lets anyone trigger a server render.
- **default-publish-server**: The CLI (`crates/richview-app/src/cli.rs:122`) and VS Code (`richview.server`) default to `https://richview.uk`, which serves the v2 marketing SPA and answers API paths with HTML. MCP requires an explicit server (`crates/richview-app/src/mcp.rs:30-34`). `docs/public/publishing.md:55` already calls the CLI default "not evidence of a deployed service". The server keeps a therichview.com → richview.uk redirect (`crates/richview-app/src/server.rs:107-122`).
- **showcase-alias**: `/showcase` maps to `showcase.html` (`crates/richview-app/src/server.rs:599`; `crates/richview-app/README.md:25`), but the build never creates that file, so the route returns 404 locally and live. The real showcase is `/showcase/openai-vs-anthropic/`.
- **chart-type-count**: Code and the spec support nine chart types (`crates/richview-core/src/parser.rs:308-327`; `docs/RMD-SPEC.md:77`; worktree `CLAUDE.md:70`; `README.md:62`). The following still say six: `docs/public/index.md:77`, `llms.txt:11`, `skill/SKILL.md:12` and `docs/AGENT-QUICKSTART.md:13` ("All six chart types").
- **frontmatter-data**: `skill/SKILL.md:10` tells agents to bind charts to "named frontmatter data", which is a hard error (`crates/richview-core/src/parser.rs:265`).
- **canonical-styles**: The code ships five styles plus aliases (`crates/richview-core/src/style.rs:16-45`; `docs/public/styles.md:7`; `docs/FEATURES.md:20`). The worktree `CLAUDE.md:76`, `llms.txt:13`, `skill/SKILL.md:12` and the npm types (`assets/node/core-api.d.ts`) name three. `crates/richview-core/src/knowledge.rs:15` calls `sleek` a "compatibility name" while `style.rs` treats it as the canonical dark style. The showcase manifest has no `sleek-light` or `hand-drawn`. The `/charts/` "Editorial" button duplicates "Brutalist".
- **default-style**: With no style, core uses `brutalist` (`crates/richview-core/src/render.rs:104`; `docs/public/styles.md:7`). However, `richview init` writes `styles: [sleek]` (`crates/richview-app/src/cli.rs:594`), the frontmatter snippet uses `scientific`, and the showcase defaults to `scientific` (`crates/richview-web/src/browser/showcase.rs:151-197`).
- **landing-motion**: PR #58 (merged 2026-10-13) applied the 2026-10-12 feedback and explicitly dropped "the background mark-made-of-data canvas/HUD animation". PR #69 (S27-653) treats that removal as incidental, restores the canvas and stage, and is what richview.ai serves. Main's `landing.rs` choreography is dormant, and `crates/richview-app/README.md:43` ("all eight landing stages, the canvas field") is stale. Decision: does the landing page animate?
- **live-site-vs-main**: richview.ai serves unmerged PR #69 content, and the PR's own checklist leaves "Public richview.ai/ serves restored landing after wrangler deploy" unchecked. PR #65 claims a richview.ai deployment whose routes are 404. news.richview.ai serves unmerged PR #70. None of these deployments matches `main`.
- **stale-fly-deployment**: `packages/server/fly.toml` says applying it needs separate authorization, yet `richview-v3.fly.dev` is live with an older build that publicly lists a report. Decision: retire it or redeploy it.
- **publishing-claims**: The landing page promises "a menu of your reports", `/r/okurka-weekly` marked "private", and "access control" (`assets/landing/index.html:202-203,237`). The native server has public reads (`docs/public/publishing.md:128`), a JSON list only and no access control.
- **vscode-claims**: The landing page promises "snippets for the sleek, scientific and brutalist styles" and "One shortcut to publish" (`assets/landing/index.html:243-244`). There are no style snippets (`packages/vscode/snippets/rmd.json`) and no keybinding (`packages/vscode/package.json`).
- **vscode-size**: `docs/RUST-ARCHITECTURE.md:49` describes an "approximately 8 KiB" activation module, but `packages/vscode/extension.cjs` is 16,945 bytes.
- **license**: `LICENSING.md:4,14,17` describes a TypeScript-era split ("everything is MIT except" an ELv2 `@richview/server`), and `packages/server/LICENSE` is ELv2. All Rust crates, including `richview-app`, which contains the server, are MIT (`Cargo.toml`). The landing page says "RichView will be fully open source." (`assets/landing/index.html:368`), and richview.uk still says "source-available under Elastic License 2.0".
- **repository-url**: The workspace `repository` field still points to `https://github.com/richview-universe/richview-v3` (`Cargo.toml:9`), but origin is `1vecera/richview` and the old name redirects there (observed 2026-10-21).
- **wrangler-description**: `cloudflare/README.md:3` calls `wrangler.toml` "the production assets-only configuration", but it binds the Worker script, D1 and a rate limiter.
- **publish-description**: `llms.txt:21` and `skill/SKILL.md:18` say publication requires a description. The code and `docs/public/publishing.md:55` treat it as recommended (warning only, `crates/richview-core/src/parser.rs:715-751`).
- **mcp-hello-world**: The resource `richview://examples/hello-world` returns the `init` template, not `examples/hello-world` (`crates/richview-app/src/mcp.rs:57`).
- **learn-workflow-stale**: `crates/richview-core/src/knowledge.rs:16` describes the web surface as "a simple source/style playground" and lists five MCP tools; seven exist.
- **asset-refs-unused**: `richview asset put` produces references that no report block can consume yet (`docs/public/publishing.md:59`; `crates/richview-core/src/asset.rs`).
- **releases**: The brief's dev/prod model says "no releases", but a GitHub prerelease with packaged artifacts exists (`chart-refinements-2026-10-12`).
- **expand-behaviour**: `docs/FEATURES.md:9` documents the phone modal and desktop inline expand plus "Inspect chart" as current; PR #67 replaces them with one modal everywhere.
- **callout-note**: The PR #66 example uses callout `kind: note`, which main rejects (inferred from `crates/richview-core/src/parser.rs:615-618`).

## Gaps

- I did not read the Linear tickets (S27-5xx to 65x) or the Notion pages. S27 IDs are cited only from repository and PR text.
- I ran no browser automation (Playwright or the `verify-*.mjs` scripts). Browser features (chart chrome, playground, showcase, reel and landing) were verified only by reading code and by HTTP status.
- I ran no Cargo tests or benchmarks, and did not check the current CI status.
- PR-branch features come from PR bodies and diffs; none were built or run.
- Not run: the VS Code extension, PDF export (needs Chromium), asset upload (needs an S3 or Garage bucket) and the reel exporter. The Marketplace listing of the extension was not checked.
- The interest endpoint was not exercised with a POST (deliberately). Cloudflare deployment history and the bindings that are actually deployed were not checked.
- I did not check news.richview.ai's protected routes or PR #70's access, operations and verification docs in full.
- I did not read all of `richview-chart/src/rmd_render.rs` (6,927 lines). The finer chart rules rely on `docs/RMD-SPEC.md` and `richview-chart/README.md`.
- I did not read the `richview-design-system` provenance behind the 20 studio variants.
- I could not determine what produced the GitHub prerelease or what deployed `richview-v3.fly.dev` and richview.ai.
