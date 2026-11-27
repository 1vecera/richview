# Evidence: `.rmd` format, chart contract and output types

Scope: `origin/main` at `b4db233` (merge of PR #61, 2026-10-16), read from the lockdown worktree on 2026-10-21. This file records what the Rich Markdown source format, the chart contract, the style-file schema, diagnostics, report identity and the CLI/MCP/HTTP/JS JSON contracts are today, with field-level tables and proposed `T-` entity IDs for part 05.

Method: read `docs/RMD-SPEC.md`, `docs/self-validating-charts.md`, `docs/TABLE-AUTHORING-CHOICE.md`, `docs/FEATURES.md`, `docs/AGENT-QUICKSTART.md`, `docs/RUST-ARCHITECTURE.md`, `docs/MOTION.md` and `assets/node/core-api.d.ts`, then the Rust schema and validation code in `crates/richview-core`, `richview-chart`, `crates/richview-app`, `crates/richview-editor` and `crates/richview-web`. A debug `richview` CLI was built from `b4db233` (Rust 1.99.0, `cargo build --locked -p richview-app --bin richview`) and about 40 probe reports were run through `validate`, `check`, `generate` and `publish` on 2026-10-21. `generate` used a stub runtime passed with `--public-dir`, and `publish` targeted a loopback `richview server` with a throwaway data directory. The probes live in the untracked `tmp/types-probe/` and are not part of the evidence. No browser, PDF, VS Code or Wasm host was run.

Confidence: **observed** means verified in code at `b4db233` (the line is cited); **observed (run)** means also reproduced with the probe build; **stated** means written in a doc, PR or ticket and not verified; **inferred** means my reading across sources. Paths are repo-relative.

## Key findings

- **Style files can inject HTML.** In the core cascade, `chart.palette` and `chart.ramp` values in a `.rstyle.yaml` are not validated (`crates/richview-core/src/style.rs:632-650`, where the `_ => true` arm accepts them). When the cascade includes `sleek` or `sleek-dark`, `print_style` writes those values unescaped into the report's `<style>` element (`style.rs:898-952`, overrides at `906-927`). A probe with `styles: [sleek, report.rstyle.yaml]` and `chart.palette: ['red;}</style><script>alert(1)</script><style>{', ...]` produced a live `<script>alert(1)</script>` in `Rendered/report.html`. Observed (run). This contradicts `docs/RMD-SPEC.md:339` ("CSS-bound strings use closed … grammars"), `crates/richview-core/src/knowledge.rs:15` ("CSS url(), @import … are rejected") and `docs/RMD-SPEC.md:333` ("report source cannot execute JavaScript"). The chart engine's own `safe_css` filter (`richview-chart/src/rmd_render.rs:1179-1181`) protects only the inline SVG style.
- **Remote media are hard errors.** An `https://` image, audio or video `src`, or a Markdown image URL, fails the whole render with `report.asset.escape` (`crates/richview-core/src/folder.rs:218-233`), and so do missing local link targets (`report.asset.missing`). Observed (run). The spec says external assets are "diagnosed" and leave `_richview.offline` false (`docs/RMD-SPEC.md:323`). `embed` is the only remote content that renders.
- **Version permalinks are not permanent.** `MAX_VERSIONS = 50` per slug, and publishing a 51st version deletes the lowest semver versions (`crates/richview-app/src/storage.rs:13,101-111`). After 52 publications, `/r/<slug>/v/1.0.0` returned 404. Observed (run). Docs and the earlier "append-only" decision describe versions as immutable.
- **Republishing a version, or publishing a lower one, does not behave as a reader expects.** Publishing changed source under an existing `{slug, version}` returns `ok: true, created: false` with exit 0 and keeps the old artifact (`storage.rs:72-74`). Publishing a lower semver later moves `latest` to it, because `latest` is the most recently created version (`storage.rs:98-100`). Both observed (run).
- **Slugs fall back to `untitled`.** If the frontmatter produces any `frontmatter.*` diagnostic, for example `tags: [1]`, publish ignores both the authored `slug` and the title and uses `untitled` (`crates/richview-core/src/compat.rs:22-44`). Observed (run).
- **Source can set engine-internal keys.** `_renderHeight`, `_xWindow`, `_csvNumericText` and `_facetColorScope` are honoured when an author writes them, because block keys are retained and passed through. A chart with `_renderHeight: 900` rendered 900 px tall (`richview-chart/src/rmd_render.rs:1581`). Observed (run). This contradicts "No author pixel height" (`docs/RMD-SPEC.md:90`) and "currently unused keys have no effect" (`docs/RMD-SPEC.md:56`).
- **Derived columns are sorted alphabetically.** A table without `columns` takes its columns from the first row's keys, but `serde_json` is built without `preserve_order`, so the columns come out alphabetical (`crates/richview-core/src/blocks.rs:385-398`). CSV header order is lost too. Observed (run): CSV `zip,name,flag,amount,big,empty` rendered as `amount, big, empty, flag, name, zip`.
- **CLI and MCP `check` miss nonexistent fields.** A chart whose `encoding` names fields that are not in the data, and a table whose `columns`/`sort` keys are missing, report `status: clean`. Observed (run). Only the VS Code host flags them, as an `authoring.field` *error* (`crates/richview-editor/src/authoring.rs:374-391`). A missing `encoding.y` passes `validate` and fails only in `check`/render (`chart.missing-encoding`). Observed (run).
- **Chart ratios differ from the spec and fail silently.** Calendar (ISO-date) line and area charts default to 2:1 (1.45 with several panes; `richview-chart/src/render.rs:373`), not the 16:9 in `docs/RMD-SPEC.md:100`. Observed (run): 360 px at 720 px. An unparsable `style.ratio` string, or an out-of-range ratio on a calendar chart, falls back silently. An undocumented top-level `ratio` key works only on calendar charts (`richview-chart/src/rmd.rs:246`). All observed (run).
- **Most diagnostics carry no source line.** Frontmatter field, style, Markdown, folder/asset and render-limit diagnostics have no `line`. Only block-scoped and chart diagnostics carry the fence line. Observed (run).
- **Five hand-kept vocabularies disagree.** The parser (`parser.rs:269-704`), the public schema adapters (`compat.rs:53-234`), the editor vocabulary (`crates/richview-core/src/authoring.rs:6-156`), the JS declarations (`assets/node/core-api.d.ts`) and the `richview learn` text (`knowledge.rs:11-19`) each describe the format differently in places; see the disagreement table below. In addition, chart engine output can emit dropped-label reasons that the public `DroppedLabelSchema` rejects.
- **Remote assets cannot be referenced yet.** Integrity-pinned remote asset references (`AssetRef`: url, bytes, sha256, type and visibility) exist for `richview asset` and the `richview_asset_put`/`richview_asset_verify` MCP tools, but no `.rmd` field accepts one (`crates/richview-core/src/asset.rs`, which has no parser call site). Observed.
- **Open PRs would extend the format.** #63 adds frontmatter `tabs`, #66 adds `data.sql`, `data.metricflow` and `data.cache` (reversing today's `data.sql` hard error) and #62 adds chart `controls`. All three are open and unmerged. Stated.

## Entity index (proposed IDs for part 05)

| ID | Entity | Primary source | Conf. |
| --- | --- | --- | --- |
| T-RMD-DOCUMENT | `.rmd` file: frontmatter + CommonMark body + rich fences | `crates/richview-core/src/parser.rs:14-175` | observed |
| T-FRONTMATTER | Frontmatter mapping and fields | `parser.rs:33-88,200-267` | observed (run) |
| T-RICH-FENCE | Fence recognition and shared block rules | `parser.rs:8-10,90-172` | observed (run) |
| T-BLOCK-CHART | `chart` block | `parser.rs:308-546` | observed (run) |
| T-CHART-ENCODING | `encoding` channels | `parser.rs:334-348`, `rmd_render.rs:1283-1680` | observed (run) |
| T-CHART-AXES | `axes.x/y/color` | `parser.rs:384-424` | observed (run) |
| T-CHART-STYLE | chart-local `style` | `parser.rs:425-493`, `rmd.rs:40-67`, `rmd_render.rs:571-587` | observed (run) |
| T-CHART-ANNOTATION | `annotations[]` | `parser.rs:494-545`, `rmd_render.rs:2988-3112` | observed (run) |
| T-CHART-BINNING | histogram `binning` | `richview-chart/src/binning.rs:36-66,126-134` | observed |
| T-CHART-LINE … T-CHART-INTERVAL | Nine chart types (line, bar, scatter, area, pie, heatmap, histogram, boxplot, interval) | `parser.rs:309-327`, `rmd_render.rs` | observed (run) |
| T-CHART-INTERACTION | Fixed reader interactions (no authored vocabulary in `.rmd`) | `docs/RMD-SPEC.md:131-196,351`, `docs/FEATURES.md:9-26` | stated |
| T-INTERNAL-KEYS | Host-injected `_`-prefixed block keys | `rmd_render.rs:288,1581,2662`, `blocks.rs:43` | observed (run) |
| T-VALUE-FORMAT | `format` vocabulary for tables, metrics and axes | `blocks.rs:241-294`, `rmd_render.rs:588-603` | observed (run) |
| T-BLOCK-TABLE | `table` block | `parser.rs:547-597`, `blocks.rs:349-610` | observed (run) |
| T-BLOCK-METRIC | `big`/`metric` block | `parser.rs:598-614`, `blocks.rs:296-338` | observed (run) |
| T-BLOCK-CALLOUT | `callout` block | `parser.rs:615-625`, `blocks.rs:212-232` | observed (run) |
| T-BLOCK-IMAGE | `image` block | `parser.rs:626-645`, `blocks.rs:648-764` | observed |
| T-BLOCK-AUDIO | `audio` block | same | observed |
| T-BLOCK-VIDEO | `video` block | same | observed |
| T-BLOCK-EMBED | `embed` block | `parser.rs:646-666`, `blocks.rs:789-870` | observed (run) |
| T-MARKDOWN-EXT | Markdown extensions and sanitizing | `crates/richview-core/src/markdown.rs` | observed (run) |
| T-DATA-SOURCE | Block-local `data` mapping | `parser.rs:669-702`, `blocks.rs:7-45` | observed (run) |
| T-DATA-CSV | CSV typing rules | `blocks.rs:109-202` | observed (run) |
| T-DATA-JSON | JSON dataset shapes | `blocks.rs:66-108` | observed (run) |
| T-REPORT-FOLDER | Dedicated report folder and inventory | `crates/richview-core/src/folder.rs` | observed (run) |
| T-RESOLVER | Native bounded resolver | `crates/richview-app/src/resolver.rs:70-422` | observed (run) |
| T-RENDERED-OUTPUT | `Rendered/` export bundle | `resolver.rs:555-673` | observed (run) |
| T-RESOURCE-LIMITS | Size and count limits | `crates/richview-core/src/lib.rs:54-56`, `resolver.rs:12`, `crates/richview-app/src/lib.rs:14` | observed |
| T-ASSET-REF | Integrity-pinned remote asset reference | `crates/richview-core/src/asset.rs:14-283` | observed |
| T-ENGINE-CHARTSPEC | Time-series engine `Spec` (standalone, not `.rmd`) | `richview-chart/src/spec.rs:9-354` | observed |
| T-STYLE-FILE | `.rstyle.yaml` document and cascade | `style.rs:201-305,451-467` | observed (run) |
| T-STYLE-TOKENS | `tokens` section | `style.rs:554-566` | observed |
| T-STYLE-LAYOUT | `layout` section | `style.rs:567-571` | observed |
| T-STYLE-COMPONENTS | `components` section | `style.rs:576-629,673-775` | observed |
| T-STYLE-MOTION | `motion` section | `style.rs:572-575`, `docs/MOTION.md:7` | observed (run) |
| T-STYLE-CHART | `chart` section (theme to engine) | `style.rs:632-650`, `rmd_render.rs:1075-1175` | observed (run) |
| T-STYLE-BUILTINS | Builtin style names and aliases | `style.rs:16-45`, `crates/richview-core/assets/*.rstyle.yaml` | observed |
| T-CHART-PALETTE | Named categorical palettes and ramp | `rmd_render.rs:1189-1241,3861` | observed |
| T-DIAGNOSTIC | Diagnostic record | `crates/richview-core/src/lib.rs:58-95` | observed (run) |
| T-DIAG-CODES | Diagnostic code catalog | various (table below) | observed |
| T-DIAG-LINES | Source line mapping rules | `parser.rs:120-131`, `render.rs:657-712` | observed (run) |
| T-DIAG-STATUS | `clean` / `issues` status and `ok` | `render.rs:364-373`, `rmd_render.rs:1043-1047`, `cli.rs:548-550` | observed (run) |
| T-CHART-DIAGNOSTICS | Per-chart diagnostics object | `rmd_render.rs:1050`, `render.rs:155-158` | observed (run) |
| T-CHART-CHECK | Chart check finding kinds | `rmd_render.rs`, `rmd_render/*.rs` | observed |
| T-DROPPED-LABEL | Dropped-label provenance | `rmd_render.rs:777,1462,2069,2395`, `compat.rs:330-413` | observed |
| T-REPORT-SLUG | Publication slug | `compat.rs:17-49`, `storage.rs:290-301` | observed (run) |
| T-REPORT-VERSION | Publication version | `compat.rs:236-246`, `storage.rs:303-311` | observed (run) |
| T-STORED-REPORT | Stored immutable slug/version bundle | `storage.rs:15-118` | observed (run) |
| T-PUBLISH-REQUEST | Client→server publish payload | `compat.rs:8-12`, `cli.rs:428-504`, `server.rs:52-62` | observed (run) |
| T-RENDER-OPTIONS | Core render options | `lib.rs:119-208` | observed |
| T-RENDER-METADATA | `_richview` metadata | `render.rs:373` | observed |
| T-CHART-RUNTIME-PAYLOAD | Embedded chart runtime JSON | `render.rs:160-184`, `rmd_render.rs:1277-1281,1489,1568` | observed (run) |
| T-CLI-JSON | `--format json` outputs | `crates/richview-app/src/cli.rs` | observed (run) |
| T-MCP-TOOLS | MCP tools, inputs and outputs | `crates/richview-app/src/mcp.rs` | observed (run) |
| T-HTTP-API | Server routes and JSON | `crates/richview-app/src/server.rs:64-95` | observed (run) |
| T-JS-API | Node/browser package API | `assets/node/core-api.d.ts`, `crates/richview-web/src/browser.rs:150-260` | observed |

## T-RMD-DOCUMENT: document structure and parse pipeline

A `.rmd` file is CommonMark with optional YAML frontmatter and tagged YAML fences (`docs/RMD-SPEC.md:3`). Observed parse order (`crates/richview-core/src/parser.rs:14-175`):

| Stage | Behaviour | Source | Conf. |
| --- | --- | --- | --- |
| Size limit | Source over 16 MiB returns a hard `source.limit` error. | `parser.rs:15-22`, `lib.rs:54` | observed |
| BOM | A leading U+FEFF is stripped. | `parser.rs:23` | observed |
| Frontmatter | Only when line 1 is exactly `---`; closes at the first `---` or `...` line. Unclosed: `frontmatter.unclosed` warning and the block renders as Markdown. YAML error: `frontmatter.yaml` warning. Non-mapping: `frontmatter.type` warning. All at line 1. | `parser.rs:33-88` | observed (run) |
| Default | `theme: auto` is inserted when absent. | `parser.rs:25,51` | observed |
| Fence scan | `pulldown-cmark` decides which code blocks are fenced, so indented, longer and nested fences follow CommonMark. The tag is the trimmed, **lowercased** info string; `` ```Chart `` is a chart. | `parser.rs:90-119` | observed (run) |
| Block decode | YAML → JSON value → `validate_block`. Any `error` diagnostic keeps the original fence as a code block; otherwise defaults apply, the block is stored and the body gets a `<!--richview:block:N-->` placeholder. | `parser.rs:123-172` | observed (run) |
| Placeholder collision | If source already contains the reserved marker, a unique `<!--richview:generated:N:block:` prefix is used and carried on the AST. | `parser.rs:177-198`, `lib.rs:113-116` | observed |
| Render | Folder/asset validation (hard), styles, data resolution per chart/table (hard on `data.*` errors), chart engine, Markdown, placeholder expansion, prolog, header, CSS, runtime, metadata. | `render.rs:60-390` | observed |

AST types (`crates/richview-core/src/lib.rs:97-117`): `Document {frontmatter: Value, body: String, blocks: Vec<RichBlock>, diagnostics: Vec<Diagnostic>, block_placeholder_prefix?: String}` and `RichBlock {tag, value, raw, line}`, where `line` is the one-based fence line including frontmatter. Observed.

## T-FRONTMATTER: frontmatter fields

All fields are optional for drafts. Publication requires a non-empty `title` and `version`; a missing `description` is a warning (`parser.rs:715-751`). Unknown keys are preserved with no warning (`docs/RMD-SPEC.md:50`; observed (run): `custom_key` kept). Field-level diagnostics carry **no line** (`parser.rs:214-266`; observed (run)).

| Field | Accepted type | Required / default | Validation and failure | Behaviour | Source | Conf. |
| --- | --- | --- | --- | --- | --- | --- |
| `title` | string | Required to publish; draft fallback `Untitled report` | Non-string → `frontmatter.type` warning and the field is **removed**. Numbers are not coerced. | `<h1 class="rv-report__title">` and HTML `<title>`; also the slug source and the PDF running title (cut at 70 chars) | `parser.rs:205-223`, `render.rs:242-272,827-836` | observed (run) |
| `description` | string | Optional; publish warns `publish.description` | Non-string → warning, removed | Lede `<p class="rv-report__lede">`, escaped | `render.rs:273-280`, `parser.rs:735-747` | observed (run) |
| `subtitle` | string | Optional | Non-string → warning, removed | `<p class="rv-report__subtitle">` after the lede; **both** `description` and `subtitle` render when present | `render.rs:273-280` | observed |
| `author` | string | Optional | Non-string → warning, removed | Escaped `<span>` in `rv-report__meta`; never Markdown | `render.rs:281-284` | observed (run) |
| `date` | string | Optional | Non-string → warning, removed. An unquoted YAML date stays a string. | `<time datetime="…">`; no parsing or reformatting | `render.rs:285-291` | observed (run) |
| `version` | string, trimmed, semver-shaped | Required to publish | Non-string (for example `1.0`) → `frontmatter.type`; bad shape (for example `"1.2"`) → `frontmatter.version`; removed in both cases. Regex `^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$`. Leading zeros (`01.02.03`) are accepted. | Publication identity; see T-REPORT-VERSION | `parser.rs:224-237`, `compat.rs:236-246` | observed (run) |
| `slug` | string | Optional | Only type-checked at parse; character rules apply at publish | Publication slug; see T-REPORT-SLUG | `parser.rs:205-223`, `storage.rs:290-301` | observed (run) |
| `tags` | string array | Optional | Non-array or non-string item → warning, removed | Retained metadata only; not rendered | `parser.rs:250-263` | observed (run) |
| `theme` | `auto`/`light`/`dark` | Default `auto` | Other value → `frontmatter.theme` warning, reset to `auto` | `data-theme` on `<html>` only for light/dark; CLI `--theme` overrides; PDF forces light | `parser.rs:238-249`, `render.rs:295-307` | observed (run) |
| `styles` | string array | Optional; none → `brutalist` | Non-array (for example `styles: scientific`) → warning, **dropped**, falls back to the brutalist default. A non-builtin entry must resolve to a report-local file, otherwise `report.asset.missing` (CLI) or `style.unresolved` (core), both hard. | Ordered builtin names or local YAML paths, then CLI `--style` overrides | `parser.rs:250-263`, `render.rs:81-105`, `folder.rs:234-240` | observed (run) |
| `data` | — | Not allowed | Any value → `frontmatter.data` **hard error** (render, validate and publish fail) | Migration message: move data into blocks | `parser.rs:264-266`, `folder.rs:203-207` | observed (run) |
| `title_image`, `title-image`, `image` | string or image mapping | Optional; first present key wins | Rendered through the image media path; remote URL → `report.asset.escape` (hard) | Header image before the title; string form uses `alt = title` | `render.rs:248-268`, `folder.rs:241-248` | observed |
| `prolog` | Markdown string | Optional | Non-string is ignored silently | `<section class="rv-report__prolog">` before the body; its images resolve like body images | `render.rs:228-235` | observed |
| unknown keys | any | — | None | Preserved in `metadata` and the stored frontmatter | `docs/RMD-SPEC.md:50` | observed (run) |

Stated intent conflict: the Linear vNext frontmatter issue (from Daniel's 2026-06-20 voice dump) names `title`, `description` and `version` as required fields. The code makes `description` a publish warning only, and drafts need none of the three.

## T-RICH-FENCE: shared rich-block rules

| Rule | Detail | Source | Conf. |
| --- | --- | --- | --- |
| Tags | `chart`, `table`, `big`, `metric` (alias of `big`), `callout`, `image`, `embed`, `audio`, `video`. Others, such as `mermaid`, stay ordinary code. | `parser.rs:8-10`; `docs/RMD-SPEC.md:54` | observed (run) |
| Body | Must be a YAML mapping, otherwise `block.type` (error) | `parser.rs:271-281` | observed |
| YAML error | `block.yaml` (error); the fence stays visible as code | `parser.rs:158-169` | observed |
| Schema error | `block.schema` (error) with path `tag.field`; the fence stays visible as `<pre><code class="language-chart">` | `parser.rs:282-292,154-156` | observed (run) |
| Unknown keys | Retained on the block and in chart runtime payloads; no CLI warning. The VS Code host warns `authoring.key`. | `docs/RMD-SPEC.md:56`, `crates/richview-editor/src/authoring.rs:360-372` | observed (run) |
| Underscore keys | `_renderHeight`, `_xWindow`, `_csvNumericText` and `_facetColorScope` are host-internal but **honoured from source**; see T-INTERNAL-KEYS | `rmd_render.rs:288,1581,2662`; `blocks.rs:43,355` | observed (run) |
| Defaults applied at parse | table `max_rows: 20`, `total: false`, `sort.dir: asc`; image `fit: contain`; embed `height: 420` and a 6-host `allow` list | `compat.rs:249-277` | observed (run) |
| Missing chart title | `block.title` warning; the chart still renders. **No visible title** appears in the caption; only the SVG `<title>`/`aria-label` fall back to `Chart`. The public `ChartSchema` requires `title`. | `parser.rs:139-141`, `compat.rs:115-117`, `render.rs:783-788`, `richview-chart/src/rmd_render.rs:1583` | observed (run) |

## T-BLOCK-CHART: chart block fields

Top-level fields. Spec: `docs/RMD-SPEC.md:58-196`. Validation: `parser.rs:308-546`. Public schema retain list: `compat.rs:126-190`.

| Field | Type | Required / default | Validation | Behaviour | Conf. |
| --- | --- | --- | --- | --- | --- |
| `type` | enum `line\|bar\|scatter\|area\|pie\|heatmap\|histogram\|boxplot\|interval` | Required | Other value → `block.schema` | Selects the renderer path | observed (run) |
| `title` | string | Required by the public schema; a missing title in source only warns | Non-string → `block.schema`; missing → `block.title` warning | Figure caption title (claim); the SVG accessible name falls back to `Chart` | observed (run) |
| `subtitle` | string | Optional | Non-string → `block.schema` | Caption dek; calendar charts pass it as the engine `description` | observed |
| `data` | mapping | Required | See T-DATA-SOURCE | Exactly one of `inline`/`csv`/`json` | observed (run) |
| `encoding` | mapping of field-name strings | Required mapping; no channel is required at parse | Channel value non-string → `block.schema`; per-type rules below | See T-CHART-ENCODING | observed (run) |
| `binning` | mapping | Histogram only | On another type → `block.schema` | See T-CHART-BINNING | observed |
| `axes` | mapping `x`, `y`, `color` | Optional | See T-CHART-AXES | | observed (run) |
| `annotations` | array | Optional | See T-CHART-ANNOTATION | | observed (run) |
| `style` | mapping | Optional | See T-CHART-STYLE | | observed (run) |
| `ratio` (top level, undocumented) | number or `W:H` | — | Not validated | Honoured **only** by calendar line/area charts (`rmd.rs:246`); ignored elsewhere | observed (run) |

### T-CHART-ENCODING: encoding channels

| Channel | Meaning | Allowed on | Notes | Source | Conf. |
| --- | --- | --- | --- | --- | --- |
| `x` | Primary axis field | All types | Required for `scatter` (non-numeric → `chart.non-numeric-x` error), `histogram` (`block.schema`) and `interval` (`block.schema`). Optional elsewhere: line/bar/pie with no `x` render with **no diagnostic**. Axis mode for line/area: calendar if every row's `x` is an ISO date **string**, numeric if every `x` is numeric, otherwise categorical. | `rmd_render.rs:276-285`; histogram `options` `rmd_render/histogram.rs:7-58`; interval `rmd_render/interval.rs:8-70` | observed (run) |
| `y` | Value field | All except histogram | Required by the engine for line/bar/scatter/area/pie (`chart.missing-encoding` error at check/render, **not** at validate); required at parse for boxplot; optional estimate for interval; heatmap uses it as the row field; histogram forbids it | `rmd_render.rs:1664-1667`; `rmd_render/boxplot.rs:7-51` | observed (run) |
| `lower`, `upper` | Authored numeric bounds | interval only | On another type → `block.schema` "authored bounds require type: interval" | `parser.rs:367-383` | observed (run) |
| `color` | Series/group channel; numeric cell value for heatmap | All | Heatmap without numeric color → `chart.missing-color-values` (error) | `docs/RMD-SPEC.md:83`; `rmd_render.rs:4021` | observed (run) |
| `size` | Bubble area field | scatter (forbidden on histogram, boxplot, interval) | Negative/non-numeric → `chart.invalid-bubble-size` (error) with the bubble omitted; missing → `chart.missing-bubble-size` (warning); largest bubble radius 16 px | `rmd_render.rs:1883`; `docs/RMD-SPEC.md:170-172` | observed (run) |
| `label` | Point-name field | scatter (forbidden on histogram, boxplot, interval) | Dense names dropped with reason `label density limit` | `rmd_render.rs:2392-2395` | observed |
| `facet` | Panel field | All | One column below 600 px, two at 600 px and above; each panel has its own Table/CSV; interval facet rows must name a panel | `rmd_render.rs:1499-1572`; `interval.rs:192-195` | observed |

### T-CHART-AXES: axes fields

| Field | Type | Validation | Behaviour | Source | Conf. |
| --- | --- | --- | --- | --- | --- |
| `axes.x.label`, `axes.y.label`, `axes.color.label` | string | Non-string → `block.schema` | Axis titles. `axes.color.label` is retained metadata only (heatmaps have no color bar). | `parser.rs:401-405`; `docs/RMD-SPEC.md:88` | observed |
| `axes.*.format` | string | String only; values are **not** validated | Implemented: `percent`, `currency`, other strings → plain; see T-VALUE-FORMAT | `rmd_render.rs:588-603` | observed |
| `axes.*.suffix` | string | String only | Appended unit; `format: percent` with `%` suffix prints one `%` | `rmd_render.rs:597-602` | observed |
| `axes.x.locale` | `en-US`/`en-GB`/`cs-CZ`/`de-DE`/`ja-JP` | Any axis accepts it (`axes.y.locale` and `axes.color.locale` pass validation) | UTC calendar labels; used by the calendar bridge from `axes.x.locale` only | `parser.rs:393-400`; `rmd.rs:256` | observed (run) |
| `axes.y.min`, `axes.y.max` | finite numbers | Non-number → `block.schema`; `min >= max` → `block.schema` | Fix value-axis ends. Histogram: `min` must be 0 and `max` positive. | `parser.rs:408-423`; `histogram.rs:31-48` | observed (run) |

### T-CHART-STYLE: chart-local style

| Field | Type | Validation | Behaviour | Source | Conf. |
| --- | --- | --- | --- | --- | --- |
| `style.palette` | name: `editorial`, `brutalist`, `scientific`, `sleek`, `sleek-dark`, `sleek-light`, `hand-drawn` | Arrays and other names → `block.schema` | Overrides the report palette; omitted → inherits the style's `chart.palette` | `parser.rs:464-482`; `rmd_render.rs:1100-1115` | observed (run) |
| `style.ratio` | positive number or any string | Non-positive number → `block.schema`; **any string passes** | `W:H` parsed; unparsable string → type default with no diagnostic. Defaults: 16:9 for line/area/bar/heatmap/histogram/boxplot/interval, 4:3 scatter, 1:1 pie. **Calendar line/area: 2:1** (1.45 with several panes), and ratios outside 0.5–6 are ignored. | `parser.rs:488-492`; `rmd_render.rs:571-587`; `rmd.rs:246-269`; `render.rs:373` | observed (run) |
| `style.orientation` | `auto`/`vertical`/`horizontal` | Other → `block.schema`; histogram allows only `auto`/`vertical` | Bars, boxplots and intervals; nominal bars with `auto` switch to horizontal when names do not fit | `parser.rs:483-487`; `docs/RMD-SPEC.md:91,146` | observed |
| `style.curve` | `linear`/`monotone`/`step`/`step_before`/`step_after` | Other → `block.schema`; together with `style.line.smoothing` → `block.schema` | Line/area interpolation | `parser.rs:437-454`; `rmd.rs:40-67` | observed (run) |
| `style.line.smoothing` | `straight`/`monotone`/`current` | Other → `block.schema` | Default `monotone` (strength 1). `current` = monotone at 0.75. A theme `chart.line.smoothing` applies when neither field is set. | `parser.rs:426-443`; `rmd.rs:48-57` | observed (run) |
| `style.curvature` | number 0–1 | Outside → `block.schema` | Monotone handle strength; ignored for straight and step curves | `parser.rs:455-460`; `rmd.rs:58-66` | observed (run) |
| Pixel height | — | — | Not an authored field (`docs/RMD-SPEC.md:90`), but `_renderHeight` in source is honoured | `rmd_render.rs:1581` | observed (run) |

### T-CHART-ANNOTATION: annotations

| `kind` | Anchor | `text` | Extra | Validation | Conf. |
| --- | --- | --- | --- | --- | --- |
| `point` | `at` mapping | Required string | — | `at` values must be strings or numbers | observed |
| `range` | `from`, `to` mappings | Optional | — | same | observed |
| `line` | `at` mapping | Optional | — | same | observed |
| `band` | `from`, `to` mappings | Optional | — | same | observed |
| `text` | `at` mapping | Required string | `rotate` number | `rotate` non-number → `block.schema` | observed |

Sources: `parser.rs:494-545`; `docs/RMD-SPEC.md:180-188`. Anchor keys are source field names (Cartesian axes), a named slice (pie), a cell (heatmap), or `value` plus the measure name for histograms (`at: {value: 10, count: 4}`, `docs/RMD-SPEC.md:115`). An unknown kind is a `block.schema` error; the engine has a fallback `chart.unknown-annotation` error that the parser makes unreachable (`rmd_render.rs:3100`). Coordinates that resolve nowhere → `chart.unresolved-annotation` warning (observed (run)). A calendar chart clears the engine's own annotations and overlays every `.rmd` annotation in source coordinates, including a y reference line (`rmd_render.rs:1357,1418-1430`; observed (run)). Labels are omitted when there is no clear position, giving a dropped label with reason `no collision-free position` (`rmd_render.rs:777`). The public schema keeps `rotate` only on `text`; the editor vocabulary offers `rotate` on every kind (`compat.rs:164-173`; `crates/richview-core/src/authoring.rs:97`). Observed.

### T-CHART-BINNING: histogram binning

`binning` deserializes into `binning::Spec` with `deny_unknown_fields` (`richview-chart/src/binning.rs:36-46`).

| Field | Type | Rule | Conf. |
| --- | --- | --- | --- |
| `count` | integer 1–256 | Default Sturges `ceil(log2 n) + 1`, capped at 256; a constant sample gives one interval | observed |
| `extent` | `[lo, hi]` finite, increasing | Observations outside are excluded and counted | observed |
| `edges` | 2–257 finite, strictly increasing numbers | Cannot combine with `count` or `extent` | observed |
| `measure` | `count` (default) / `proportion` / `density` | Proportion over accepted visible observations per panel; density = proportion ÷ width | observed |

Sources: `binning.rs:6,48-66,80-125,126-134`; `docs/RMD-SPEC.md:104-113`. Intervals are lower-inclusive and upper-exclusive, with the last edge included (`binning.rs:136-140`). Edges resolve against the full source before facets and visibility changes, and the runtime payload carries the resolved edges (`rmd_render/histogram.rs:60-84`). Observed.

### Chart types (T-CHART-*)

Rows show channel roles: **R** required, **O** optional, **X** rejected with a `block.schema` error, and "—" meaning accepted but unused (inferred where noted).

| ID | Type | x | y | lower/upper | color | size | label | facet | binning | orientation | Default ratio | Engine path | Conf. |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| T-CHART-LINE | `line` | O (calendar, numeric or categorical) | R (engine) | X | O series | — | — | O | X | — | 16:9; calendar 2:1 | Calendar → time-series engine (payload `kind: time-series`); else SVG painter | observed (run) |
| T-CHART-BAR | `bar` | O categorical | R (engine) | X | O series/group | — | — | O | X | O | 16:9 (horizontal grows) | SVG painter | observed (run) |
| T-CHART-SCATTER | `scatter` | R numeric (engine) | R (engine) | X | O | O bubble | O names | O | X | — | 4:3 | SVG painter | observed (run) |
| T-CHART-AREA | `area` | O | R (engine) | X | O | — | — | O | X | — | 16:9; calendar 2:1 | as line | observed (run) |
| T-CHART-PIE | `pie` | O category | R non-negative (engine) | X | — | — | — | O | X | — | 1:1 (grows for labels) | SVG painter | observed (run) |
| T-CHART-HEATMAP | `heatmap` | R column (engine) | R row (engine) | X | R numeric value (engine) | — | — | O | X | — | 16:9; grows with rows | SVG painter | observed (run) |
| T-CHART-HISTOGRAM | `histogram` | R numeric (parse) | X | X | O stacks groups | X | X | O | O | `auto`/`vertical` | 16:9 | SVG painter | observed |
| T-CHART-BOXPLOT | `boxplot` | O categorical | R numeric (parse) | X | O | X | X | O | X | O | 16:9 | SVG painter | observed (run) |
| T-CHART-INTERVAL | `interval` | R categorical (parse) | O estimate | R (parse) | O (non-empty) | X | X | O (non-empty) | X | O | 16:9 | SVG painter | observed (run) |

Engine-level errors and warnings by type, all surfaced as `chart.<kind>` diagnostics. Source: the check-kind grep in `richview-chart/src`; probes observed (run) where marked.

| Type | Errors | Warnings / info | Spec behaviour | Conf. |
| --- | --- | --- | --- | --- |
| line/area | `source-error` (calendar data invalid, for example repeated timestamps: "Cannot draw this time series"), `missing-encoding` | `source-warning` (gaps; narrow breaks get rug marks), `series-differentiation` (>8 series), `aggregation`/`decimation` (info), `non-numeric-y` | Lines break at missing values (`docs/RMD-SPEC.md:152`); numeric X zoom (`:131-133`); monotone default; steps (`:129`); zero rules (`:148`) | observed / stated |
| bar | `ambiguous-bar-slot` (repeated category/series values, run), `missing-encoding` (run) | `label-dropped`, `label-thinned`, `fixed-height` | Rounded value end via `chart.barRadius`, 1.5-unit zero clearance, `n/a` marks, color equal to x gives one slot, decals (`docs/RMD-SPEC.md:96,137,148,150`) | observed (run) / stated |
| scatter | `non-numeric-x` (run), `invalid-bubble-size` (run) | `missing-bubble-size`, `fixed-height` | Bubble key with a 16 px reference circle; zero padding trimmed for one-signed percent axes (`:170-174`) | observed (run) / stated |
| pie | `negative-pie-value` (run), `value-overflow` | `empty-region` (total 0), `fixed-height` | Repeated categories aggregate; `<0.1%` / `>99.9%` labels; wrapped key on narrow pies; decals (`:139-142`) | observed (run) / stated |
| heatmap | `missing-color-values` (run), `invalid-color-values`, `unlocated-color-values` (run), `ambiguous-numeric-cell`, `ambiguous-missing-cell` | `fixed-height` | Missing markers `.`, `na`, `n/a`, `nan`, `null`, `none` → neutral `n/a` cell; OKLCH three-stop ramp with a negative arm turned 150°; no color bar; values print in cells ≥ 40×20 px (`:154-166`) | observed / stated |
| histogram | `histogram` (option failures) | `binning` (info), `excluded-observations`, `clipped-measure` | See T-CHART-BINNING (`:102-115`) | observed / stated |
| boxplot | `boxplot` (option failures) | `boxplot-summary` (info, run), `clipped-distribution`, `excluded-observations` | R-7 quartiles; whiskers at observed values within inclusive 1.5-IQR fences (`:117-121`) | observed (run) / stated |
| interval | `source-error` (duplicate pair, run; reversed bounds, run; malformed or underflowing numbers; empty facet name) | `interval-summary` (info, run), `clipped-interval`, `collapsed-interval`, `excluded-observations` | No statistical inference; estimate outside its range allowed (`:123-127`) | observed (run) / stated |
| all | `unsupported-chart` (unreachable after the parser), numeric underflow `source-error` | `clipping`, `label-collision`, `empty-region` ("No resolved data rows"), `chart.font-bounds-estimated` (info, always) | Exact source values in readouts, Table and CSV (`:196`) | observed |

### T-CHART-INTERACTION: reader interactions

`.rmd` has no authored interaction field; behaviour is fixed host policy (inferred from the absence of any interaction key in `parser.rs` and `authoring.rs`). Reader controls by type, as stated in `docs/RMD-SPEC.md:131-196,351` and `docs/FEATURES.md:9-26`: every chart has caption actions for Table, SVG and CSV plus Expand (observed in the HTML: three action spans plus `data-expand`); hover/tap and keyboard readouts of exact source rows; series legend toggles when there is more than one group; SVG export of the current view and CSV of all resolved rows. Line, area and scatter add numeric X zoom and pan. Calendar line/area use engine zoom (drag, pinch, ctrl-scroll, double-click reset), but range presets, transforms and the navigator stay unreachable from `.rmd` because the bridge sets no `interaction` (`rmd.rs:254-272`). Pie key entries inspect slices. Boxplot readouts scroll with Page Up/Down. Phone Expand opens a landscape modal. Hit testing is containment for filled marks and 24 px proximity for line points (`rmd_render.rs:69-105`, observed). Planned author-level controls (playback, filter, metric, variant) exist only on open PR #62.

### T-INTERNAL-KEYS: host-internal block keys

| Key | Set by | Effect | Honoured from source | Conf. |
| --- | --- | --- | --- | --- |
| `_renderHeight` | browser host (`crates/richview-web/src/browser.rs:241`, `browser/charts.rs:2659`) | Fixed SVG height (`rmd_render.rs:1581`) | Yes: `_renderHeight: 900` → 900 px, and it persists into the runtime payload | observed (run) |
| `_xWindow` | browser zoom (`browser/charts.rs:2672`) | Numeric X window (`rmd_render.rs:287-294`) | Yes (inferred from code) | observed (code) |
| `_csvNumericText` | core CSV resolution (`blocks.rs:43`) | Table treats numeric strings as numbers for sort/total (`blocks.rs:355,474-479`) | Yes (inferred from code) | observed (code) |
| `_facetColorScope` | facet renderer (`rmd_render.rs:1514-1527`) | Parent palette slots in facets (`rmd_render.rs:2660-2676`) | Yes (inferred from code) | observed (code) |

### T-VALUE-FORMAT: value formats

| Context | Vocabulary | Behaviour | Source | Conf. |
| --- | --- | --- | --- | --- |
| Table column `format`, metric `format` | `currency` (`$` + grouped integer), `currency_compact` (`$` + K/M/B), `percent` (×100, one decimal), `number` (grouped, up to 3 decimals), `number_compact` (K/M/B) | Unknown strings → raw text; nulls → empty; numeric strings are formatted | `blocks.rs:241-294` | observed (run) |
| Axis `format` | `percent`, `currency`, anything else → plain | Automatic `k`/`M`/`B`, scientific notation for long or tiny values, percent precision up to 8 decimals | `rmd_render.rs:588-603`; `docs/RMD-SPEC.md:176-178` | observed / stated |
| Editor completion | same five values for axes, table and metric | Suggests `currency_compact` and `number*` for axes, which axes do not implement (inferred) | `crates/richview-core/src/authoring.rs:146-154` | observed |

## T-BLOCK-TABLE: table block

| Field | Type | Required / default | Validation | Behaviour | Source | Conf. |
| --- | --- | --- | --- | --- | --- | --- |
| `data` | data mapping | Required | `block.schema` if not a mapping; see T-DATA-SOURCE | Rows as records | `parser.rs:548-550` | observed (run) |
| `columns` | array of `{key, label?, format?, align?}` | Optional; derived from the **first row's keys in alphabetical order** | Each needs a string `key`; `label`/`format` strings; `align` in `left`/`center`/`right` | Header buttons sort client-side; `align: right` or a numeric format → `num` class | `parser.rs:557-582`; `blocks.rs:362-399,545-558` | observed (run) |
| `sort` | `{key: string, dir?: asc\|desc}` | `dir` default `asc` | Non-string key or bad dir → `block.schema` | Order: nulls, numbers, text (`desc` reverses); exact-decimal tie-break for CSV text numbers | `parser.rs:583-593`; `blocks.rs:400-413,483-544` | observed |
| `max_rows` | positive integer | Default 20 | 0 or non-integer → `block.schema` | Rows beyond go into `<details>` "Showing N of M rows"; PDF prints every row | `parser.rs:551-556`; `blocks.rs:414-458` | observed |
| `total` | boolean | Default false | Non-boolean → `block.schema` | `<tfoot>` sums columns whose cells are all numeric; the first non-numeric column says `Total` | `parser.rs:594-596`; `blocks.rs:428-450` | observed |
| (cells) | any | — | — | A cell that is one whole http(s) URL becomes a link with the host as its text; ISO dates and identifier-like tokens get `date`/`id` no-wrap classes | `blocks.rs:577-646` | observed |

Unknown field names in `columns`/`sort` produce no CLI or MCP diagnostic (observed (run)); the VS Code host flags them as `authoring.field` errors. The table payload (`<script data-rv-table>`) embeds the resolved block, including `_csvNumericText` (observed (run)). Row-mapping syntax is the supported contract. The column-oriented `series` proposal is unsupported and Daniel's choice is pending (`docs/TABLE-AUTHORING-CHOICE.md:5,76-99`; the Linear table-syntax decision, status `Que` when read on 2026-10-21).

## T-BLOCK-METRIC: big / metric block

| Field | Type | Required | Validation | Behaviour | Conf. |
| --- | --- | --- | --- | --- | --- |
| `value` | number or string | Required | Other types → `block.schema` | Formatted with `format`, then the unit is attached | observed (run) |
| `label` | string | Optional | String | `rv-big__label` | observed (run) |
| `unit` | string | Optional | String | Suppressed for `currency*` formats; a unit ending in `$ € £ ¥` leads the value; otherwise the axis unit rule. `format: percent` with `unit: "%"` renders **`68.2%%`**; `value: "n/a"` with `unit: "%"` renders `n/a%`. | observed (run) |
| `format` | string | Optional | String only | See T-VALUE-FORMAT | observed (run) |
| `delta` | number | Optional | Non-number → `block.schema` | Always printed as a signed percent (×100, one decimal) | observed (run) |
| `delta_label` | string | Optional | String | Appended to the delta | observed (run) |
| `trend` | `up`/`down`/`flat` | Optional; class default `flat` | Other → `block.schema` | Colors the delta from `trend`, **not** from the sign of `delta` | observed (run) |

Sources: `parser.rs:598-614`; `blocks.rs:296-338`; `docs/RMD-SPEC.md:295-307`. The public schema adapter accepts `BigSchema`/`big` but not `metric` (`compat.rs:57`). Observed.

## T-BLOCK-CALLOUT: callout block

| Field | Type | Required | Validation | Behaviour | Conf. |
| --- | --- | --- | --- | --- | --- |
| `kind` | `up`/`down`/`neutral`/`hold` | Required | Otherwise `block.schema` | `rv-callout--<kind>` | observed (run) |
| `title` | string | Optional | String | Runs into the first paragraph (`<b class="rv-callout__title">`), or stands on its own line when the body starts with another block | observed (run) |
| `body` | Markdown string | Required | Non-string → `block.schema` | Rendered through the shared Markdown pipeline; images resolve like body images | observed (run) |

Sources: `parser.rs:615-625`; `blocks.rs:212-232`; `docs/RMD-SPEC.md:309-319`.

## T-BLOCK-IMAGE, T-BLOCK-AUDIO, T-BLOCK-VIDEO: media blocks

| Field | Applies to | Type | Validation | Behaviour | Conf. |
| --- | --- | --- | --- | --- | --- |
| `src` | all | string, report-relative path or safe `data:` URI | Required. A remote URL is a **hard** `report.asset.escape` at render (run). An unsafe protocol → `security.media` warning and a blocked placeholder. | Native hosts embed local files as base64 data URIs (`resolver.rs:201-227`) | observed (run) |
| `alt` | all | string | String | Accessible label; falls back to `title` | observed |
| `title` | audio, video (image: unvalidated fallback) | string | String for audio/video | Label fallback | observed |
| `caption` | all | string | String | Escaped `<figcaption>` | observed |
| `fit` | image | `contain`/`cover`/`fill` | Otherwise `block.schema`; default `contain` | `object-fit` | observed |
| `width`, `height` | image, video (audio accepted but unused) | positive integer | Otherwise `block.schema` | HTML attributes | observed |
| `poster` | video | string image | Unsafe → `security.media` warning | Missing poster in PDF → `export.video-poster` warning and a text card | observed |

Sources: `parser.rs:626-645`; `blocks.rs:648-787`; `docs/RMD-SPEC.md:319`. The TS `Media` type omits `alt`/`title` (`assets/node/core-api.d.ts:21`).

## T-BLOCK-EMBED: embed block

| Field | Type | Required / default | Validation | Behaviour | Conf. |
| --- | --- | --- | --- | --- | --- |
| `url` | absolute URL | Required | Unparsable → `block.schema`. Non-HTTPS, a host not on the list, or credentials → `security.embed` **warning** and a blocked placeholder (not a schema error). | Sandboxed `<iframe>`; `assets.network-dependent` info | observed (run) |
| `allow` | string array | Default `youtube.com`, `www.youtube.com`, `vimeo.com`, `player.vimeo.com`, `gist.github.com`, `codepen.io` | Non-array → `block.schema` | Replaces the default; matches a host or its subdomains | observed (run) |
| `height` | positive integer | Default 420 | Otherwise `block.schema` | iframe height | observed (run) |
| `caption` | string | Default text `Embedded content` | String | `figcaption` and iframe `title` | observed (run) |

Sandbox: hosts on the built-in 7-entry list get `allow-scripts allow-same-origin allow-presentation`; other author-allowed hosts get `allow-scripts allow-presentation` (`blocks.rs:800-863`). That render-time list includes `youtube-nocookie.com`, but parse always inserts the 6-host default (`compat.rs:264-274`). A nocookie URL is therefore blocked by default (observed (run)), yet gets same-origin rights when an author allows it. PDF prints a link card instead of the iframe.

## T-MARKDOWN-EXT: Markdown extensions

Observed (run) in one probe: GFM tables, footnotes, definition lists, `[[Other Report|label]]` and `[[Plain Name]]` → `<a class="rv-wikilink" href="/r/other-report">`, `> [!quote] Author` → `<cite class="rv-pullquote__author">`, a wholly emphasised `*Source: …*` paragraph after a rich block → `<p class="rv-source">` with `rv-source__label`, curly quotes and dashes outside code spans, `$x^2$` → MathML, and raw `<script>` removed with a `security.html` warning (`markdown.rs:6-580`; `docs/RMD-SPEC.md:325-333`). Markdown diagnostics (`security.link`, `security.image`, `security.html`, `markdown.math`, `assets.*`) carry no line. Heading attribute anchors (`{#id}`) are not enabled on main; PR #63 enables them.

## T-DATA-SOURCE, T-DATA-CSV, T-DATA-JSON: data

| Key | Type | Rule | Failure | Source | Conf. |
| --- | --- | --- | --- | --- | --- |
| `data.inline` | array of row mappings | Exactly one of `inline`/`csv`/`json` | Not an array of mappings → `block.schema`; zero or several sources → `data.local` (error, block stays code) | `parser.rs:669-690` | observed (run) |
| `data.csv` | relative path string | Inside the report folder, no `..`, `\`, `:`, `?`, `#`, `%`, NUL or leading `/` | Escape → `report.asset.escape` (error, with line); missing → `report.asset.missing` (hard, no line) | `parser.rs:695-701`; `folder.rs:97-102,218-233` | observed (run) |
| `data.json` | relative path string | same | same | same | observed (run) |
| `data.id`, `data.url`, `data.sql` | — | Not allowed | `data.local` error ("Header IDs, remote URLs and SQL loading are not report-local") | `parser.rs:692-694` | observed (run) |
| resolved bytes | `RenderOptions.data[reference]` | Hosts resolve; core performs no I/O | Missing → `data.unresolved` (hard) | `blocks.rs:7-45` | observed |

CSV typing (`blocks.rs:109-202`), all observed (run): headers are trimmed and must be non-empty and unique (`data.csv.header`, hard, with line); empty cell → `null`; `true`/`false` → booleans; leading-zero tokens such as `00123` stay strings; integers beyond 2^53 that do not round-trip (`9007199254740993`) stay strings; decimals that do not round-trip (`1.234567890123456789`) stay strings; `1.50` becomes `1.5`. A malformed CSV row gives a `data.csv` error. JSON accepts an array of row mappings or `{inline: [...]}`, otherwise `data.json` (hard). A resolved dataset over 16 MiB gives `data.limit`. Any `data.*` error aborts the whole render (`render.rs:137-145`). Rows are stored as `serde_json` maps, so key order is alphabetical everywhere downstream (observed (run)).

## T-REPORT-FOLDER, T-RESOLVER, T-RENDERED-OUTPUT, T-RESOURCE-LIMITS: folder, resolver and output

| Rule | Detail | Source | Conf. |
| --- | --- | --- | --- |
| Inventory required | Every core render requires `RenderOptions.report_folder {root, source, files}`; none → `report.root.missing` (hard). Root must not be `.`, `/` or `..`; the source is a single `.rmd` file name directly in the root. | `folder.rs:104-125`; `render.rs:65` | observed |
| One `.rmd` per folder | Several `.rmd` files anywhere in the inventory → `report.root.shared`; duplicate entries → `report.root.shared` | `folder.rs:126-181` | observed |
| Native inventory | Recursively lists the source's parent; a symlinked root or source → `report.root.escape`; over 10,000 entries → `report.root.unresolved`. No host infers a report from its working directory. | `resolver.rs:315-384` | observed |
| Hard asset checks before render | `frontmatter.data`, `data.local`, `report.asset.escape` and `report.asset.missing` for styles, header image, data, media `src`/`poster`, Markdown images and relative Markdown **links** | `folder.rs:186-291` | observed (run) |
| Containment | Canonical path must stay under the root (symlinks resolved) and be a regular file | `resolver.rs:387-422` | observed |
| Network | `--allow-network` is accepted and **ignored** (`_allow_network`); there is no remote fetch | `resolver.rs:201-205,269-275`; `docs/RMD-SPEC.md:247` | observed |
| `data.sql` | The resolver also rejects it: "SQL execution requires an explicitly attached data engine" | `resolver.rs:297-299` | observed |
| CLI `--style` | Builtin name, or a file that must sit inside the report folder | `resolver.rs:110-118,245-267` | observed |
| Export | `Rendered/<basename>.html`; `-o` must be a `.html` name directly inside `Rendered`; referenced local files are copied with the same relative paths; collisions → `report.output.collision`; a symlinked output → `report.output.escape`; charts without a built runtime → `report.output.runtime` | `resolver.rs:555-673` | observed (run) |
| Limits | Core source and dataset 16 MiB (`lib.rs:54`); report HTML and accumulated blocks 64 MiB (`render.output-limit`, `lib.rs:56`); native resolved data/media/style 64 MiB combined (`resolver.rs:12`); host file and request bodies 32 MiB (`crates/richview-app/src/lib.rs:14`); chart width: core 240–8192 (else a `render.width` warning and 720), CLI/MCP/HTTP 240–4096 (hard) | as cited | observed |

## T-ASSET-REF: integrity-pinned asset reference

Defined but not part of `.rmd` (no parser call site; observed). Used by `richview asset put/verify/abort` and the MCP tools `richview_asset_put` and `richview_asset_verify` (`crates/richview-app/src/asset_upload/mod.rs:312-336`), introduced in commit `92732c4` (2026-10-11) via PR #22.

| Field | Type | Rule | Conf. |
| --- | --- | --- | --- |
| `url` | string | Absolute HTTPS (HTTP only for loopback), ≤2048 chars, no credentials, no fragment, no presigning query parameters (`x-amz-*`, `x-goog-*`, `signature`, `key-pair-id`, `sig` with `se`/`sp`, and others) | observed |
| `bytes` | integer 1 to 2^53−1 | Object size | observed |
| `sha256` | 64 lowercase hex | Whole-object hash | observed |
| `type` | media type | data: `text/csv`, `application/json`; image: png/jpeg/webp/avif/gif/svg+xml; video: mp4/webm; audio: mpeg/mp4/ogg/webm | observed |
| `visibility` | `public`/`private` | Private objects are read with store-profile credentials | observed |
| `duration` | seconds > 0 | Audio/video only | observed |
| `width`, `height` | positive u32 | Image/video only | observed |

Codes: `asset.mapping`, `asset.url`, `asset.presigned`, `asset.bytes`, `asset.sha256`, `asset.type`, `asset.visibility`, `asset.duration`, `asset.dimensions` (errors) and `asset.unknown-key` (warning) (`asset.rs:121-283,304-387`).

## T-ENGINE-CHARTSPEC: time-series engine spec

The typed `ChartSpec` of `richview-chart` (`spec.rs`, every struct `deny_unknown_fields`). `.rmd` reaches it only through the calendar bridge (`rmd.rs:69-273`, which sets `data.csv`, `time.field`, `time.locale`, `y`, `series`, `title`, `description` and `aspect`). The full vocabulary is used by standalone JSON specs in `richview-chart/web/examples/` (`cpi.json`, `grid.json`, `keeling.json`). Observed.

| Struct | Fields |
| --- | --- |
| `Spec` | `title?`, `description?`, `data`, `time`, `y?`, `series?`, `panes?`, `transform?`, `view?`, `notes?`, `annotations[]`, `interaction`, `source?`, `demo` (bool), `aspect?` (0.5–6, `chart.rs:656-659`) |
| `DataSpec` | exactly one of `csv`, `rows`, `columns`, `url` (hosts resolve `url`) |
| `TimeSpec` | `field`, `locale` (5 locales), `format` (`iso`/`unix_s`/`unix_ms`) |
| `AxisSpec` | `label`, `unit`, `format`, `scale`, `zero`, `min`, `max`, `decimals` (0–324), `secondary {unit, factor, offset}` |
| `SeriesSpec` | `field`, `name`, `mark` (`line`/`area`/`step`), `color` (`series-1..8`, `accent`, `accent-2`, `ink`, `ink-soft`, `ink-faint`), `width`, `curve`, `curvature`, `stack`, `negative_color`, `end_label`, `max_gap` (e.g. `45D`) |
| `PaneSpec` | `y`, `series[]`, `weight` |
| `TransformSpec` | `type` (`none`/`rebase`/`yoy`), `at` |
| `ViewSpec` | `from`, `to`, `last` (e.g. `10Y`, `YTD`) |
| `AnnotationSpec` | `{type: band, from, to, label}` or `{type: event, at, label}` |
| `InteractionSpec` | `hover` (`x`/`nearest`/`none`), `zoom` (default true), `ranges[]`, `navigator`, `transforms[]`, `zero_toggle` |
| `SourceSpec` | `label`, `url`, `license`, `note` |

Engine diagnostics are `{level: error|warning, path: JSON pointer, message, hint?}` (`diag.rs:6-47`). They become `source-error`/`source-warning` checks in `.rmd` output.

## T-STYLE-FILE and sections: style files

A style is a YAML mapping with the sections `tokens`, `layout`, `chart`, `motion` and `components`; unknown top-level sections are ignored (observed (run)). References apply in order: frontmatter `styles`, then CLI `--style`, defaulting to `brutalist`. Merge is shallow and last-wins per section leaf; arrays replace (`style.rs:451-467`). A **single** invalid or unknown leaf (`style.schema`) skips the **whole file**, as `docs/RMD-SPEC.md:339` states: in the probe, an unknown token dropped that file's valid `accent` and `report-max-width` too. Retired leaves (`motion.duration|delay|stagger|easing`, `components.callout.left-bar-width`) give `style.retired` and the rest applies (`style.rs:155-177,237-246`). Observed (run). A reference is treated as inline YAML when it contains `:` or a newline; otherwise an unknown name → `style.unknown` warning (`style.rs:221-232`). Diagnostic paths are `styles[<index in merged refs>].<section>.<leaf>`, with no line.

| Section | Leaf | Accepted value | Conf. |
| --- | --- | --- | --- |
| `tokens` (T-STYLE-TOKENS) | `bg`, `bg-elev`, `bg-chart`, `ink`, `ink-soft`, `ink-faint`, `rule`, `rule-soft`, `accent`, `accent-soft`, `accent-2`, `link`, `up`, `down`, `hold`, `neutral`, `chart-ink` | Color ≤80 chars: `oklch\|lch\|lab\|oklab\|hsl(a)\|rgb(a)(…)`, `#hex` (3–8) or a bare keyword | observed |
| `tokens` | `font-text`, `font-display`, `font-ui`, `font-mono` | Font stack ≤200 chars of `[A-Za-z0-9_ .,'"+-]` | observed |
| `tokens` | `line-base`, `measure`, `radius-s`, `radius-m`, `size-small\|base\|h3\|h2\|h1\|display` and each `*-phone` (≤640 px) | Non-negative number, or ≤20-char dimension with unit `px ch rem em % vh vw fr s ms` | observed |
| `layout` (T-STYLE-LAYOUT) | `report-max-width` (dimension), `margin-column` (bool) | as left | observed |
| `motion` (T-STYLE-MOTION) | `enabled` (bool) | `false` renders charts settled | observed |
| `components` (T-STYLE-COMPONENTS) | `big.style` | `card`/`bracket`/`knockout`/`underline` | observed |
| `components` | `deck.style` | `italic`/`normal` (italic only when the text face embeds a true italic) | observed |
| `components` | `callout.border-width`, `callout.style` | dimension; `rule`/`bracket` | observed |
| `components` | `header.border-top`, `header.border-bottom` | dimension | observed |
| `components` | `rules.style` | `hairline`/`pencil` | observed |
| `components` | `title`, `heading`, `table-head`, `source-label` → `transform`, `weight`, `letter-spacing`, `font` | `none`/`uppercase`; `normal`/`bold`/`lighter`/`bolder`/1–1000; number or `normal`/length; `display`/`text`/`ui`/`mono` | observed |
| `chart` (T-STYLE-CHART) | validated: `line.smoothing`; colors `ink`, `ink-soft`, `inkSoft`, `grid`, `accent`, `background`, `muted`; fonts `font-family`, `fontFamily`; color arrays `colors`, `seriesColors` | as tokens | observed |
| `chart` | **unvalidated** in core: `palette` (name or color array), `ramp` (3 colors), `barRadius` (engine clamps 0–64), `lineWidth`, `areaOpacity`, `gridWidth`, `decals` (bool), `fontText`, any other key | The engine filters `palette`/`ramp` strings with `safe_css` for the SVG only; the print CSS does not (injection, observed (run)) | observed (run) |

Sources: `style.rs:472-500,516-671`; spec `docs/RMD-SPEC.md:335-353`. The spec's token list omits `chart-ink`, which builtins use and the code accepts (`docs/RMD-SPEC.md:343` vs `style.rs:557`). An unknown palette name in a style file silently gives the default light palette (`rmd_render.rs:1189,1231-1240`); in a chart block it is a schema error.

### T-STYLE-BUILTINS, T-CHART-PALETTE: builtins and palettes

Builtins: `brutalist` (default), `scientific`, `sleek` (= `sleek-dark`, dark), `sleek-light`, `hand-drawn`; `editorial` is a deprecated alias of brutalist with a `style.alias` warning (`style.rs:16-45,204-214`). A PDF swaps sleek/sleek-dark for sleek-light (`style.rs:871-880`). Builtin YAML lives in `crates/richview-core/assets/*.rstyle.yaml`; each sets `chart.palette` to its own name, a 3-stop OKLCH `ramp` and `barRadius` (3; hand-drawn 0); hand-drawn also sets `decals: true` and a 6-grey palette array. Named palettes have 8 OKLCH colors with the accent first (hand-drawn has 6); unknown names fall back to a light default (`rmd_render.rs:1189-1241`); the ramp fallback is `rmd_render.rs:3861`. Palette quality claims (3:1 contrast for the first four colors, ΔE thresholds under color-vision simulation) are stated at `docs/RMD-SPEC.md:168` and tested per `rmd_render.rs:1182-1188`; not re-run here.

## T-DIAGNOSTIC: diagnostics contract

Record (`crates/richview-core/src/lib.rs:58-71`): `{level, code, message, path, line?, tag?, details?}`. `level` ∈ `error | warning | info` (a string, not an enum). `details` carries the chart engine's original finding. JSON keys serialize alphabetically. Observed (run).

Level semantics: `error` means hard (render, validate or publish fails, CLI exit 1, MCP `isError`) or "this block or chart is not drawn as authored"; `warning` means it renders but may mislead; `info` is disclosure. Hard-failure errors come back as a bare diagnostic array (the core `Err`) rather than a report: `source.limit`, `report.root.*`, `report.asset.*`, `frontmatter.data`, `data.*`, `style.unresolved`, `render.output-limit` (`render.rs:65,91-99,137-145`; `folder.rs`). Observed (run).

### T-DIAG-STATUS: status and ok

Report `status` is `issues` if any `error` or `warning` exists, else `clean` (`render.rs:364-372`). Chart `status` is `issues` if any non-`info` check exists (`rmd_render.rs:1043-1047`). `unmeasured` is accepted by schemas and TS types but never emitted. CLI `ok` means no `error`; exit 1 when not ok (`cli.rs:548-550,582-586`). `check` therefore exits 0 with `status: issues` on warnings (observed (run)). `validate` covers parse, schema and folder rules only; engine errors such as `chart.missing-encoding` appear only in `check`, `generate` and the MCP render/check tools (observed (run)).

### T-DIAG-LINES: line mapping

| Diagnostic family | `line` | `path` | Conf. |
| --- | --- | --- | --- |
| Block schema/YAML/title, `data.local`, parser `report.asset.escape` | Fence line, one-based, including frontmatter | `tag.field` (`block.yaml`: `blocks[N]`) | observed (run) |
| `data.*` resolution | Fence line | Data reference | observed (run) |
| Chart engine (`chart.*`) | Fence line | Chart id `rv-chart-<index among valid blocks>`; `details.path` holds a JSON pointer such as `/data/inline/1/k` | observed (run) |
| Frontmatter YAML/type/unclosed | 1 | `frontmatter` | observed (run) |
| Frontmatter field warnings | none | `frontmatter.<key>` | observed (run) |
| Folder/asset (`report.*`), style, Markdown, `render.*` | none | asset location, `styles[i]…`, `markdown.*`, `options.*` | observed (run) |
| VS Code host | adds an LSP `range` (zero-based, UTF-16), mapping YAML errors and chart `details.path` into exact scalars | | observed (code: `crates/richview-editor/src/authoring.rs:182-344`) |

### T-DIAG-CODES: code catalog

| Family | Codes (level) | Effect | Source |
| --- | --- | --- | --- |
| Source | `source.limit` (E) | Hard | `parser.rs:16` |
| Frontmatter | `frontmatter.yaml`, `frontmatter.type`, `frontmatter.unclosed`, `frontmatter.version`, `frontmatter.theme` (W); `frontmatter.data` (E) | Field dropped or reset; `data` is hard | `parser.rs:57-266` |
| Blocks | `block.yaml`, `block.type`, `block.schema` (E); `block.title` (W) | Block stays code; title fallback | `parser.rs:140-292` |
| Data | `data.local` (E, parse); `data.unresolved`, `data.limit`, `data.json`, `data.csv`, `data.csv.header` (E, hard) | | `parser.rs:689-693`; `blocks.rs:34-148` |
| Report folder | `report.root.missing`, `report.root.invalid`, `report.root.escape`, `report.root.shared`, `report.root.unresolved`, `report.asset.escape`, `report.asset.missing` (E, hard) | | `folder.rs`; `resolver.rs` |
| Output | `report.output.escape`, `report.output.invalid`, `report.output.collision`, `report.output.runtime` (E); `render.output-limit` (E); `render.width` (W); `render.output.invalid`, `render.page.invalid` (E, JS API) | | `resolver.rs:555-673`; `render.rs:75,565,615,628` |
| Publish | `publish.metadata` (E), `publish.description` (W) | | `parser.rs:727-745` |
| Style | `style.unresolved` (E, hard); `style.yaml`, `style.type`, `style.schema`, `style.retired`, `style.unknown`, `style.alias`, `style.fonts-unavailable` (W); `style.properties`, `options.style` (E, JS API) | Invalid style skipped | `style.rs`; `render.rs:93` |
| Security | `security.media`, `security.embed`, `security.link`, `security.image`, `security.html`, `security.runtime-url` (W) | Blocked output | `blocks.rs`; `markdown.rs`; `render.rs:327` |
| Assets | `assets.network-dependent`, `assets.external-reference`, `assets.runtime-external` (I) | Sets `_richview.offline: false` | `blocks.rs:677,706,844`; `markdown.rs:174-176,215`; `render.rs:325` |
| Markdown | `markdown.math` (W) | | `markdown.rs:487` |
| Chart | `chart.<check kind>` (level = severity), `chart.label-dropped` (I, from dropped labels), `chart.font-bounds-estimated` (I) or `chart.unmeasured` (W) | | `render.rs:657-712` |
| PDF | `export.browser-missing`, `export.platform`, `export.browser-failed`, `export.settle-timeout`, `export.image-failed` (E); `export.browser-version`, `export.font-fallback`, `export.pdf-untagged`, `export.network-blocked`, `export.video-poster` (W); `export.font-check`, `export.local-link`, `export.browser-sandbox` (I) | Not run here | `export_pdf.rs:76-828`; `blocks.rs:720` |
| Editor only | `authoring.key` (W), `authoring.field` (E), `resource.resolve` (E) | VS Code only | `crates/richview-editor/src/authoring.rs:360-391`; `crates/richview-editor/src/main.rs:161` |
| Schema adapter | `schema.invalid` (E) | JS `.parse` / `.safeParse` | `compat.rs:279-281` |
| Asset references | `asset.*` (see T-ASSET-REF) | Upload commands only | `asset.rs` |

Note that the chart code `chart.label-dropped` is used twice with different levels: as a `warning` check for category names that do not fit (`rmd_render.rs:3503`) and as an `info` diagnostic generated from `droppedLabels` (`render.rs:680-694`). Observed.

### T-CHART-DIAGNOSTICS, T-CHART-CHECK, T-DROPPED-LABEL: chart diagnostics

Per-chart object, observed (run) keys: `chartId`, `sourceLine` (both added by core, `render.rs:155-158`), `chartType`, `engine` (always `rust-svg`, even for calendar charts), `measured: false`, `geometryMeasured: true`, `fontBounds`, `measurement`, `status`, `checks[]`, `droppedLabels[]`, `dimensions {width, height}` (`rmd_render.rs:1050`). Check kinds and severities: errors `ambiguous-bar-slot`, `ambiguous-missing-cell`, `ambiguous-numeric-cell`, `invalid-bubble-size`, `invalid-color-values`, `missing-color-values`, `missing-encoding`, `negative-pie-value`, `non-numeric-x`, `unknown-annotation`, `unlocated-color-values`, `unsupported-chart`, `value-overflow`, `histogram`, `boxplot`, `source-error`; warnings `clipped-distribution`, `clipped-interval`, `clipped-measure`, `clipping`, `collapsed-interval`, `empty-region`, `excluded-observations`, `fixed-height`, `label-collision`, `label-dropped`, `missing-bubble-size`, `non-numeric-y`, `series-differentiation`, `source-warning`, `unresolved-annotation`; info `aggregation`, `binning`, `boxplot-summary`, `decimation`, `interval-summary`, `label-thinned`. Checks may carry `region {x,y,w,h}`, `related[]`, `path`, `hint`, `facet` and kind-specific counts. Dropped labels are `{text, reason, wouldBeAt?, collidedWith?, kind?}`. Emitted reasons include `density`, `no collision-free position` and `label density limit` (`rmd_render.rs:777,1462,2069,2395`), but the public `DroppedLabelSchema`, `ChartDiagnosticsSchema` and the TS type accept only `collision | out-of-bounds | density` (`compat.rs:345-349`; `core-api.d.ts:27`). Validating real output with those schemas can therefore fail (inferred from code; not run). The browser tier adds measured glyph checks in `data-rvc-diagnostics` and `rvc:diagnostics` (`docs/self-validating-charts.md:7`; stated).

## T-REPORT-SLUG, T-REPORT-VERSION, T-STORED-REPORT, T-PUBLISH-REQUEST: report identity

| Rule | Detail | Source | Conf. |
| --- | --- | --- | --- |
| Slug choice | `--slug`/MCP `slug` → frontmatter `slug` → title-derived (lowercase, runs of `[^a-z0-9]` → `-`, trimmed, ≤80 chars) → `untitled`. **Any** `frontmatter.*` diagnostic discards the frontmatter, so the slug becomes `untitled`. | `compat.rs:17-49` | observed (run) |
| Slug grammar | 1–80 of `[a-z0-9_-]`; an invalid authored slug is a hard publish error (plain string error, `diagnostics: null`) | `storage.rs:290-301` | observed (run) |
| Version grammar | Trimmed; ≤64 bytes; no surrounding whitespace in storage; semver-shaped regex (leading zeros allowed) | `compat.rs:236-246`; `storage.rs:303-311` | observed (run) |
| Publish preflight (client) | `validate_publish` (title and version required, description warns) + folder validation + slug check; HTTPS required except loopback; 30 s timeout; no redirects | `cli.rs:428-504` | observed (run) |
| Request | `POST /api/reports` with bearer token; body `{slug, source, styles, data, report_folder}`; runtime code is never sent | `cli.rs:476`; `server.rs:52-62` | observed (code + test `cli.rs:637-702`) |
| Server checks | Token (constant-time compare; 503 if the server has none), slug, non-empty title, `validate_version`; renders once; rejects any error diagnostic | `server.rs:220-304` | observed (run) |
| Storage layout | `<data-dir>/<slug>/<version>/{source.rmd, rendered.html, meta.json, assets.json}` and `<slug>/latest.txt`; staging dir + rename; symlinks rejected | `storage.rs:56-118` | observed (run) |
| Immutability | An existing `{slug, version}` is never rewritten: republish returns the stored artifact with `created: false` (and CLI exit 0) **even when the source changed** | `storage.rs:72-74` | observed (run) |
| Latest | `latest.txt` = the most recently **created** version, even when its semver is lower | `storage.rs:98-100` | observed (run) |
| Retention | At most 50 versions per slug; older (lowest-semver) versions are **deleted**, so their permalinks 404 | `storage.rs:13,101-111` | observed (run) |
| Regeneration | `POST /api/reports/{slug}/v/{version}/regenerate` re-renders retained source/styles/data and returns render JSON without storing | `server.rs:401-424` | observed (code) |

Stated intent: versions exist "to preserve consistency as the format changes", and raw `.rmd`/style sources are kept for regeneration (the Linear publishing and version model issue). The earlier main-checkout `CLAUDE.md` describes storage as "append-only per-slug … idempotent per `{slug,version}` (dec-67/68/73)"; the code is idempotent but not append-only, because of the 50-version pruning.

## T-CLI-JSON, T-MCP-TOOLS, T-HTTP-API, T-JS-API: output contracts

### T-CLI-JSON: CLI JSON output (`--format json`)

Source: `crates/richview-app/src/cli.rs`. Keys are alphabetical; output is one line of JSON.

| Command | Success shape | Source | Conf. |
| --- | --- | --- | --- |
| `generate` (report) | `{ok, output, blocks, diagnostics[], renderDiagnostics, interactive}` | `cli.rs:269-282` | observed (run) |
| `generate --to pdf` | `{ok, output, to: "pdf", page, pages, bytes, browser, settled, blocks, diagnostics[], renderDiagnostics}` | `cli.rs:241-267` | observed (code) |
| `validate` | `{ok, blocks, diagnostics[]}` | `cli.rs:284-308` | observed (run) |
| `check` | `{status, diagnostics[], charts[]}`, with **no `ok` key**; `charts` holds the T-CHART-DIAGNOSTICS objects | `cli.rs:309-328`; `lib.rs:223-228` | observed (run) |
| `publish` | Server JSON `{ok, slug, version, url, created, permalink}`, with `url` rewritten to `<server>/r/<slug>` | `cli.rs:396-423,496-503` | observed (run) |
| Any failure | `{ok: false, error: <string>, diagnostics: <Diagnostic[] or null>}`, exit 1. `error` repeats the serialized array when the failure is diagnostic, otherwise it is plain text and `diagnostics` is `null`. | `cli.rs:205-216` | observed (run) |
| `renderDiagnostics` | `{status, parse (non-`chart.*` diagnostics), diagnostics (all), charts}` | `server.rs:211-218` | observed (run) |

`learn` prints plain text (sections `grammar`, `chart-contract`, `style`, `workflow`, `anti-slop`; `knowledge.rs:11-19`). `serve` previews on 127.0.0.1:4711 by default; `server` serves on 127.0.0.1:8080. The `publish` default server is `https://richview.uk` (`cli.rs:122`), while every report footer links to `https://richview.ai` (`render.rs:353`). Observed.

### T-MCP-TOOLS: MCP tools

Newline-framed JSON-RPC over stdio; `initialize` answers protocol `2025-06-18` (or the client's version) with tools and resources capabilities (`mcp.rs:294-368`). Seven tools, observed (run) via `tools/list`:

| Tool | Inputs | Output (`structuredContent`) | `isError` | Conf. |
| --- | --- | --- | --- | --- |
| `richview_render` | `source` or `input_path`, `report_folder`, `theme`, `styles[]`, `data{}`, `output_path`, `output` (`report`/`pdf`), `page` (`a4`/`letter`) | `{output_path, blocks, has_charts, errors, diagnostics, renderDiagnostics, interactive}`; PDF adds `output, page, pages, bytes, browser, settled`. **Requires `input_path`** to write output; source-only calls fail after rendering. | Hard errors | observed (code) |
| `richview_validate` | source fields | `{ok, blocks, diagnostics}` | Errors present | observed (run) |
| `richview_check` | source fields + `chart_width` 240–4096 | `{status, diagnostics, charts}` | Errors present | observed (run) |
| `richview_publish` | source fields + `server` (required), `token` (or `RICHVIEW_TOKEN`), `slug` | Publish response + `url` | Failures | observed (code) |
| `richview_learn` | `section` enum | Text content only | — | observed (run) |
| `richview_asset_put` | `path`, `store` (required), `public`, `type` | Asset reference result | Failures | observed (code) |
| `richview_asset_verify` | `asset` (required), `store`, `hash` | Verification result | Failures | observed (code) |

Resources: `richview://spec/rmd` (the learn bundle) and `richview://examples/hello-world` (the `init` template) (`mcp.rs:48-61`). Diagnostic failures return `{ok: false, diagnostics}` with `isError: true`; other failures return text (`mcp.rs:82-88`). `docs/AGENT-QUICKSTART.md:15` lists the same seven tools; the `learn` workflow text lists five (`knowledge.rs:16`).

### T-HTTP-API: server routes

Source: `crates/richview-app/src/server.rs:64-95`.

| Route | Method | Contract | Conf. |
| --- | --- | --- | --- |
| `/health` | GET | `{ok: true, engine: "rust"}` | observed (run) |
| `/api/render` | POST | Request `{source, report_folder?, theme?, styles[], data{}, width?}`, where `report_folder` is effectively required. 200: `{html, has_charts, diagnostics, renderDiagnostics, metadata}`; 422: `{error: "Render failed", diagnostics}`. Not authenticated. | observed (code) |
| `/api/reports` | GET / POST | List summaries / publish (bearer token) | observed (run) |
| `/api/reports/{slug}` | GET | `{slug, version, frontmatter, updated_at, versions[]}` | observed (run) |
| `/api/reports/{slug}/versions` | GET | Semver-descending array | observed (run) |
| `/api/reports/{slug}/v/{version}` | GET | Full stored bundle, including source, html and resolved data; public | observed (code) |
| `/api/reports/{slug}/v/{version}/regenerate` | POST | Render JSON; not authenticated | observed (code) |
| `/r/{slug}`, `/r/{slug}/v/{version}` | GET | HTML; latest is `no-cache`, versioned is `immutable, max-age=31536000` | observed (run) |
| `/raw/{slug}`, `/raw/{slug}/v/{version}` | GET | `text/markdown` source | observed (run) |
| any | — | Host `therichview.com` → 301 to `https://richview.uk…`; adds `x-richview: v3-rust`, `nosniff` and a referrer policy | observed (code) |

### T-JS-API, T-RENDER-METADATA, T-CHART-RUNTIME-PAYLOAD, T-RENDER-OPTIONS: JS API, metadata and payloads

JS package API (`assets/node/core-api.d.ts`; bindings `crates/richview-web/src/browser.rs:150-260`), observed (code):

- `render(source, options)` returns `{html, parsed, hasCharts, diagnostics: {status, charts, parse}}`, where `parse` excludes `chart.*` (`render.rs:578-655`).
- `check()` returns that `diagnostics` object.
- `diagnose(parsed, options)` returns `{status, charts, parse(all)}`.
- `validate()` returns `Diagnostic[]`.
- `renderChart(chart, id, {width, height, theme})` returns `{svg, diagnostics}`; `height` sets `_renderHeight`.
- Schemas with `.parse`/`.safeParse` are `ChartSchema`, `TableSchema`, `BigSchema`, `CalloutSchema`, `EmbedSchema`, `ImageSchema`, `DocumentFrontmatterSchema`, `versionString`, `DataSourceSchema`, `ChartDiagnosticsSchema` and `RenderDiagnosticsSchema`; they strip unknown keys through retain lists (`compat.rs:126-232`).
- The JS options use camelCase `chartWidth` and `reportCard`, and `styles` as `StyleRef[]`.

`RenderOptions` (Rust, `lib.rs:119-165`): `styles[]`, `report_folder?`, `data{ref: string}`, `width` (720), `runtime_js?`, `runtime_url?`, `runtime_wasm?`, `runtime_wasm_codec` (`raw`/`gzip`), `theme?`, `source?`, `report_card`, `output` (`report`/`pdf`), `page` (`a4`/`letter`; margins 20/18/22 mm). Report `metadata` is the frontmatter plus `_richview {generator, source, block_count, chart_count, status, offline, interactive, chart_diagnostics}` (`render.rs:373`). The chart runtime payload, embedded as `<script type="application/json" data-rvc-runtime>`, is `{kind: rmd|time-series|facets, chart (full block incl. unknown and underscore keys), spec? (time-series), children? (facets), facet?, id, width, motion, theme {palette, ramp, barRadius, fontFamily, …}, output? (pdf)}` (observed (run); `render.rs:160-184`).

## Spec, doc and code disagreements (proposed decisions)

| Decision ID | Disagreement | Evidence | Choice it forces |
| --- | --- | --- | --- |
| D-TYP-STYLE-INJECTION | Style `chart.palette`/`ramp` reach the `<style>` element unvalidated; docs promise closed grammars and no executable report source | `style.rs:632-650,906-927`; probe s3; `docs/RMD-SPEC.md:333,339` | Validate every CSS-bound chart leaf in core (and decide whether `chart` stays "supplier-owned"), or escape at every sink |
| D-TYP-REMOTE-MEDIA | Remote media and Markdown images are hard errors; the spec says they are diagnosed and leave the report non-offline | `folder.rs:218-233`; probes h3/h6; `docs/RMD-SPEC.md:323`; `blocks.rs:676-678` (unreachable info path) | Keep strictly report-local media, allow remote media with diagnostics, or require `AssetRef` pinning |
| D-TYP-ASSET-REF | `AssetRef` exists for uploads, but `.rmd` cannot reference it | `asset.rs`; `asset_upload/mod.rs:312-336`; `parser.rs:626-629` (src must be a string) | Wire pinned references into `src`/`data`, or drop them from the rewrite |
| D-TYP-INTERNAL-KEYS | `_renderHeight`, `_xWindow`, `_csvNumericText`, `_facetColorScope` are honoured from source | probe `_renderHeight: 900`; `docs/RMD-SPEC.md:56,90` | Strip reserved keys at parse, or pass host state out of band |
| D-TYP-VERSION-RETENTION | Versions pruned at 50 vs "immutable" permalinks | `storage.rs:13,101-111`; probe p5 | Keep a cap (and document it), or make permalinks permanent |
| D-TYP-REPUBLISH | Same version with changed content → silent `ok: true, created: false`; latest follows creation order, not semver | `storage.rs:72-74,98-100`; probe p1 | Conflict error on differing content (`content_sha256` already exists), and latest by semver or by creation |
| D-TYP-SLUG-FALLBACK | Any frontmatter warning makes the slug `untitled`, ignoring an authored slug and the title | `compat.rs:22-31`; probe p2 | Derive the slug from validated fields individually, or fail publication |
| D-TYP-DESCRIPTION | The Linear vNext frontmatter issue (stated): title, description, version required; code: description only warns | Linear; `parser.rs:735-747` | Required or recommended `description` |
| D-TYP-COLUMN-ORDER | Derived table columns, CSV columns and payload keys are alphabetical | `blocks.rs:385-398`; probe csv1; `Cargo.lock` serde_json 1.0.151 has no `indexmap` dependency, so `preserve_order` is off | Preserve source order (`serde_json/preserve_order` or explicit column lists) |
| D-TYP-FIELD-CHECKS | Unknown encoding/column fields: CLI/MCP `clean`, VS Code `authoring.field` error; missing `encoding.y` passes `validate` | probes f1 and my; `crates/richview-editor/src/authoring.rs:374-391` | One field-existence rule in core for every host, plus which channels are required per type |
| D-TYP-RATIO | Calendar line/area default 2:1 vs spec 16:9; bad `style.ratio` strings and out-of-range calendar ratios ignored silently; legacy top-level `ratio` works only on calendar charts | `render.rs:373`; `rmd.rs:246-269`; `rmd_render.rs:571-587`; probes charts/tr | Single default-ratio table, ratio validation with diagnostics, and drop top-level `ratio` |
| D-TYP-LINES | Frontmatter-field, style, Markdown and asset diagnostics have no line | probes fm1/s1/md/h3 | Require source positions on every source-derived diagnostic |
| D-TYP-VOCABULARIES | Five hand-kept lists diverge: `locale` accepted on every axis (spec: x only; editor: x only; public schema keeps it on x and y); TS `Chart.style` lacks `line`; TS `Diagnostic` lacks `details`; TS `Media` lacks `alt`/`title`; TS `ChartDiagnostics` lacks `sourceLine`, `geometryMeasured`, `fontBounds`; the editor offers `rotate` on all annotations; dropped-label reasons exceed the schema enum | `parser.rs:393-405`; `compat.rs:149-173,345-349`; `crates/richview-core/src/authoring.rs:86,97`; `core-api.d.ts:12,20,21,27,28` | Generate schemas, TS types, editor vocabulary and spec tables from one source |
| D-TYP-DOC-DRIFT | `docs/AGENT-QUICKSTART.md:13` says "all six chart types" (there are nine); `docs/RUST-ARCHITECTURE.md:19` describes opt-in remote HTTPS loading and caller-authority style paths, while the code ignores `--allow-network` and confines `--style` to the report folder (`resolver.rs:201-205,245-267`; `docs/RMD-SPEC.md:247,263` agree with the code); `knowledge.rs:16` lists five MCP tools (seven exist); the main-checkout `CLAUDE.md` describes the obsolete TypeScript layout (six tags, Zod, frontmatter `data:` map, five tools) | as cited | Which document is canonical for the rewrite |
| D-TYP-METRIC-UNIT | Metric `format: percent` + `unit: "%"` → `68.2%%`; non-numeric value + unit → `n/a%`; delta color follows `trend`, not sign | probe charts; `blocks.rs:296-338` | Unit/format rules for metrics |
| D-TYP-EMBED-LIST | Two default embed allowlists (6 hosts at parse, 7 at render incl. `youtube-nocookie.com`) | `compat.rs:264-274`; `blocks.rs:800-863`; probe all | One list |
| D-TYP-WIDTH-RANGE | Chart width: core 240–8192 (warn), hosts 240–4096 (hard) | `render.rs:70-80`; `cli.rs:310`; `mcp.rs:207`; `server.rs:161` | One range |
| D-TYP-DOMAIN | Publish default `https://richview.uk` and therichview.com redirect target vs report footer `https://richview.ai` | `cli.rs:122`; `server.rs:107-121`; `render.rs:353` | Canonical public host |
| D-TYP-UNTITLED-CHART | "Fallback title" in the spec, but the caption has no visible title | `docs/RMD-SPEC.md:56`; probe charts | Visible fallback or hard requirement |

## Pending format proposals on open PRs (not on main)

| PR | Branch head | Proposed type change | Source | Conf. |
| --- | --- | --- | --- | --- |
| #63 (To Review) | `f1cb254`, 2026-10-18 | Frontmatter `tabs {default, export: all\|active, panels[{id, label, from}]}`, heading anchors `{#id}`, codes `frontmatter.tabs`, `tabs.anchor-missing` | `git diff origin/main...f1cb254 -- docs/RMD-SPEC.md` | stated |
| #66 | `4ecef05`, 2026-10-18 | `data.sql {connector, query\|inline}`, `data.metricflow {connector, metrics[]}`, `data.cache {key}`; `data.local` widened. This reverses main's "SQL loading is not report-local" error. | `crates/richview-core/src/parser.rs` diff on that branch | stated |
| #62 (To Review) | `a703897`, 2026-10-18 | Chart `controls[]` with `playback`/filter/metric/variant dimensions, engine `ControlsSpec`, `data-rvc-controls` envelope, `rv:control`/`rv:control-set` events | `docs/CONTROLS.md` on that branch | stated |

Linear also holds older format intents in Brain Dump status: the vNext frontmatter, versioning and media blocks issue, the content plus cascading style files issue and the publishing and version model issue. The table syntax decision and the inline-data default are both in `Que` status. A further issue records Daniel's 2026-10-04 requirement of one dedicated folder per report, failing fast otherwise, with output under `Rendered/`.

## Gaps

- PDF output, the browser runtime (glyph measurement, interactions, readouts, zoom), the Wasm/JS package and the VS Code extension were not run; their rows rest on code reading or the spec.
- Spec rules for chart geometry and statistics were not re-verified: bar rounding, label alignment, pie key, heatmap ramp math, percent and scientific axis formatting, R-7 quartiles, palette contrast. Those rows are marked stated.
- Area stacking from `.rmd` (multiple colour groups) was not probed; stacked areas are documented only for the engine `ChartSpec`.
- Code-verified facts that were not reproduced with a probe: `_xWindow`/`_csvNumericText` injection, `render.output-limit` behaviour, the 32 MiB host limits, asset upload commands and authenticated failure responses.
- Linear was sampled only for the publishing and version model, vNext frontmatter, inline-data default, table syntax and report folder issues and a project query. Notion and richview.ai were not consulted for this file.
