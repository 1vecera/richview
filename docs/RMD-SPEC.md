# Rich Markdown source contract

The reference tables in this spec (tags, frontmatter fields, each block's fields, chart types and diagnostic codes) are generated from the schema registry in `crates/richview-core/src/registry.rs` and checked by the gate; edit the registry, then run `RICHVIEW_BLESS=1 cargo test -p richview-core --test spec_docs` to refresh them. The prose explains the rules but does not list the vocabulary.

This document describes the Rust implementation in RichView v3 alpha. Rich Markdown (`.rmd`) is CommonMark with YAML frontmatter and tagged YAML fences. An ordinary Markdown renderer displays the rich fences as code. RichView decodes supported fences into report elements; unknown fences remain ordinary code, and invalid rich fences remain visible with source diagnostics.

The content source, resolved data and ordered `.rstyle.yaml` sources remain editable and are retained for report regeneration. The core performs no filesystem or network I/O; native/browser hosts resolve resources before calling it.

## YAML profile

Frontmatter, rich fences and `.rstyle.yaml` files are read by one profile: the YAML 1.2 core schema with these limits (TYP-02). Every reader uses it, so two hosts never disagree about what a source says.

| Construct | Rule |
| --- | --- |
| A key repeated in one mapping | Error `yaml.duplicate-key` at the second key's line |
| Anchors, aliases, merge keys (`<<`), explicit tags, a second document | Error `yaml.unsupported`; write the value out in full |
| `.inf`, `.nan`, a number too large to hold | Error `yaml.unsupported`; numbers are finite |
| `yes`, `no`, `on`, `off`, unquoted dates, `1_000`, `0b11` | Text |
| `007` and other integers with a leading zero | Text (the core schema would read 7) |
| `0x10`, `0o17`, `1e3` | Numbers |
| `~`, `null`, an empty value | Null |
| Quoted and block scalars | Always text |

Text that is not YAML at all keeps its family's code (`frontmatter.yaml`, `block.yaml`, `style.yaml`) with the parser's line and column.

## Report identity

A published report is the pair `{slug, version}`. One function derives slugs everywhere (publish, wikilinks, server): Unicode NFKD, drop combining marks, transliterate the remaining Latin letters (`ß` to `ss`, `ł` to `l`, `đ` to `d`, `ø` to `o`, `æ` to `ae`, `œ` to `oe`), lowercase, turn each run of other characters into one hyphen, trim, and cut at the last hyphen before 80 characters. `Průzkum žluťoučkého koně v Česku 2026` becomes `pruzkum-zlutouckeho-kone-v-cesku-2026`. A title with no Latin letters or digits yields no slug.

The publish slug is the `--slug` option or MCP `slug`, else frontmatter `slug`, else the title. Each is judged alone: a problem in another frontmatter field never changes it, and there is no `untitled` fallback. A `[[wikilink]]` target that already matches the slug grammar is used as written; any other target is derived, and one with no letters or digits points at `report`.

A `{slug, version}` is immutable. Publishing identical content again returns `created: false`; different content is `publish.version-exists` (HTTP 409) naming the stored hash, and the author bumps the version. Identity is `contentSha256`, the SHA-256 of the `.rmd` source, the ordered styles and every file sorted by path, each with its byte length; slug, version, rendered HTML and engine version are not part of it. Versions are never pruned. A server may set a byte quota (`--quota-bytes`, `RICHVIEW_QUOTA_BYTES`); a publication past it is `publish.quota` (HTTP 507) and stores nothing. `DELETE /api/reports/{slug}/v/{version}` removes a version and leaves a tombstone: its permalink answers 410 and the number can never be published again (`publish.version-deleted`). `/r/{slug}` serves the highest release by SemVer precedence, a pre-release only while no release exists, unless the owner pins a version with `PUT /api/reports/{slug}/latest` (body `{"version": "..."}`) and removes the pin with `DELETE`.

## Document metadata

Frontmatter starts at the beginning of the file with `---` and closes with `---` or `...`. Local drafts need no metadata. Publication requires a nonempty `title`, a `description` and a valid `version`; a missing one is `publish.metadata`. A source marks its format with `format: rmd/1`; a newer major version is the error `frontmatter.format`. A `version` is a quoted string `MAJOR.MINOR.PATCH` with an optional `-prerelease`, under SemVer 2.0.0 numeric rules: no leading zeros, no `+build` metadata, no surrounding whitespace, at most 64 bytes. A YAML number such as `1.0` is `frontmatter.type` (quote it), and an invalid string is `frontmatter.version`; both leave the version unset, so publishing fails with `publish.metadata`.

````rmd
---
title: Revenue rose in Q3
description: Retained customers drove most of the improvement.
version: 0.0.1
author: Daniel Vecera
date: 2026-10-04
styles: [scientific]
---

<!-- registry:begin frontmatter -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `title` | text |  |  | The report title; publishing requires it, and a draft without one shows `Untitled report`. |
| `description` | text |  |  | The deck under the title; publishing requires it. |
| `author` | text |  |  | The author, as plain text. |
| `date` | text |  |  | The report's date, `YYYY-MM-DD` or RFC 3339. |
| `slug` | text |  |  | The address name the report is published under. |
| `version` | text |  |  | The semver version; publishing requires it. |
| `lang` | text |  | `en` | The report's language as a BCP 47 tag: it sets the page language, quotes and number separators. |
| `format` | text |  | `rmd/1` | The format version the report was written for; a newer major version is refused. |
| `tags` | text or list of text |  |  | Labels for the report; one string is one tag. |
| `styles` | text or list of text |  |  | Style names or report-relative style files; the first is the base and later ones override it. One string is one entry. |
| `title_image` | text or mapping |  |  | The header image; a bare source string stands for `{src}`. |
| `title_image.src` | text | yes |  | The image: a report-relative path, a data address or a credited Unsplash photo address. |
| `title_image.alt` | text |  |  | The text alternative. |
| `title_image.caption` | text |  |  | The caption under the image. |
| `title_image.fit` | one of `contain`, `cover` or `fill` |  | `contain` | How the image fills its box. |
| `title_image.width` | whole number |  |  | The image width in pixels; a positive whole number. |
| `title_image.height` | whole number |  |  | The image height in pixels; a positive whole number. |
| `title_image.credit` | mapping |  |  | The photo credit; an Unsplash photo needs one. |
| `title_image.credit.author` | text | yes |  | The photographer's name. |
| `title_image.credit.author_url` | text |  |  | The photographer's profile; required on an Unsplash photo. |
| `title_image.credit.source` | text |  |  | The site the photo comes from. |
| `title_image.credit.source_url` | text |  |  | The photo's page on the source site. |
| `title_image.color` | any value |  |  | A #rrggbb colour that fills the box until the photo loads. |
| `publish` | any value |  |  | Publication metadata for RichView Publish; other hosts ignore it. |
| `tabs` | mapping |  |  | Splits the body into tab panels at heading anchors. |
| `tabs.default` | text |  |  | The panel shown first; an omitted or unknown string selects the first panel. |
| `tabs.export` | one of `all` or `active` |  | `all` | Whether printed and PDF output holds every panel or only the active one. |
| `tabs.panels` | list of mapping | yes |  | The panels, in document order; at least one. |
| `tabs.panels[n].id` | text | yes |  | The panel id, unique, using the report slug grammar; it names the #tab link. |
| `tabs.panels[n].label` | text | yes |  | The text on the panel's tab. |
| `tabs.panels[n].from` | text | yes |  | The explicit heading anchor where the panel starts. |
| `toc` | `true` or `false` |  | `true` | Lists the report's sections beside the body, following the scroll, or under the header on a narrow screen; a report with fewer than three sections, or with tabs, has none. |
| `progress` | `true` or `false` |  | `true` | Shows how far the reader has scrolled as a thin bar in the report's colours along the bottom of the window. |
<!-- registry:end -->

## The evidence

```chart
type: bar
title: Revenue increased across both months
data:
  inline:
    - month: Jul
      revenue: 100
    - month: Aug
      revenue: 140
encoding: {x: month, y: revenue}
```
````

| Field | Accepted value | Behavior |
| --- | --- | --- |
| `title` | string | Report heading and HTML title; draft fallback is `Untitled report`. Required for publication. |
| `description` | string | The one deck under the title. Required for publication. |
| `version` | strict SemVer string, no build metadata | Immutable publication identity; start at `0.0.1`. |
| `author`, `date` | strings | Escaped display text. A date is `YYYY-MM-DD` or an RFC 3339 timestamp; author text is not evaluated as Markdown. |
| `slug` | 1-80 characters of `a-z`, `0-9` and `-`, with no leading, trailing or double hyphen | Publication slug. An invalid value warns as `frontmatter.slug` and fails publishing as `publish.slug`; it is never rewritten. Without `slug` it is derived from the title. |
| `lang` | BCP 47 tag | The report language: `lang` on the page, calendar labels and the decimal and grouping separators of tables, metrics and charts. |
| `format` | `rmd/<major>` | The format marker; a newer major version is refused, and retired fields require source edits. |
| `tags` | string or string array | Retained report metadata; a bare string is a list of one. |
| `styles` | string or ordered string array | Builtin names or resource references, followed by caller/CLI style overrides. |
| `title_image` | source string or image mapping | Header image; object form supports the image block fields, including `credit`. |
| `publish` | mapping | Publication settings read by the host. |
| `tabs` | mapping | Splits the body into views that readers switch between. See [Tabs](#tabs). |

Retired frontmatter keys fail with their own code and replacement. [Renamed and removed names](#renamed-and-removed-names) lists these from the registry. Repair the named fields in source; no migration command is exposed.

An unknown frontmatter key stays in the metadata and warns `frontmatter.unknown-key`, naming the nearest known key. Malformed known fields produce diagnostics and degrade gracefully. Numeric text is not substituted for metadata strings.

### Tabs

`tabs` splits one report's body into views that readers switch between, such as Overview, Sales and Customers. It is optional: reports without it render exactly as before.

````rmd
---
tabs:
  default: overview            # optional; otherwise the first panel
  export: all                  # all (default) | active
  panels:
    - { id: overview,  label: Overview,  from: overview }
    - { id: sales,     label: Sales,     from: sales }
    - { id: customers, label: Customers, from: customers }
---

## Overview {#overview}

Summary.

## Sales {#sales}

Sales detail.

## Customers {#customers}

Customer detail.
````

Each panel starts at a heading with an explicit anchor, such as `## Sales {#sales}`, and runs to the next panel's heading or the end of the body. An explicit heading id is kept only when a panel names it; a generated id alone cannot start a panel. Content before the first panel stays above the tabs and is never hidden. Prose, rich fences, charts and tables inside a panel behave as anywhere else.

Panel IDs and explicit heading anchors use the report slug grammar. The panel ID names deep links such as `#tab=sales`; panels list their starting anchors in document order. The generated frontmatter reference above defines the fields and value types.

The interactive report, which loads the browser runtime, renders a `tablist` of buttons and `tabpanel` sections. Inactive panels carry `hidden` but stay in the DOM, so chart state and readouts survive a switch. Clicking a tab or pressing `←`/`→` (wrapping), `Home` or `End` selects it and moves focus with the selection; `Tab` moves on to the panel content. Selection replaces the URL fragment with `#tab=<id>` without adding history entries, and loading or changing to a known `#tab=<id>` selects that panel. Charts in a hidden panel mount when the panel is first shown.

The HTML report holds every panel in order. The runtime adds the tab buttons and hides inactive panels when it mounts; before that, without JavaScript or when the runtime cannot start, every panel remains readable without buttons. On narrow screens the tab list scrolls horizontally on its own.

A malformed tab configuration, including invalid field types or an unsupported export value, warns and renders the complete body without tabs. Missing, repeated or out-of-order starting anchors follow the same recovery. An unknown string default remains recoverable: it warns and selects the first panel.

## Rich fences

The fence tags:

<!-- registry:begin tags -->
| Tag | Block |
| --- | --- |
| `chart` | A chart drawn from block-local data. |
| `table` | A table drawn from block-local data. |
| `metric` | A single large number with its caption and change. |
| `callout` | A highlighted note with a direction. |
| `image` | An image from the report folder or a credited Unsplash photo. |
| `embed` | An iframe of an allowed https host. |
| `scrolly` | One image that stays in view beside narrative steps, each naming the state the figure takes. |
| `audio` | An audio player. |
| `video` | A video player. |
<!-- registry:end -->

Rich block bodies must be YAML mappings. Fence recognition follows CommonMark, including longer fences and quoted code examples; diagnostic lines include the frontmatter.

An unknown key in a rich block, at any depth, is a `block.unknown-key` warning with its line and the nearest known key, and it is dropped before rendering, so it reaches neither the HTML nor a payload. A known key with a value of the wrong type, enum or range, and a missing required field, is a `block.schema` error: the fence stays visible as code and publishing refuses the report. A required `title` (chart, audio, video) must not be empty; an image's `alt` may be `""` to mark it decorative. The same walker checks the frontmatter, where an unknown key warns and stays in the metadata with no effect.

## Charts

```yaml chart
type: line
title: Churn turned in Q2
subtitle: The mature cohorts improved first.
data:
  inline:
    - {month: Jan, churn: 0.08, cohort: Existing}
    - {month: Feb, churn: 0.06, cohort: Existing}
encoding: {x: month, y: churn, color: cohort}
axes:
  x: {label: Month}
  y: {label: Churn, format: percent}
annotations:
  - {kind: line, at: {y: 0.07}, text: Target}
style: {ratio: '16:9', palette: scientific}
```

Charts require a supported `type`, a `title` that states the claim (a missing one is `block.schema` and the block stays visible as source), and `data` and `encoding` mappings. A channel the type does not read, or a required one that is missing, is `block.schema`, and a channel, `highlight.by`, annotation anchor or table column that names a column the data lacks is `block.field`. `subtitle` is optional. `stack: true` stacks the series of a bar or area chart that has `encoding.color`; `wide: true` takes the full report width; `digits` and `readout_digits` set the significant digits (2 to 6) of printed numbers and readouts.

<!-- registry:begin chart-types -->
| Type | Required channels | Optional channels | Draws |
| --- | --- | --- | --- |
| `line` | `x` and `y` | `color`, `facet` and `detail` | Values over a time axis, one line per series. |
| `bar` | `x` and `y` | `color`, `facet` and `detail` | Values per category, one bar per series. |
| `scatter` | `x` and `y` | `color`, `size`, `label`, `facet` and `detail` | Points on two numeric axes, optionally sized and labelled. |
| `area` | `x` and `y` | `color`, `facet` and `detail` | Values over a time axis, filled to the baseline. |
| `pie` | `x` and `y` | `facet` and `detail` | Shares of a total by category. |
| `heatmap` | `x`, `y` and `color` | `facet` and `detail` | A grid of cells coloured by a numeric value. |
| `histogram` | `x` | `color`, `facet` and `detail` | Counts of one numeric field in bins. |
| `boxplot` | `y` | `x`, `color`, `facet` and `detail` | The distribution of a numeric field per category. |
| `interval` | `x`, `lower` and `upper` | `y`, `color`, `facet` and `detail` | Authored lower and upper bounds per category, with an optional estimate. |
| `waterfall` | `x` and `y` | `measure`, `color`, `facet` and `detail` | Signed steps from a start to a total, each step drawn from the running level. |
| `treemap` | `label` and `y` | `parent`, `color`, `facet` and `detail` | A hierarchy of named parts whose tile areas are their values. |
| `funnel` | `x` and `y` | `detail` | Stages from first to last as centred bands as wide as their values. |
| `dumbbell` | `x`, `lower` and `upper` | `color`, `facet` and `detail` | A line per category between two values, with a hollow start and a filled end. |
| `band` | `x`, `lower` and `upper` | `y`, `color`, `facet` and `detail` | A filled range between a lower and an upper value over x, with an optional line for a central value. |
| `bullet` | `x` and `y` | `target`, `facet` and `detail` | A value bar against a target, with optional shaded ranges, per category. |
| `candlestick` | `x`, `open`, `high`, `low` and `close` | `facet` and `detail` | Open, high, low and close per period, drawn as a body with wicks. |
| `icicle` | `label` and `y` | `parent`, `color`, `facet` and `detail` | A hierarchy as stacked bands by depth; widths are shares of the whole. |
| `map` | `region` and `y` | `facet` and `detail` | Regions shaded by value. |
| `ohlc` | `x`, `open`, `high`, `low` and `close` | `facet` and `detail` | Open, high, low and close per period, drawn as ticks on a vertical bar. |
| `radar` | `x` and `y` | `color`, `facet` and `detail` | Spokes at equal angles with one polygon per group. |
| `sunburst` | `label` and `y` | `parent`, `color`, `facet` and `detail` | A hierarchy as rings by depth; angles are shares of the whole. |
| `pareto` | `x` and `y` | `facet` and `detail` | Bars sorted from the largest down with a running share of the total on a second axis. |
| `tree` | `label` | `parent`, `y`, `facet` and `detail` | A hierarchy as nodes joined by elbow curves, with an optional value per node. |
| `combo` | `x`, `y` and `line` | `facet` and `detail` | Bars for one measure with a line for another on a second axis. |
| `rose` | `x` and `y` | `color`, `facet` and `detail` | Bars around a circle, one per category, with radius as the value. |
| `waffle` | `x` and `y` | `color`, `facet` and `detail` | A grid of squares in which each category fills a share of the whole. |
| `gauge` | `y` | `x` and `facet` | One value on an arc, with optional bands that split the scale. |
| `calendar` | `x` and `y` | `facet` | A square for each day in week columns, coloured by value, one strip per year. |
| `streamgraph` | `x` and `y` | `color` and `facet` | Stacked areas around a moving centre line, one stream per group, thickness as the value. |
| `ribbon` | `x` and `y` | `color` and `facet` | Stacked bars ranked by value in each category, with ribbons joining each group across categories. |
| `sankey` | `source`, `target` and `y` | `facet` | Flows between named nodes as ribbons as wide as their values, in columns by longest path from a source. |
| `alluvial` | `dimensions` | `y` and `facet` | Rows as paths through several categorical fields, drawn as ribbons between the categories of neighbouring columns. |
| `chord` | `source`, `target` and `y` | `facet` | Nodes as arcs around a circle sized by their flow, with ribbons between them as wide as the flow they carry. |
| `network` | `source` and `target` | `y` and `facet` | Nodes joined by lines in a fixed force layout, with node size by number of links and an optional value that widens each line. |
| `lollipop` | `y` | `x`, `color`, `facet` and `detail` | A stem from zero with a dot at the value, per category. |
| `violin` | `x` and `y` | `color` and `facet` | The distribution of a numeric field per category as a mirrored density shape with quartiles and median. |
| `dotplot` | `x` | `color` and `facet` | Counts of one numeric field in equal-width intervals, one stacked dot for every observation. |
| `bin2d` | `x` and `y` | `facet` | Counts of observations in a grid of rectangles over two numeric fields, coloured by count. |
| `hexbin` | `x` and `y` | `facet` | Counts of observations in a tiling of hexagons over two numeric fields, coloured by count. |
| `density` | `x` | `color`, `facet` and `detail` | A smooth estimate of how values are spread, one filled curve per group. |
| `ecdf` | `x` | `color`, `facet` and `detail` | A step line of the share of values at or below each value, per group. |
| `qq` | `y` | `color`, `facet` and `detail` | Sample values against normal quantiles, with a quartile reference line. |
| `strip` | `y` | `x`, `color`, `facet` and `detail` | Every observation as a point, spread sideways so none hide, per category. |
| `splom` | `dimensions` | `label`, `color` and `facet` | Every pair of several numeric fields as a small scatterplot in a grid, with the field names on the diagonal. |
| `parallel` | `dimensions` | `label`, `color` and `facet` | A vertical axis for each of several numeric fields, scaled to its own range, with one line per row across them. |
| `gantt` | `label`, `start` and `end` | `color` and `facet` | One bar per task from its start to its end on a time axis, tasks in source order. |
| `contour` | `x`, `y` and `z` | `facet` | Iso-lines of a value over the plane at round levels, coloured along the style's ramp, with a key. |
| `packed-bubble` | `label` and `y` | `color` and `facet` | Circles packed together, each as large in area as its value, coloured by group. |
| `wordcloud` | `label` and `y` | `facet` | Words sized by their values, packed together without overlap. |
| `venn` | `sets` and `y` | `facet` | Two or three overlapping circles whose areas are the sets' sizes and whose overlaps are drawn at their own sizes. |
| `vector` | `x`, `y` and `angle` | `size` and `facet` | An arrow at each position pointing in the direction of its angle, optionally as long and as coloured as its value. |
| `flow` | `label` | `source`, `target`, `lane`, `y` and `facet` | Cards in ranks along the flow, written as `nodes` and `edges` or drawn from data, joined by right-angled lines that never cross a card. |
| `pivot` | `rows`, `columns` and `values` |  | A matrix of sums with row, column and grand totals, as in a spreadsheet pivot table. |
| `surface` | `x`, `y` and `z` |  | A mesh of z over the x-y plane seen from a set angle, coloured by z. |
| `scatter3d` | `x`, `y` and `z` | `color`, `size` and `label` | Points in a box seen from a set angle, nearer dots larger, coloured by group and sized by a value. |
| `bar3d` | `x` and `y` | `color` | Bars as boxes on a grid of two categories, seen from a set angle, with the height as the value. |
<!-- registry:end -->

<!-- registry:begin fields/chart -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `type` | chart type | yes |  | The chart type. |
| `title` | text | yes |  | The title that states the claim; it is the caption heading and the chart's accessible name. |
| `subtitle` | text |  |  | A line under the title. |
| `stack` | `true` or `false` |  | `false` | Stacks the series of a bar or area chart; needs `encoding.color`. |
| `wide` | `true` or `false` |  | `false` | Draws the chart at the full report width. |
| `auto_annotations` | `true` or `false` |  | `true` | Prints the values and point names a chart adds by itself, such as bar and line values, heatmap cell values, pie shares and a scatter's point names; `false` prints none of them and leaves the authored `annotations`, the `highlight` names and the series and category names. |
| `digits` | whole number of significant digits from 2 to 6 |  |  | Significant digits of the chart's printed numbers, not decimal places. Omit to use the default. |
| `readout_digits` | whole number of significant digits from 2 to 6 |  |  | Significant digits of the hover readout, not decimal places; never fewer than `digits`. Omit to use the chart precision. |
| `highlight` | mapping |  |  | Names the scatter points that always get a label. |
| `highlight.by` | column name | yes |  | The column whose values `include` matches. |
| `highlight.include` | list of text, number or `true`/`false` | yes |  | The values to label; at least one. |
| `highlight.priority` | list of text, number or `true`/`false` |  |  | Values that win a label slot first. |
| `data` | data source | yes |  | The chart's data; exactly one local source is allowed. |
| `data.inline` | list of mapping |  |  | Rows written in the block, each a mapping of column to value. |
| `data.csv` | report-folder file |  |  | A CSV file inside the report folder. |
| `data.json` | report-folder file |  |  | A JSON file of row mappings inside the report folder. |
| `data.url` | `http` or `https` address |  |  | An https data source; current native and browser hosts do not load this field. |
| `data.sql` | mapping |  |  | A SQL query run through a named connector on a host that has a registry. |
| `data.sql.connector` | text | yes |  | The name of the connector that runs the query. |
| `data.sql.query` | report-folder file |  |  | A report-relative file holding the SQL; set this or inline, not both. |
| `data.sql.inline` | text |  |  | The SQL text itself; set this or query, not both. |
| `data.metricflow` | mapping |  |  | A MetricFlow query run through a named connector. |
| `data.metricflow.connector` | text | yes |  | The name of the connector that runs the query. |
| `data.metricflow.metrics` | list of text | yes |  | The metric names to query; at least one. |
| `data.cache` | mapping |  |  | A reference to a cached connector result. |
| `data.cache.key` | text | yes |  | The key of the cached result. |
| `data.sql.params` | mapping |  |  | Named values passed to the query. |
| `data.sql.cache` | mapping |  |  | How long the query's result is reused. |
| `data.sql.cache.key` | text |  |  | The key of the cached result. |
| `data.sql.cache.ttl` | text |  |  | How long the result stays fresh, such as 1h. |
| `data.sql.cache.freshness` | text |  |  | A freshness rule for the result. |
| `data.sql.cache.refresh` | `true` or `false` |  |  | Whether every run refreshes the result. |
| `data.metricflow.group_by` | list of text |  |  | The dimensions to group by. |
| `data.metricflow.where` | list of text |  |  | Filters on the query. |
| `data.metricflow.order_by` | list of text |  |  | The order of the rows. |
| `data.metricflow.limit` | whole number |  |  | The most rows returned. |
| `data.metricflow.cache` | mapping |  |  | How long the query's result is reused. |
| `data.metricflow.cache.key` | text |  |  | The key of the cached result. |
| `data.metricflow.cache.ttl` | text |  |  | How long the result stays fresh, such as 15m. |
| `data.metricflow.cache.freshness` | text |  |  | A freshness rule for the result. |
| `data.metricflow.cache.refresh` | `true` or `false` |  |  | Whether every run refreshes the result. |
| `data.cache.ttl` | text |  |  | How long the result stays fresh. |
| `encoding` | mapping | yes |  | Maps data columns to chart channels. |
| `encoding.x` | column name |  |  | The column on the x axis or the categories. |
| `encoding.y` | column name |  |  | The column that gives the values. |
| `encoding.lower` | column name |  |  | The column with each interval's lower bound; interval charts only. |
| `encoding.upper` | column name |  |  | The column with each interval's upper bound; interval charts only. |
| `encoding.color` | column name |  |  | The column whose distinct values become series. |
| `encoding.size` | column name |  |  | The column that sizes bubbles on a scatter chart, or gives the length and colour of the arrows of a vector chart. |
| `encoding.label` | column name |  |  | The column whose values label scatter points. |
| `encoding.facet` | column name |  |  | The column whose distinct values split the chart into panels. |
| `encoding.target` | column name |  |  | Bullet: the column that gives each target. Sankey, chord, network and flow: the column with the node each link ends at. |
| `encoding.dimensions` | list of column name |  |  | Alluvial: the columns of categories, in order from left to right. Parallel coordinates and scatterplot matrix: the numeric columns drawn as axes. At least two. |
| `encoding.source` | column name |  |  | Sankey, chord, network and flow: the column with the node each link starts at. |
| `encoding.open` | column name |  |  | Candlestick and OHLC: the opening value. |
| `encoding.high` | column name |  |  | Candlestick and OHLC: the highest value. |
| `encoding.low` | column name |  |  | Candlestick and OHLC: the lowest value. |
| `encoding.close` | column name |  |  | Candlestick and OHLC: the closing value. |
| `encoding.region` | column name |  |  | Map only: the column naming each region. |
| `encoding.line` | column name |  |  | Combo only: the column drawn as a line on the second axis. |
| `encoding.start` | column name |  |  | Gantt only: the column holding each task's start, as a date or a number. |
| `encoding.end` | column name |  |  | Gantt only: the column holding each task's end, as a date or a number of the same kind as the start. |
| `encoding.rows` | column name |  |  | Pivot only: the column whose values head the rows of the matrix. |
| `encoding.columns` | column name |  |  | Pivot only: the column whose values head the columns of the matrix. |
| `encoding.values` | column name |  |  | Pivot only: the numeric column summed into each cell. |
| `encoding.detail` | list of column name |  |  | Extra columns shown in readouts, Table views and downloads. |
| `encoding.measure` | column name |  |  | The column that marks each waterfall row as `delta` (a step), `start` or `total`. |
| `encoding.z` | column name |  |  | Contour, surface and 3-D scatter: the numeric column drawn as iso-lines or as the third coordinate. |
| `encoding.sets` | column name |  |  | Venn only: the column naming the sets of each row, joined by `&` or a comma, such as `A&B`. |
| `encoding.angle` | column name |  |  | Vector only: the column holding each arrow's direction in degrees, counter-clockwise from east. |
| `encoding.lane` | column name |  |  | Flow only: the column naming the lane, drawn as a band, that each step sits in. |
| `encoding.parent` | column name |  |  | The column that names each row's parent tile in a treemap. |
| `controls` | list of mapping |  |  | Reader controls drawn above the chart, in author order. |
| `controls[n].kind` | one of `playback`, `filter`, `metric` or `variant` | yes |  | What the control changes. |
| `controls[n].id` | text |  |  | The control's name, unique in the chart. |
| `controls[n].field` | column name |  |  | The data column a playback or filter control steps through. |
| `controls[n].label` | text |  |  | The control's caption. |
| `controls[n].options` | any value |  |  | The choices: values, or mappings with value, label and the field, axis or chart each applies. |
| `controls[n].initial` | text, number or `true`/`false` |  |  | The choice shown first. |
| `controls[n].all` | text |  |  | The label of a filter's choice that shows every row. |
| `controls[n].speed` | number |  | `800` | Milliseconds per frame of a playback control. |
| `controls[n].loop` | `true` or `false` |  | `true` | Whether playback starts over; false plays once. |
| `controls[n].as` | one of `buttons` or `dropdown` |  |  | How the choices are drawn. |
| `binning` | mapping |  |  | Histogram bin options; other chart types refuse it. |
| `binning.count` | whole number |  | `Sturges' rule` | The number of equal-width bins, 1 to 256. |
| `binning.edges` | list of number |  |  | Explicit bin edges, 2 to 257 increasing values; not combined with count or extent. |
| `binning.extent` | list of number |  |  | The lower and upper bound of the bins, two increasing values. |
| `binning.measure` | one of `count`, `proportion` or `density` |  | `count` | What bin heights show. |
| `axes` | mapping |  |  | Axis options. |
| `axes.x` | mapping |  |  | Options of the x axis. |
| `axes.x.label` | text |  |  | The x axis title. |
| `axes.x.format` | one of `number`, `number_compact`, `percent` or `year` |  |  | The number format of x values. |
| `axes.x.unit` | text |  |  | A unit printed with x values, by the unit rule. |
| `axes.x.digits` | whole number of significant digits from 2 to 6 |  |  | Significant digits of x values on this axis, from 2 to 6. |
| `axes.x.locale` | one of `en-US`, `en-GB`, `cs-CZ`, `de-DE` or `ja-JP` |  |  | The locale of x values and calendar labels; the report's lang otherwise. |
| `axes.y` | mapping |  |  | Options of the value axis. |
| `axes.y.label` | text |  |  | The value axis title. |
| `axes.y.format` | one of `number`, `number_compact`, `percent` or `year` |  |  | The number format of values. |
| `axes.y.unit` | text |  |  | A unit printed with values, by the unit rule. |
| `axes.y.digits` | whole number of significant digits from 2 to 6 |  |  | Significant digits of values on this axis, from 2 to 6. |
| `axes.y.min` | number |  |  | The lowest value the axis shows; below axes.y.max. |
| `axes.y.max` | number |  |  | The highest value the axis shows. |
| `axes.y2` | mapping |  |  | Combo charts: options of the second value axis, at the right. |
| `axes.y2.label` | text |  |  | The second axis title. |
| `axes.y2.format` | one of `number`, `number_compact`, `percent` or `year` |  |  | The number format of the second axis. |
| `axes.y2.unit` | text |  |  | A unit printed with the second axis's values. |
| `axes.y2.digits` | whole number |  |  | Significant digits of values on the second axis. |
| `axes.y2.min` | number |  |  | The lowest value the second axis shows. |
| `axes.y2.max` | number |  |  | The highest value the second axis shows. |
| `axes.color` | mapping |  |  | Options of the colour scale or key. |
| `axes.color.label` | text |  |  | The colour key title. |
| `axes.color.format` | one of `number`, `number_compact`, `percent` or `year` |  |  | The number format of colour values. |
| `axes.color.unit` | text |  |  | A unit printed with colour values, by the unit rule. |
| `axes.color.digits` | whole number of significant digits from 2 to 6 |  |  | Significant digits of colour values on this axis, from 2 to 6. |
| `annotations` | list of mapping |  |  | Marks added on top of the data. |
| `annotations[n].kind` | one of `point`, `range`, `line`, `band` or `text` | yes |  | What the annotation draws. |
| `annotations[n].at` | mapping |  |  | The coordinate of a point, line or text annotation; required for those kinds. |
| `annotations[n].from` | mapping |  |  | The start coordinate of a range or band; required for those kinds. |
| `annotations[n].to` | mapping |  |  | The end coordinate of a range or band; required for those kinds. |
| `annotations[n].text` | text |  |  | The annotation's text; required for point and text annotations. |
| `annotations[n].rotate` | number |  |  | The rotation of a text annotation. |
| `direction` | one of `down` or `right` |  | `down` | Flow only: whether the flow runs down the page or across it; across turns down where it does not fit. |
| `nodes` | list of mapping |  |  | Flow only: the cards, in reading order; with nodes the chart takes no `data` or `encoding`. |
| `nodes[n].title` | text | yes |  | The card's title: a verb and an object for a step, a short question for a decision. |
| `nodes[n].id` | text |  |  | The name edges and groups use for the card; the title in lower case with hyphens when omitted. |
| `nodes[n].lines` | text or list of text |  |  | The two or three facts under the title, one per line. |
| `nodes[n].kind` | one of `step`, `decision`, `start`, `end`, `store` or `object` |  | `step` | What the card stands for, which sets its outline: a rounded card, a diamond, a pill for a start or an end, a cylinder for stored data, a sheet with a folded corner for a thing made. |
| `nodes[n].tone` | one of `alert` or `muted` |  |  | `alert` for what needs the reader's eye, such as a failure; `muted` for what sits outside the subject. |
| `edges` | list of text or mapping |  |  | Flow only: one line each, `from -> to`, dashed `from ..> to`, with `: label` to name it; ends are ids or titles. |
| `edges[n].edge` | text | yes |  | The edge as `from -> to: label`, when it also takes a tone. |
| `edges[n].tone` | one of `alert` or `muted` |  |  | `alert` marks the path the reader should follow, such as a failure. |
| `groups` | list of mapping |  |  | Flow only: cards that belong together, drawn in one box with its label in the border. |
| `groups[n].label` | text | yes |  | What the group is, set into its top border. |
| `groups[n].nodes` | list of text | yes |  | The ids or titles of the group's cards; a card sits in one group. |
| `groups[n].tone` | one of `alert` or `muted` |  |  | The tone of every card in the group. |
| `style` | mapping |  |  | Chart styling. |
| `style.palette` | one of `air`, `brutalist`, `hand-drawn`, `scientific` or `sleek` |  |  | The colour palette of the marks. |
| `style.ratio` | positive number or `W:H` |  |  | The width to height ratio of the chart. |
| `style.orientation` | one of `auto`, `vertical` or `horizontal` |  |  | Whether bars and boxes stand or lie. |
| `style.curve` | one of `linear`, `monotone`, `step`, `step_before` or `step_after` |  |  | How line and area marks connect points; the default is `monotone`. |
| `style.curvature` | number |  |  | How strongly curves bend, from 0 to 1. |
| `style.points` | one of `all`, `m4` or `band` |  |  | Explicit `all` draws every observation without a ceiling; `m4` keeps per-pixel extrema and `band` dense means/ranges for line and area. When omitted, measured ceilings trigger disclosed representatives. Scatter accepts explicit `all` only. |
| `style.facet_scale` | one of `shared` or `free` |  |  | Whether facet panels share one value scale (`shared`, the default) or each draws its own (`free`). |
| `style.gaps` | one of `break` or `bridge` |  |  | Combo charts: whether a missing line value breaks the line (the default) or is bridged. |
| `style.ranges` | list of number |  |  | Bullet charts: up to three ascending thresholds that shade the bands between them. |
| `style.bands` | list of mapping |  |  | Gauge charts: a list of `{to, label, color}`, each band running from the end of the one before. |
| `style.min` | number |  |  | Gauge charts: the value at the start of the arc; `axes.y.min` also sets it. |
| `style.max` | number |  |  | Gauge charts: the value at the end of the arc. |
| `style.cells` | number |  |  | Waffle charts: 25, 50 or 100 cells; other numbers draw 100 with a warning. |
| `style.inner` | number |  |  | Pie charts: the share of the radius left open for a ring, from 0 to 0.9. |
| `style.azimuth` | number |  |  | Surface, 3-D scatter and 3-D bar: the degrees the view turns round the vertical axis; the default is 35. |
| `style.elevation` | number |  |  | Surface, 3-D scatter and 3-D bar: the degrees the view looks down from, from 5 to 85; the default is 28. |
| `style.shade` | `true` or `false` |  |  | Pivot only: tint each cell along the style's ramp by its value. |
| `style.bandwidth` | number |  |  | The kernel bandwidth of a density chart, in the units of x; Silverman's rule when absent. |
<!-- registry:end -->

Field notes:

- `encoding.x`, `encoding.y`: Source field names for the primary axes.
- `encoding.lower`, `encoding.upper`: Required authored numeric bounds for interval charts. Their optional `encoding.y` supplies an estimate.
- `encoding.color`: Series/group channel; heatmaps use it for cell value.
- `encoding.size`: Bubble-size field for scatter.
- `encoding.label`: Optional scatter point-name field. Authored annotations take priority, then explicit `highlight` names, then sparse points; dense names are selected only when collision-free. Every observation remains in readouts and tables.
- `highlight`: Top-level, scatter only: `{by: <field>, include: [values], priority: [values]}`. Rows whose `by` value matches an `include` entry are labelled first and always attempted; see the label rules below. `by` must name a column (checked against inline rows), `include` is a non-empty array of strings or numbers, and `priority` is an optional array that orders entries already in `include`. Other keys, a non-mapping value or a non-scatter chart are schema errors.
- `encoding.facet`: Source field defining separate panels.
- `encoding.detail`: Extra columns listed for readouts, Table views and downloads, on every type.
- `axes.x`, `axes.y`: Optional `label`, `format` and `unit` strings and `digits`; `suffix` is retired for `unit`. `axes.x.locale` selects UTC calendar labels: `en-US` (default), `en-GB`, `cs-CZ`, `de-DE`, `ja-JP`. Day/week ticks use one compact date entry. `axes.y.min` and `axes.y.max` are optional numbers that fix the ends of the value axis (`min` below `max`).
- `axes.color`: Heatmap cell value formatting: optional `format`, `unit` and `digits`; `label` names the cell value in readouts. Heatmaps have no color bar.
- `style.palette`: `sleek` (stepped for its dark surface), `air` (for light surfaces), `scientific`, `brutalist` or `hand-drawn`. An omitted palette inherits the report treatment.
- `style.ratio`: Positive width/height number or `W:H` string. No author pixel height.
- `style.orientation`: Bars, box plots and intervals: `auto` (default), `vertical` or `horizontal`. Histograms accept `auto` or `vertical`.
- `style.curve`: Line/area interpolation: `monotone` (default), `linear`, `step`, `step_before` or `step_after`; `style.line.smoothing` is retired (`straight` is `linear`). Monotone cubic interpolation preserves each adjacent interval's direction and range. Steps change value halfway between observations, at the preceding coordinate, or at the next coordinate, respectively.
- `style.curvature`: Finite number from 0 to 1; default 1 for monotone. Zero is straight; larger values increase the handle length without overshoot. Straight and step curves ignore this setting.

Scatter point names follow one order. Authored annotations are placed first. Explicit highlights come next and are always attempted: matching is case-insensitive, the `priority` entries go first in their listed order (so the earlier entry wins a contested position), then the remaining `include` entries in listed order. A repeated value is placed once. A highlight sits beside its point when it can; only a highlight that cannot sit next to its point is joined to it by a leader line. When no position clears every mark, a highlight may sit over other marks on its halo but never over another label; one that cannot be placed even then is reported in `droppedLabels`, as is an `include` value that matches no plotted point (`highlight matched no plotted point`). Ordinary `encoding.label` names are sparse by default: points are ranked by isolation (fewest neighbours within 24 px first, then source order), at most 16 are attempted, and a name that collides with a placed label or would need a leader line is hidden, never drawn with a leader. Each hidden name is listed in `droppedLabels` with its reason (`would require a leader; drop by default`, `no collision-free position` or `label density limit`). A highlighted point is never labelled a second time by the ordinary pass, and highlights do not count toward its 16-name cap. Marks, readouts, Table and CSV values are unchanged by labelling. [The scatter-highlights example](../examples/scatter-highlights/report.rmd) shows a dense chart with sparse defaults and a highlighted one.

Ordinary bars use a flat baseline and optional rounding at the value end, including negative and horizontal bars. The theme's `chart.bar-radius` supplies `--rv-chart-bar-radius`; an absent or zero token gives square corners. The radius is capped at 15% of bar thickness and the painted length. Bars leave one zero-line stroke-width of clearance (1.5 SVG units); very small nonzero values retain one pixel of ink beyond that gap, while true zeros remain empty. Source targets and readouts retain their original values. Histogram bins keep square, area-faithful rectangles.

Chart titles stay on the caption's left edge, including when initial observations are zero or missing. The first x label uses the renderer's left paint edge as its optical anchor, including visible marker outlines and exact clipping boundaries. Native HTML uses bundled font-outline estimates; settled browser charts measure the loaded first glyph's ink separately and retain that correction in PNG downloads. Tick values, tick marks, later labels and data coordinates stay fixed. A first label keeps its original position if alignment would overlap its neighbor. Numeric/date labels and line/point category labels also stay at their original position if shifting would detach the label from its own scale coordinate, as with a leading data gap; categorical bar labels can align to their band's leading edge but never move toward a later nonzero bar. Empty charts keep the normal caption inset.

Default ratios are 16:9 for line/area/bar/heatmap/histogram/boxplot/interval, 4:3 for scatter and 1:1 for pie. SVG scales to the report column. Axes support the value formats `number`, `number_compact`, `percent` and `year`; a `unit` appends an authored unit, and a currency is a `unit` such as `$`. The `year` format prints the exact integer (`1700`, never `1.70k` or `1,700`) on ticks, readout headings and direct-label period text (a fractional value under an explicit `year` format keeps its exact source digits in readouts), and forces whole-year tick steps of at least 1 (no 2.5 step), so labels neither repeat nor sit between years. A numeric x axis uses whole-year ticks by default when every plotted x value is an integer from 1000 to 2999. An explicit non-year `axes.x.format`, an out-of-range value or a fractional value keeps numeric ticks; plotted values never change.

Histograms use `encoding.x` as the numeric observation field and calculate the vertical measure. Each accepted row contributes one observation. Optional `encoding.color` stacks groups within the same interval; `encoding.facet` splits panels with shared edges. `encoding.y`, `size` and `label` are unsupported for this type. Histograms stay vertical and their measure axis starts at zero; an authored positive `axes.y.max` may clip heights, with a diagnostic and unchanged exact readouts.

| Histogram `binning` option | Contract |
| --- | --- |
| `count` | Integer from 1 to 256. Equal-width intervals span the finite observed range or authored `extent`. Without a count, use `ceil(log2(n)) + 1` (Sturges), capped at 256; a constant sample defaults to one finite interval. |
| `extent` | Two increasing finite values. Observations outside the interval are excluded and counted. |
| `edges` | Two to 257 finite, strictly increasing boundaries. Cannot be combined with `count` or `extent`. |
| `measure` | `count` (default), `proportion` (count divided by accepted visible observations in the panel), or `density` (proportion divided by interval width). |

Intervals include their lower boundary and exclude their upper boundary, except that the last upper boundary is included. These boundary and Sturges conventions follow the [D3 binning reference](https://d3js.org/d3-array/bin); RichView uses exact equal-width edges rather than D3's optional domain nicening. Edges that collapse to the same floating-point value are deduplicated. Automatic edges are resolved from the full eligible source before faceting or visibility changes; hiding a group changes its contribution and the visible normalization denominator, not the bins.

Unequal widths are supported. Counts and proportions describe interval mass; use density when rectangle area should represent probability. Density integrates to one across all displayed intervals and groups in a panel with accepted data. Empty intervals remain inspectable; with no accepted observations, proportion and density read `Not defined`. A density outside the finite numeric range is an error with a units/count alternative. Missing/nonnumeric observations, missing group names and out-of-extent values are counted separately in diagnostics and disclosed below the chart. Bin readouts identify calculated summaries; Table and CSV retain every original source row.

Histogram annotations use the source X field and the derived `count`, `proportion` or `density` coordinate, matching the selected measure. For example, `at: {value: 10, count: 4}` anchors a count annotation without inventing a source Y column. Measure readouts honor `axes.y.format` and `suffix`; when showing proportion or density, the separate record count and denominator remain unformatted counts. [The histogram example](../examples/histogram/report.rmd) demonstrates counts, unequal-width density and faceting.

Box plots use `type: boxplot` with numeric `encoding.y`, optional categorical `encoding.x`, and optional color/facet fields. With no X field, one category contains all accepted observations. Each observed category/color pair gets a separate box; empty combinations are not invented. `style.orientation` accepts `auto`, `vertical` and `horizontal`. Auto uses the existing category-label fitting rules; explicit horizontal layouts grow with the rows. `axes.x` always describes categories and `axes.y` always describes values, including value limits, formats and suffixes, regardless of drawing orientation. Size and point-label channels are unsupported and diagnosed.

Quartiles use [R-7 linear interpolation](https://d3js.org/d3-array/summarize#quantile), with positions `(n - 1) * p` in the sorted finite sample. The box spans Q1 to Q3 and its rule marks the median. Whiskers end at the most extreme **observed** values within inclusive 1.5-IQR fences; they do not end at the theoretical fences. Observations outside those fences are drawn individually and remain inspectable as potential outliers. This conventional rule does not establish that a record is an error, and the box is not a confidence interval. Singleton and repeated-value samples retain their actual zero-width numeric interval. Finite extreme and subnormal samples use overflow-safe interpolation and fence comparisons.

Box-plot readouts label derived summaries separately from original outlier records. Long readouts scroll with the pointer or with Page Up/Page Down while the chart has keyboard focus; scrolling the readout preserves the selected observation and chart window. Missing values, nonnumeric values, missing categories and missing color groups are counted separately, with each excluded row counted once. Table and CSV retain all original rows and text. Value-axis bounds can clip visible marks; diagnostics disclose that clipping, while summaries and original source downloads keep the full accepted data. Facet panels and visible color groups are summarized independently, with stable parent colors. Annotations retain category-X/value-Y anchors in both orientations. See the [box-plot example](../examples/boxplot/report.rmd) for grouped, horizontal and faceted distributions.

Interval charts use `type: interval`, categorical `encoding.x`, numeric `encoding.lower` and `encoding.upper`, and optional `encoding.y` for an authored estimate. Optional color/facet fields keep group identity across sparse panels and visibility changes. Each accepted category/color pair supplies exactly one interval per panel; duplicates are errors. Bounds describe only the author's supplied meaning. The renderer does not calculate confidence, standard error or a distribution. A missing estimate draws the range without an estimate marker; an estimate outside its own range is allowed and contributes to the automatic value domain. See the [interval example](../examples/interval/report.rmd).

Lower bounds must not exceed upper bounds, including distinct decimal strings that round to the same binary64 value. True zero-width intervals remain valid. Positive intervals below that numeric precision are disclosed while readouts, Table and CSV preserve their distinct source bounds. Missing bounds, categories and color groups are counted as exclusions; original rows remain accessible. Malformed supplied numbers, nonzero underflow and missing facet names produce actionable source errors. Every facet row must name a panel so its original data can remain in that panel's Table/CSV. Source errors identify the original row before faceting.

Interval orientation and axis metadata follow the category-X/value-Y convention used by box plots. Readouts label lower and upper bounds separately, preserve available decimal digits and value-axis units, and show an estimate only when supplied. A geometric hit target never becomes an invented observation. Authored value limits clip ranges and estimates; endpoint caps appear only at visible authored endpoints, and clipping is disclosed. Source readouts and downloads keep the full values. Size, point-label channels and binning are unsupported. This categorical interval chart does not add continuous forecast bands or temporal transforms.

Use `style: {curve: step_after}` for values held until the next observation, such as a posted price or policy rate. Step curves work on numeric, categorical and calendar axes, including areas and facets. Missing values and calendar gaps still break the path; corner vertices never become observations in tooltips, tables or CSV. Stacked areas retain straight layer boundaries.

For a line that connects every observation without smoothing, write `style: {points: all, curve: linear}`. Point retention and interpolation are separate choices. With the field omitted, the default retains every valid observation up to the chart type's declared automatic ceiling; above it, line and area use M4, while scatter keeps one representative per occupied CSS-pixel cell per series plus the x, y and size extremes. Each automatic policy always discloses its density and kept/total counts, even after a narrow zoom. Explicit `points: all` bypasses the ceiling and draws every point; the author or embedding host accepts the cost. Reduced paths keep every source row in readouts, Table and CSV at the author's precision, and each actual reduction emits an informational check with the density and kept/total counts. The dense-band strategy uses per-pixel means and ranges above six observations per plot pixel for unstacked series, and per-pixel extrema otherwise; stacked areas retain extrema rather than drawing mean/range bands.

Numeric X zoom accepts the same finite numbers and numeric strings as chart geometry. Scatter skips unusable X coordinates; numeric line and area require numeric X on every visible source row. A line or area whose X values are all calendar strings, including four-digit years, retains calendar behavior. A numeric zoom keeps its numeric scale when only year strings remain in the selected window. Hiding a nominal series can make a line or area numeric; restoring it ends numeric zoom and shows all visible categories. Distinct finite endpoints remain zoomable even if their difference exceeds the floating-point range. At the smallest representable scale, a window retains two distinct endpoints within the source extent instead of collapsing to one value. Tables and CSV keep the original values and text; selected readouts use the original source rows.

Numeric line and area zoom selects an X viewport through their existing paths. Crossing segments remain visible between observations, while missing values still break the path. Automatic Y limits use all currently visible source rows; X zoom and pan do not rescale Y, and authored Y limits still apply. Only original observations inside the inclusive X window are available to readouts and keyboard traversal, including missing-value rows; outside path context never creates an interpolated observation. Scatter keeps point selection within the window.

Bar and scatter facets retain the parent chart's category colors through interaction and PNG downloads. Nonempty categories reserve their existing first-seen palette slots before value eligibility is considered; unavailable categories can reserve a slot without appearing in another panel. Monochrome bars retain the same parent hatch identity. Each panel keeps its own grouping positions, source rows, Table and CSV. Rendering a changed source derives the category slots again from that source's order.

When explicit chart decals are enabled, bar legend swatches show the same hatch as their bars in native reports, live views and PNG downloads. The existing six hatch shapes repeat in original category order; legend names, dimensions and controls stay unchanged.

Pies retain labels around the disk when the complete names and percentages fit. Narrow or crowded pies use a wrapped category key and grow beyond the preferred ratio to keep both the disk and its labels readable; categories are never combined or abbreviated. Hovering or tapping a key entry inspects the same aggregated value as its slice, including tiny and zero shares. A positive share that rounds to zero is labeled `<0.1%`; a partial share that rounds to a whole is labeled `>99.9%`. Genuine zero and full shares retain `0.0%` and `100.0%`.


When a style explicitly enables `chart.decals`, pie sectors and their category-key swatches use the existing six hatch patterns. Patterns follow the current pie category order and repeat after six categories; they are an additional cue, not unique category identifiers. Tiny sectors may show only part of a pattern. Values, proportions, labels and category order are unchanged; grayscale colors alone do not enable decals.

Generic axis and heatmap value labels using `format: percent` print one percent sign when `unit` is also `%` (including surrounding whitespace). Suffix-only percentage labels remain literal source units, and other authored suffixes retain their existing text. This label rule does not change exact source readout formatting or normalize arbitrary suffix whitespace.

Category names are never cut with an ellipsis and never silently dropped. At each width a category axis uses the first layout that fits: every name on one line; every name in a balanced wrap of at most two lines that breaks only at spaces or after a hyphen; for ordered categories (years, quarters such as `2025 Q3`, months such as `2025-08` or `Aug 2025`, ISO days and numbered names such as `Day 3`), labels on every k-th period boundary (every 2, 3, 6 or 12 months, 2 or 4 quarters, 2, 5 or 10 years); and for nominal bars with `style.orientation: auto`, horizontal bars. Horizontal bars give every row at least 1.6 lines and grow the chart; their names sit in a gutter of at most 40% of the width and two lines, or on a line of their own above each bar when the chart is narrower than 480 px. The value axis then runs along the bottom with the `axes.y.label` under it, and `axes.x.label` heads the names. Calendar labels stay on one row and carry their year (`2025-08` as `Aug 25`, `2025 Q3` as `Q3 25` within one century; full years across centuries); tables, readouts and CSV keep the source text. A name that still has no room, which only happens on axes that cannot turn (`orientation: vertical`, nominal line or heatmap columns), is reported as a `chart.label-dropped` warning; thinned period labels are disclosed as `chart.label-thinned` information.

Value axes end on labelled ticks. One value axis uses a shared magnitude, units and tick precision across its entire span; zero has no decimal padding. Direct line labels use a second value line on phones when it fits, and fall back to a legend when complete identities cannot fit. A direct label is the series name in its series colour with no dot or other mark before it; the legend fallback keeps its short line swatch (square for area and bar) as the cue that tells lines apart. A chart whose colour field has a single group draws neither a direct label nor a legend; its title carries the identity. Bars and areas keep zero, and so does any axis in percent (the `percent` format, a `%` suffix, or a label whose unit is `%`) when its data are all on one side of zero. A line whose values all lie on one side of zero gets up to a third of its range of room towards zero, and the axis reaches zero when that room would get there; it never crosses zero unless the data do. `axes.y.min` and `axes.y.max` override this. A parsed zero bar has no filled length and retains its source inspection target; a group without a value for a category is marked `n/a` at the baseline. When `encoding.color` names the same field as `encoding.x`, each category uses one bar slot with its existing category color; impossible combinations of two different values in that same field do not reserve slots or receive `n/a` marks. Hiding a category keeps the remaining category colors stable. Distinct X and color fields retain grouped slots even when their current values happen to match.

Each visible bar category/series slot accepts one finite numeric observation, including numeric strings. Repeated observations reject that chart panel with `chart.ambiguous-bar-slot`; this includes equal values, zero and opposite signs. The diagnostic identifies each repeated category, optional series and count. Aggregate or resolve the source explicitly before drawing bars. Separate series and facets remain independent; missing or nonnumeric values do not count as a second numeric observation. Table, CSV and source inspection retain the rows. This bar rule does not change pie aggregation or repeated observations in line and area charts.

Missing observations stay visible. Lines end with butt caps, so a break is as wide as the missing run, and lines and area fills break at the same observations. A break narrower than 3 px on screen, which a line cannot show, is marked by a 1 px rug mark (`rvc-gap`) standing on the plot's bottom edge; marks closer than 2 px merge into one short bar. The native gap diagnostic counts those narrow breaks at the rendered width.

Heatmaps show an authored missing color value as a neutral outlined cell with `n/a` where the cell has room for readable text. Null, an absent color field, blank text and the case-insensitive markers `.`, `na`, `n/a`, `nan`, `null` and `none` mean no observation; an absent row/column combination stays empty. The cell title and readout retain source identity, while Table and CSV retain the original values. Arbitrary nonnumeric text is an error and is not painted. A missing row that repeats an existing coordinate is diagnosed and is not drawn over another observation; resolve those source rows explicitly. Missing cells do not enter the numeric color domain. A heatmap with no numeric observations retains its source error and has no numeric key.

Heatmap cell labels use three-significant-digit scientific notation when their compact text would round a nonzero value to `0`. Actual zero and existing nonzero compact strings keep their formatting. Percentage conversion and literal suffixes apply as before. A longer label appears only when it fits the existing cell and clears authored annotation text or strokes; cell geometry, color, exact readouts, Table and CSV remain unchanged.

Two or more numeric observations at the same heatmap coordinate reject that panel with an explanation and source diagnostics; row order never chooses the visible value. Aggregate or resolve those source rows explicitly. Diagnostics identify the original one-based rows of the affected chart's resolved data, including each facet's own Table. All source rows remain available in Table and CSV, and valid neighboring facets still draw.

Only finite heatmap values with nonempty X and Y coordinates enter the color domain. A numeric observation with an absent, null or empty coordinate reports an error and stays available in source interfaces; its invisible value does not alter visible cell colors. The diagnostic names the encoded coordinate fields and counts affected observations, with separate X/Y counts that can overlap. Numeric zero, boolean false, significant whitespace and literal category names remain valid coordinates. If no numeric values can be placed, there is no numeric key; located missing cells keep their outlines, and a chart with no placeable cells explains the coordinate problem. Existing source-derived row and column names remain unchanged.

Line and area charts whose X values all select calendar handling retain calendar validation. Invalid data, including repeated timestamps within one series, displays `Cannot draw this time series` with its original diagnostics instead of falling back to categorical drawing. Source Table and CSV values remain available; valid neighboring facets still draw. A facet field with no actual groups does not disable calendar handling. Live generic chart measurements retain source errors alongside browser glyph checks, and valid updates clear previous errors.

Series controls use the same category names as their marks, including numeric and boolean categories; numeric `1` and string `"1"` share one group without rewriting source rows. Multi-series generic plots retain their controls when all groups are hidden, so every group can be restored. A sole group has no legend control and stays visible. Updating a generic chart retains hidden groups still present in the new source and restores all current groups if none would otherwise remain visible.

Heatmap rows are at least 1.6 lines tall and the chart grows with them. Row names wrap to two lines in a gutter of at most 35% of the width; column names wrap to two lines, and the grid transposes when that lets every column be named. Cells are coloured by magnitude on the theme's three-stop `chart.ramp`, mixed in OKLCH: zero is the ramp's low stop and the rounded maximum magnitude its high stop. Negative values take a second arm with the same lightness steps and the hue turned by 150° (blue ramps meet orange), so salience follows magnitude and hue shows the sign. The color domain includes zero and the data and rounds outwards to a shared maximum magnitude; no color bar is drawn. Cells at least 40 × 20 px print their value. Values at the author's chosen precision remain available in readouts and tables. Where CSS `contrast-color()` is supported, numeric cell text chooses black or white from that cell’s color expression, including live ramp-variable changes. Other viewers retain the existing theme ink/surface fallback. The rule travels inside standalone SVGs; authored `!important` text fills still take precedence. This does not change cell colors, value formatting or label fit, and does not promise a contrast threshold for arbitrary overrides or viewers.

Each builtin theme has a categorical palette, authored as `oklch()` inside sRGB and validated on that theme's chart surface: the first four colours reach 3:1 against it, and neighbouring colours differ by an OKLab ΔE×100 of at least 15 for normal vision and at least 8 under protan, deutan and tritan simulation (Machado, Oliveira and Fernandes, 2009). A style's `chart.palette` (a builtin palette name, or one to eight colours) and `chart.ramp` (exactly three colours, low to high) set them; each theme's ramp is authored in OKLCH and its lightness moves steadily away from the surface, so salience rises with value. Sequential encodings use the ramp, never the categorical palette. Short palettes repeat through the eight series slots. The accent is first except in hand-drawn, whose six grays repeat and whose red pencil is reserved for the stat underline.

With `encoding.size`, scatter bubble areas are proportional to finite nonnegative values; the largest plotted positive size has a 16 px radius. Zero has no filled area and remains available through keyboard and author-precision readouts. Missing, negative or nonnumeric sizes omit the bubble and produce diagnostics; their original rows remain available in the table and CSV. Plots with no positive bubble sizes include an explanatory note. Without `encoding.size`, scatter points have a fixed 4 px radius.

Sized scatter plots with a positive size include an outlined reference circle with a 16 px radius below the axes, labelled with the complete size field and the finite value, at the author's precision, used by that panel's area scale. The guide follows the currently rendered rows and each facet's existing local scale. It does not represent an additional observation or inspection target. It appends height without moving the plot; an explicit render height can grow with a `fixed-height` diagnostic. Tiny or long scale numbers use scientific notation when necessary to retain their value.

For one-signed percentage scatters without authored Y bounds or annotations, the axis ends at zero when every drawn point or bubble still has 4 px of space beyond its radius at both Y edges. Values at or near zero retain the padding they need. Trimming unused space preserves the opposite endpoint, tick values, bubble areas and a linear value scale.

Percentage axes multiply fractional values by 100 and share the precision required by their tick step, up to eight decimal places. Nonzero percentages retain at least one decimal; zero is printed as `0%`. An authored `%` suffix appends the unit without scaling values.

Numeric axes and the heatmap key use scientific notation when the existing numeric body exceeds 12 characters, a nonzero scale would round to zero, or its tick step cannot be expressed at eight decimal places. Coefficient precision follows the existing step, up to 17 significant digits. Explicit display units and percentage conversion still apply once; automatic `k`/`M`/`B` suffixes are omitted in scientific notation. Exact source readouts and compact cell/end-value labels keep their separate formatting policies. Shorter labels can change the existing automatic tick layout and plot gutter; source values and authored bounds remain unchanged.

| Annotation | Anchor fields | Text |
| --- | --- | --- |
| `point` | `at` coordinate mapping | Required. |
| `range` | `from`, `to` coordinate mappings | Optional. |
| `line` | `at` coordinate mapping | Optional. |
| `band` | `from`, `to` coordinate mappings | Optional. |
| `text` | `at` coordinate mapping; optional numeric `rotate` | Required. |

Anchor coordinates are strings or numbers. Cartesian annotations use axis/source fields; pie points identify a named slice, and heatmap points identify a cell. Unsupported coordinates remain diagnosable rather than inventing observations. Labels search around their exact anchors with clearance from marks and already-placed glyph boxes; placement never moves an observation. If no clear position exists, optional text is omitted with a dropped-label reason. Browser checks measure actual glyph rectangles after fonts settle.

Numeric text below the representable nonzero range reports a source error instead of silently becoming zero. This applies to numeric coordinates, measures and scatter sizes, including CSV sources; category labels and unused metadata retain their text. The affected facet explains the error and retains its source Table/CSV while other panels remain usable. True zero and representable subnormal values remain valid. Explicit table number formats retain unrepresentable text verbatim. JSON/YAML numeric literals already converted by their parser cannot recover lost decimal spelling; use quoted numeric text when exact source representation matters.

Calendar and numeric line/area charts share the same geometry engine and point-retention policy. Calendar x changes the axis and gap rules. Layout uses the source values; readers receive values at the author's precision and bounded position grids, including when every observation is drawn. Already rounded JSON numbers cannot recover their original decimal spelling.

Bar, scatter, pie and non-temporal line/area readouts honor numeric axis unit and percentage formats at the author's precision (three significant digits by default, `digits` per chart or column); no measured value reaches the HTML at more precision than the author chose; a position decodes to no more digits than the larger of the author's precision and 1/4,096 of the axis, and a zoom stops where one grid step spans about two pixels; dates keep their source resolution, and integer scales with fewer than 4,096 intervals use unit steps; category names, identifiers and other text the author wrote travel as written (TYP-36, FEA-CH-37). Heatmap measures use `axes.color` labels, formats and suffixes, while coordinate names retain their nominal role; a color field reused as a coordinate keeps that coordinate's existing readout. Percentage format converts fractions to percentages; a literal `%` suffix alone does not rescale values. Missing measures read `No observation`, and invalid text remains verbatim. Table and CSV values carry the same author-precision values the readouts show. Source tables include fields introduced by later rows, even beyond the first page. CSV downloads encode arrays and objects as quoted JSON cells so embedded commas and quotes cannot shift columns. Temporal readouts use the time-series engine's separate formatting policy.

## Report folders, local data and output

Each rendered report requires its own named folder with exactly one `.rmd` directly inside it. Missing sources/roots, multiple `.rmd` files (including nested ones), unreadable inventories, escaped paths and external symlinks are hard failures. CLI commands exit nonzero; editor/service/browser hosts expose actionable structured diagnostics. Parsing, formatting and static suggestions stay available to repair invalid source. No host infers a report from its process working directory.

```text
quarterly-sales/
  report.rmd
  report.rstyle.yaml       # optional; referenced explicitly
  data/sales.csv          # optional larger input
  media/logo.svg          # optional local media
  Rendered/               # generated; exact capital R
    report.html
    data/sales.csv        # copied when used, including local links
    media/logo.svg        # copied when used; media is also embedded
    report.rstyle.yaml    # copied when referenced
```

Small authored charts/tables default to expanded row mappings inside the consuming block:

```yaml chart
type: bar
title: Revenue increased from January to February
encoding: {x: month, y: revenue}
data:
  inline:
    - month: Jan
      revenue: 100
    - month: Feb
      revenue: 140
# For larger inputs choose exactly one block-local file reference instead:
# data: {csv: data/sales.csv}
# data: {json: data/sales.json}
```

Header `data` and block `data.id` are retired and fail with replacement guidance. For portable report bundles, each block chooses exactly one of `inline`, `csv` or `json`; file references are relative, bounded paths inside the same report root. Native hosts check canonical paths including symlinks; browser/source-only hosts must supply a matching explicit `report_folder: {root, source, files}` inventory before resolving or rendering. Static browser bundles use a sibling `report-folder.json`, generated by the builder. In-memory playground drafts have one explicit isolated virtual folder and support inline/embedded resources only.

Configured native hosts also resolve `data.sql`, `data.metricflow` and `data.cache` through named connectors declared in `report.connectors.yaml` beside the report. The host must supply the connector's runtime and configuration; a missing connector declaration is a resource-resolution failure. See `examples/sql-report` for a local DuckDB report. Credentials stay in trusted host configuration.

The accepted `data.url` field is not implemented by current native or browser loaders. Save remote input as a report-local CSV or JSON file for validation, rendering and portable exports.

Pure core performs no I/O. Hosts provide resolved CSV/JSON bytes in `RenderOptions.data` keyed by the exact local reference, with style sources/media supplied by their adapters. The source key decides the parser, never the content: `csv:` content is CSV and `json:` content is JSON, and content that does not parse as the declared format is `data.csv` or `data.json` (error) at the block's line.

Data typing is one contract for inline, CSV and JSON rows in every block:

- **CSV** is UTF-8 with a dropped BOM, RFC 4180 quoting and a comma delimiter only; there is no `delimiter` option, and a semicolon file fails as ragged rows. A CSV or JSON file in another encoding, such as an Excel export in Windows-1250, is a `data.csv` or `data.json` error at its first byte that is not UTF-8, and only the blocks that read it fail. Every header and value is trimmed. The header is the first row; names must be non-empty and unique, else `data.csv.header`. A row with a different number of fields is `data.csv` and names the file line. An empty cell is null; `true` and `false` are booleans; a number whose decimal value survives binary64 is a number (`10.50` becomes 10.5); a token with a leading zero (`00123`; not `0` or `0.5`) is a string identifier that sorts as text and is never totalled; an integer beyond 2^53 or a decimal that does not round-trip is numeric text that keeps its digits; anything else is a string.
- **JSON** is an array of flat row objects whose values are strings, finite numbers, booleans or null. Any other top-level shape, including the former `{"inline": [...]}` wrapper, or a nested object or array value is `data.json` (error) naming the row and key.
- **Inline** rows follow the JSON value rule: a row that is not a mapping, or a nested value, is one `data.inline` error at that row (`table.data.inline[1]`), naming the row and key.
- **Missing values** are null, an empty CSV cell, or `.`, `NA`, `N/A`, `NaN`, `null` and `none` in any case after trimming. Charts draw a gap and count the rows in a `chart.missing-observations` info check. Tables print the cell as written, sort it after every value in both directions and leave it out of totals. Any other text in a numeric chart channel, including comma-grouped numbers such as `1,234`, is `chart.non-numeric` (error) naming the row and value; in a table it stays text.
- **Dates and times** are strings only (a quoted year is temporal, an unquoted year a number). `YYYY` is year precision, `YYYY-MM` month, `YYYY-MM-DD` day; a date followed by `T`, `t` or a space and `HH:MM` is minute precision, with `:SS` second precision and a fraction of 1 to 9 digits after `.` or `,` millisecond precision (digits beyond three are dropped). A date-time may end in `Z` or `z`, `±HH`, `±HHMM` or `±HH:MM`; without an offset it is UTC. Surrounding spaces are ignored. Out-of-range fields (month 13, 30 February, 25:00, an offset of 24 hours) make the value a non-date. An axis is calendar only when every `x` value qualifies.
- **Limits**, enforced in core for every host: a `.rmd` source over 16 MiB is `source.limit`; one resolved dataset over 16 MiB is `data.limit`; generated HTML over 64 MiB is `render.output-limit`.

CSV numeric cells become JSON numbers only when their decimal value survives binary64 parsing and JSON number serialization. Longer decimals and integers that would lose digits, such as `1.234567890123456789` or `9007199254740993`, remain source text; chart coordinates still use binary64 while readouts and source downloads retain those digits. Ordinary numeric typing and insignificant trailing-zero normalization are unchanged. JSON input cannot recover digits already rounded by its producer.

Native `generate` and VS Code **RichView: Export to Rendered** create `Rendered/<source-basename>.html`. `-o name.html` selects another basename there; an explicit output path must be directly inside that same `Rendered` directory. Source/style/data are preserved. Output directories/files cannot escape or be symlinks, and inputs cannot collide with output. CSS, licensed fonts, data/media and the Rust chart runtime are embedded in HTML; referenced local files are also copied under `Rendered` with matching relative paths. Missing interactive runtime refuses chart export with a build/package instruction. Browser downloads cannot select a filesystem directory: save their self-contained HTML inside your report's `Rendered`; reports with local document links must use native/editor export for the copied bundle (browser download refuses them). Explicit external web links/embeds still depend on their remote destinations.

### PDF output

`richview generate report.rmd --to pdf` writes `Rendered/report.pdf` atomically. A `.pdf` output filename also selects this mode. `--page a4|letter` defaults to A4; `--browser PATH` or `RICHVIEW_BROWSER` selects a local Chromium-family browser, otherwise discovery checks PATH and macOS application bundles. Linux and macOS are supported; Windows and hosted rendering are not. Chrome 131 or later provides running titles and page counters. `--settle-timeout` defaults to 30 seconds. MCP `richview_render` accepts `output: report|pdf` and `page: a4|letter`. Core/JavaScript `RenderOptions.output` returns print-ready HTML; filesystem export and browser execution belong to the native host.

PDF HTML carries `data-rv-output="pdf"` and chart payloads with `output: "pdf"` and `motion.enabled: false`. The driver emulates print/reduced motion at the page text width, awaits the runtime's `window.__richview.settled()` hook (or fonts, chart mounts and final frames with older runtimes), then requests a tagged PDF with an outline. All browser requests except the staged report, `data:` resources and GET requests for the images of credited `https://images.unsplash.com/` photo figures (image blocks and `title_image`) that the report shows are blocked (`export.network-blocked`). Data, styles and media retain the bounded report-folder contract; PDF mode allows no other remote resources.

The print stylesheet is present in every report. Sleek prints as Air, the light builtin its `print-as` token names; other styles keep their treatment. PDF page margins are 20 mm top, 18 mm sides and 22 mm bottom, with a running title and `page / pages` footer after the first page. Content keeps its source order; figures and their following source note, callouts, metrics and table rows avoid breaks. Tables print all rows, repeat headers and print totals once. Video becomes a poster plus caption/filename, or a diagnosed text card without a poster; audio and embeds become link cards; PDF HTML emits no players or iframe. Local links print as text. Images load eagerly, chart controls are hidden, and the existing RichView colophon follows the content. No `outputs.*` frontmatter, carousel or story output is implemented by this contract.

Whole HTTP(S) URL table cells link with the hostname as their screen text and the full address in `href` and `title`; print uses the full address. ISO dates and identifier-like cells keep their tokens on one line. Table payloads, sorting and CSV values retain the original source strings.

The former `--allow-network` flag is removed. Keep data inside the report folder or use a configured native connector.

## Optional styling YAML

No styling file is required. A report without `styles` uses the builtin marked as the default in the [style reference](#styles-and-motion); builtin-only `styles: [scientific]` or `[sleek]` also needs no local YAML. To customize the existing grammar, place any named `.yaml`/`.yml` file beside the source and reference it explicitly, for example `styles: [scientific, report.rstyle.yaml]`. The `.rstyle.yaml` suffix is the convention, not auto-discovery. Omit/remove the reference if you remove the file; a declared missing file is an actionable resource error.

```yaml style
# report.rstyle.yaml — ordered after the scientific builtin
tokens:
  bg: 'oklch(0.9695 0.0336 86.31)'
  accent: 'oklch(0.4806 0.1597 25.56)'
chart:
  palette: ['oklch(0.4806 0.1597 25.56)', 'oklch(0.5429 0.0892 211.21)']
  bar-radius: 8
```

Use the established `tokens`, `layout`, `components`, `motion` and `chart` sections and their safe values; see the style section below. Native `--style` overrides follow source styles and remain report-local. An unspecified chart palette inherits the report style, including a ratio-only `style`; an explicit `style.palette` overrides it. Save a local YAML edit to refresh the VS Code/native live preview. In the browser authoring selector, **Source / default** uses source styling YAML; choosing a canonical style applies an explicit later override. Switching back to **Source / default** restores local styling without changing source/data. Canonical switches rerender the same report, including its chart payload.

## Renamed and removed names

These names are no longer accepted. The table is generated from the same definitions used by validation; style leaves with a removal warning are ignored. Use the replacement instead of copying an old name into new source.

<!-- registry:begin retired -->
| Kind | Scope | Removed name | Replacement |
| --- | --- | --- | --- |
| field | `chart` | `data.id` | Write the data in this block: `data.inline`, a report-relative `data.csv` or `data.json`, or a connector configured in `report.connectors.yaml`. |
| field | `chart` | `data.headers` | Save remote data inside the report folder and use `data.csv` or `data.json`, or use a configured connector; credentials stay outside the report. |
| field | `table` | `data.id` | Write the data in this block: `data.inline`, a report-relative `data.csv` or `data.json`, or a connector configured in `report.connectors.yaml`. |
| field | `table` | `data.headers` | Save remote data inside the report folder and use `data.csv` or `data.json`, or use a configured connector; credentials stay outside the report. |
| field | `chart` | `axes.x.suffix` | Rename `suffix` to `unit` (TYP-16). |
| field | `chart` | `axes.y.suffix` | Rename `suffix` to `unit` (TYP-16). |
| field | `chart` | `axes.color.suffix` | Rename `suffix` to `unit` (TYP-16). |
| field | `chart` | `encoding.highlight` | Move `encoding.highlight` to the top-level `highlight` field (TYP-78). |
| field | `chart` | `style.line` | Replace `style.line.smoothing` with `style.curve`: straight is `linear`, monotone is `monotone` (TYP-17). |
| field | `image` | `title` | Rename `title` to `alt`, the image's text alternative (TYP-29). |
| field | `audio` | `alt` | Rename `alt` to `title`, the clip's accessible name (TYP-30). |
| field | `video` | `alt` | Rename `alt` to `title`, the video's accessible name (TYP-31). |
| field | `frontmatter` | `title-image` | Rename `title-image` to `title_image`. |
| field | `frontmatter` | `image` | Rename `image` to `title_image`. |
| field | `frontmatter` | `subtitle` | Move the text into `description`, the one deck under the title. |
| field | `frontmatter` | `prolog` | Move the text to the top of the body; the opening text is the first paragraph. |
| field | `frontmatter` | `theme` | Remove `theme`; a style is one colour scheme, so choose `sleek` (dark) or `air` (light) in `styles`. |
| field | `frontmatter` | `data` | Move each dataset into the `data` mapping of the block that uses it. |
| tag | fence | `big` | `metric` |
| value | `chart.axes.x.format` | `currency` | `number` with unit `$` |
| value | `chart.axes.x.format` | `currency_compact` | `number_compact` with unit `$` |
| value | `chart.axes.y.format` | `currency` | `number` with unit `$` |
| value | `chart.axes.y.format` | `currency_compact` | `number_compact` with unit `$` |
| value | `chart.axes.color.format` | `currency` | `number` with unit `$` |
| value | `chart.axes.color.format` | `currency_compact` | `number_compact` with unit `$` |
| value | `table.columns[n].format` | `currency` | `number` with unit `$` |
| value | `table.columns[n].format` | `currency_compact` | `number_compact` with unit `$` |
| value | `metric.format` | `currency` | `number` with unit `$` |
| value | `metric.format` | `currency_compact` | `number_compact` with unit `$` |
| style-name | styles | `sleek-dark` | `sleek` |
| style-name | styles | `sleek-light` | `air` |
| style-name | styles | `editorial` | `brutalist` |
| style-field | style | `motion.delay` | motion has no delay: blocks and marks arrive on their own timing (motion.duration, motion.stagger) |
| style-field | style | `components.callout.left-bar-width` | callouts have no side bar; components.callout.border-width sets the rule above |
<!-- registry:end -->

## Migration from header datasets or shared folders

Create one dedicated folder per report and move its `.rmd` plus needed data/style/media there. Keep original bytes and update paths relative to that new root. For each old `data: {id: sales}` block, copy that dataset's `inline` mapping directly into the block, or replace it with the same `csv`/`json` relative file reference. Repeated consumers can reference the same local file. Remove frontmatter `data` only after every consumer has migrated. Preserve row order, field names, types, nulls and exact values; migration is an authoring change, not a transform. Regenerate into `Rendered` and compare the resulting values before replacing old output. The task-owned examples demonstrate this layout; historical artifacts and peer worktrees are unchanged.

## Tables and metrics

```yaml table
# table
data:
  inline:
    - month: Jan
      revenue: 100
    - month: Feb
      revenue: 140
columns:
  - key: month
    label: Month
  - key: revenue
    label: Revenue
    format: number
    unit: $
    align: right
sort: {key: revenue, dir: desc}
max_rows: 20
total: true
```

Use a tagged `table` YAML fence for structured table authoring. Its fields are generated below. See [the readability comparison](TABLE-AUTHORING-CHOICE.md) for the pending series proposal; it is not part of this contract. Tables require `data`; `columns` is optional and otherwise derives from the first row. Columns accept string `key`, `label`, `format` and `align: left|center|right`. Source sorting accepts `asc|desc`, defaulting to `asc`. `max_rows` defaults to 20 and must be a positive integer; `total` defaults to false. Native `details` exposes remaining rows; header-click sorting uses the Rust browser runtime and the same core table operation.

<!-- registry:begin fields/table -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `title` | text |  |  | A caption above the table, also its accessible name. |
| `data` | data source | yes |  | The table's data; exactly one local source is allowed. |
| `data.inline` | list of mapping |  |  | Rows written in the block, each a mapping of column to value. |
| `data.csv` | report-folder file |  |  | A CSV file inside the report folder. |
| `data.json` | report-folder file |  |  | A JSON file of row mappings inside the report folder. |
| `data.url` | `http` or `https` address |  |  | An https data source; current native and browser hosts do not load this field. |
| `data.sql` | mapping |  |  | A SQL query run through a named connector on a host that has a registry. |
| `data.sql.connector` | text | yes |  | The name of the connector that runs the query. |
| `data.sql.query` | report-folder file |  |  | A report-relative file holding the SQL; set this or inline, not both. |
| `data.sql.inline` | text |  |  | The SQL text itself; set this or query, not both. |
| `data.metricflow` | mapping |  |  | A MetricFlow query run through a named connector. |
| `data.metricflow.connector` | text | yes |  | The name of the connector that runs the query. |
| `data.metricflow.metrics` | list of text | yes |  | The metric names to query; at least one. |
| `data.cache` | mapping |  |  | A reference to a cached connector result. |
| `data.cache.key` | text | yes |  | The key of the cached result. |
| `data.sql.params` | mapping |  |  | Named values passed to the query. |
| `data.sql.cache` | mapping |  |  | How long the query's result is reused. |
| `data.sql.cache.key` | text |  |  | The key of the cached result. |
| `data.sql.cache.ttl` | text |  |  | How long the result stays fresh, such as 1h. |
| `data.sql.cache.freshness` | text |  |  | A freshness rule for the result. |
| `data.sql.cache.refresh` | `true` or `false` |  |  | Whether every run refreshes the result. |
| `data.metricflow.group_by` | list of text |  |  | The dimensions to group by. |
| `data.metricflow.where` | list of text |  |  | Filters on the query. |
| `data.metricflow.order_by` | list of text |  |  | The order of the rows. |
| `data.metricflow.limit` | whole number |  |  | The most rows returned. |
| `data.metricflow.cache` | mapping |  |  | How long the query's result is reused. |
| `data.metricflow.cache.key` | text |  |  | The key of the cached result. |
| `data.metricflow.cache.ttl` | text |  |  | How long the result stays fresh, such as 15m. |
| `data.metricflow.cache.freshness` | text |  |  | A freshness rule for the result. |
| `data.metricflow.cache.refresh` | `true` or `false` |  |  | Whether every run refreshes the result. |
| `data.cache.ttl` | text |  |  | How long the result stays fresh. |
| `columns` | list of mapping |  |  | The columns to show, in order; all columns when omitted. |
| `columns[n].key` | column name | yes |  | The data column this table column shows. |
| `columns[n].label` | text |  |  | The column heading. |
| `columns[n].format` | one of `number`, `number_compact`, `percent` or `year` |  |  | The number format of the column's values. |
| `columns[n].unit` | text |  |  | A unit printed with each value, by the unit rule. |
| `columns[n].digits` | whole number of significant digits from 2 to 6 |  |  | Significant digits of the column's numbers, not decimal places. |
| `columns[n].align` | one of `left`, `center` or `right` |  |  | The alignment of the column's cells. |
| `sort` | mapping |  |  | The row order. |
| `sort.key` | column name | yes |  | The column to sort by. |
| `sort.dir` | one of `asc` or `desc` |  | `asc` | The sort direction. |
| `max_rows` | whole number |  | `20` | The most rows shown; a positive whole number. |
| `total` | `true` or `false` or list of column name |  | `false` | A total row: true sums the numeric columns that are not percentages or years, a list of columns sums those. |
| `wide` | `true` or `false` |  | `false` | Whether the table takes the report's full width instead of the text measure. |
<!-- registry:end -->

Ascending table order is numeric values, then textual values, then missing cells (null and the markers `.`, `NA`, `N/A`, `NaN`, `null`, `none`); descending reverses the values and keeps missing cells last. Within the supported numeric range, precise text retained from CSV sorts by its exact decimal value, including values that share a rounded binary64 representation. A reader who sorts by a column header orders numbers by the value each cell shows: cells that show the same value keep the table's order, and the page carries no digit a cell does not show. Leading-zero CSV identifiers and authored inline strings remain textual. Numeric totals require numeric cells throughout the column apart from missing cells, which are left out; they include retained CSV numbers, and keep the existing binary64 calculation and selected column format.

```yaml metric
# metric
value: 4234567
label: Q3 revenue
format: number_compact
unit: $
delta: 0.18
delta_label: vs Q2
trend: up
```

<!-- registry:begin fields/metric -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `value` | number or text | yes |  | The headline number or text the block shows. |
| `label` | text |  |  | The caption under the value. |
| `unit` | text |  |  | A unit shown with the value. |
| `format` | one of `number`, `number_compact`, `percent` or `year` |  |  | The number format of the value. |
| `delta` | number |  |  | The change since the last period as a fraction, shown beside the value as a signed percentage: 0.12 prints +12.0%. |
| `delta_label` | text |  |  | The caption of the change. |
| `digits` | whole number of significant digits from 2 to 6 |  |  | Significant digits of the value and the change. |
| `trend` | one of `up`, `down` or `flat` |  |  | The delta's colour, `up` for good news and `down` for bad; it defaults to the sign of the delta. |
<!-- registry:end -->

Metrics require numeric or string `value`. Table columns, metrics and axes share one `format` vocabulary, listed in the field tables; another word is `block.schema`. The retired currency formats become a number format plus `unit: $`, `€`, `£` or `¥`, and authors replace them explicitly in source. Percentages multiply fractional values by 100. Compact values use scale words in tables and metrics (`12.5 thousand`) and K, M and B on axes. `year` prints a whole number without grouping (`2024`, never `2,024`), and `total: true` skips year columns as it skips percent ones. `digits` (2 to 6) sets the significant digits; only numeric cells of a table are formatted (identifiers such as `001` stay text). A `unit` ending in a currency symbol (`$`, `US$`, `€`, `£`, `¥`) leads the value; any other unit follows it with the chart axis rule (`68.2%`, `12 users`). Formatting can round displayed values without changing source data.

A metric renders as a stat line, never a filled panel: the value with its unit in the display face at `size-h1` with lining tabular figures, then the label and the signed delta at the small size. The delta carries the trend colour (`trend: up` and `down` use the `up` and `down` tokens). The style's `components.metric.style` chooses the rules around the stat; `knockout` also sets the value in the accent.

## Callouts and media

```yaml callout
# callout
kind: hold
title: Sample size caveat
body: |
  Only twelve weeks of post-launch data are available.
```

<!-- registry:begin fields/callout -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `kind` | one of `up`, `down`, `neutral` or `hold` | yes |  | The direction the callout signals. |
| `title` | text |  |  | The callout heading. |
| `body` | inline Markdown | yes |  | The callout text. |
<!-- registry:end -->

Callouts require `kind: up|down|neutral|hold` and a Markdown `body`; `title` is optional. A callout is an aside under one rule, without a fill, outline or side bar, and its body stays within the text measure. The title runs into the first paragraph in the display face, coloured by kind (`up`, `down` and `hold` tokens; ink for `neutral`); when the body opens with a list or another block, the title stands on its own line. Images require `src` and `alt` (`""` marks a decorative image; the old `title` is `alt`), with optional escaped `caption`, `fit: contain|cover|fill` (default contain), positive integer `width`/`height` and a `#rrggbb` `color`. Audio and video require `src` and a `title` (the accessible name; the old `alt` is `title`), use native controls and metadata preloading, and accept an optional `caption`; video also accepts a safe image `poster`, dimensions and `wide`. Audio takes no dimensions or poster. Native generation embeds local media, including posters and Markdown images in body and callout content. Safe caller-provided image/audio/video data URIs are supported.

An image `src` may also be a credited `https://images.unsplash.com/...` photo (lowercase host, no userinfo or port). It is the only remote image origin a report may use, for image blocks and the `title_image` header alike, and it is a hotlink: no host fetches, embeds or inventories it, and the reader's browser loads it when the report is read. Every other remote image, audio, video, poster and Markdown image (including an Unsplash one outside an image block or `title_image`) must still be a report file: a remote address there is a `report.asset.remote` error at its field, from `validate` and the render alike, and only that element fails. An Unsplash photo needs a `credit`. An optional `credit` mapping attributes the photo:

```image
src: https://images.unsplash.com/photo-1559538834-8b012e20a84f?w=1600&q=80&fm=jpg&fit=max
alt: A calm sea under a pale sky
credit:
  author: Marek Rucinski
  author_url: https://unsplash.com/@marulla?utm_source=richview&utm_medium=referral
  source: Unsplash
  source_url: https://unsplash.com/?utm_source=richview&utm_medium=referral
```

<!-- registry:begin fields/image -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `src` | text | yes |  | The image: a report-relative path, a data address or a credited Unsplash photo address. |
| `alt` | text | yes |  | The text alternative; `""` marks a decorative image. |
| `caption` | text |  |  | The caption under the image. |
| `fit` | one of `contain`, `cover` or `fill` |  | `contain` | How the image fills its box. |
| `width` | whole number |  |  | The image width in pixels; a positive whole number. |
| `height` | whole number |  |  | The image height in pixels; a positive whole number. |
| `credit` | mapping |  |  | The photo credit; an Unsplash photo needs one. |
| `credit.author` | text | yes |  | The photographer's name; a non-empty string. |
| `credit.author_url` | text |  |  | The photographer's profile; required on an Unsplash photo, which pins it to unsplash.com. |
| `credit.source` | text |  |  | The site the photo comes from; Unsplash on an Unsplash photo. |
| `credit.source_url` | text |  |  | The photo's page on the source site. |
| `color` | any value |  |  | A #rrggbb colour that fills the box until the photo loads; another value is dropped with a warning. |
| `wide` | `true` or `false` |  | `false` | Whether the image takes the report's full width instead of the text measure. |
<!-- registry:end -->

`credit.author` is required and a non-empty string; `author_url`, `source` and `source_url` are optional strings, except on an `images.unsplash.com` photo, where the credit is pinned to Unsplash: `author_url` is required and must be an `https://unsplash.com/...` address (lowercase host, no userinfo or port), `source_url` if present must be on `https://unsplash.com/`, and `source` if present must be `Unsplash` (case-insensitive). Any violation is an `image.credit-invalid` error at the offending `credit` key; the image block stays visible as source and the header image is omitted, as with `image.credit-missing`. Credits on local images keep the looser rule below. The image figure ends with `Photo by <author> on <source>` after any caption, and prints in PDF output. Each name links (`target="_blank" rel="noopener noreferrer"`) only when its URL is plain http(s); an unsafe URL keeps the text, drops the link and warns `image.credit-url`. A credit without a usable author warns `image.credit-invalid` (header image, local images) or is a `block.schema` error (image block). An image served from `images.unsplash.com` without a credit is an `image.credit-missing` error: the image block stays visible as source and the header image is omitted, as with any recoverable invalid block. The same `credit` mapping works in a `title_image` mapping. `richview image search|use` (and the MCP tools `richview_image_search` and `richview_image_use`) find a photo and write this YAML, with the download registered as the Unsplash API guidelines require.

A hotlinked photo stays readable offline. The HTML keeps the URL and embeds no bytes. With `width` and `height` the figure reserves its box (`aspect-ratio`) before the photo loads, and an optional `color: '#rrggbb'` (the photo's dominant colour, which `richview image use` writes from the API's `color`) is painted behind the image, with alt text in a contrasting ink. Any other `color` value is dropped with an `image.color-invalid` warning. The caption and the credit line always render, so offline the text and credit remain and only the photo is missing. Rendering never touches the network: the Unsplash download ping happens once, at `richview image use`. The native PDF export loads exactly the credited `images.unsplash.com` photos of its image blocks and `title_image` (GET requests for images only; the renderer marks them `data-rv-photo`) and prints them as the page shows them; a photo that cannot load, or does not answer within a few seconds, prints as its box and credit with an `export.photo-unavailable` warning.


<!-- registry:begin fields/embed -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `url` | text | yes |  | The absolute address to embed; the host must be allowed and https. |
| `height` | whole number |  | `420` | The frame height in pixels; a positive whole number. |
| `allow` | list of text |  | `[youtube.com, www.youtube.com, youtube-nocookie.com, vimeo.com, player.vimeo.com, gist.github.com, codepen.io]` | The hosts that may be embedded. |
| `caption` | text |  |  | The caption under the frame. |
| `wide` | `true` or `false` |  | `false` | Whether the frame takes the report's full width instead of the text measure. |
<!-- registry:end -->

Embeds require an absolute HTTPS `url`. `height` defaults to 420; `caption` is optional. The default allowlist is `registry::EMBED_HOSTS`: youtube.com, www.youtube.com, youtube-nocookie.com, vimeo.com, player.vimeo.com, gist.github.com and codepen.io. A blocked embed is the error `security.embed`. An authored `allow` host array replaces it, matching the host or its subdomains. URLs containing credentials are blocked. Known third-party player hosts receive script/presentation/same-origin sandbox permissions; custom author-allowed hosts omit same-origin permissions. Embeds remain network-dependent.

Audio and video:

<!-- registry:begin fields/audio -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `src` | text | yes |  | The audio file: a report-relative path or a data address. |
| `title` | text | yes |  | The player's accessible name. |
| `caption` | text |  |  | The caption under the player. |
| `transcript` | report-folder file |  |  | A report-relative Markdown file shown under the player as its transcript. |
<!-- registry:end -->

<!-- registry:begin fields/video -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `src` | text | yes |  | The video file: a report-relative path or a data address. |
| `title` | text | yes |  | The player's accessible name. |
| `caption` | text |  |  | The caption under the player. |
| `poster` | text |  |  | A poster image; without one the PDF prints a text card. |
| `transcript` | report-folder file |  |  | A report-relative Markdown file shown under the player as its transcript. |
| `captions` | report-folder file |  |  | A report-relative WebVTT file embedded as the video's captions track. |
| `width` | whole number |  |  | The player width in pixels; a positive whole number. |
| `height` | whole number |  |  | The player height in pixels; a positive whole number. |
| `wide` | `true` or `false` |  | `false` | Whether the player takes the report's full width instead of the text measure. |
<!-- registry:end -->

Scrollytelling keeps one image in view beside ordered steps:

<!-- registry:begin fields/scrolly -->
| Field | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| `title` | text |  |  | The heading of the scrollytelling element. |
| `chart` | any value |  |  | A chart as the figure, in place of `image`: the same keys as a chart block. |
| `image` | mapping |  |  | The one figure that stays in view beside the steps. |
| `image.src` | text | yes |  | The image file: a report-relative path or a data address. |
| `image.alt` | text |  |  | The text alternative. |
| `image.caption` | text |  |  | The caption under the figure. |
| `image.fit` | one of `contain`, `cover` or `fill` |  | `contain` | How the image fills its box. |
| `image.width` | whole number |  |  | The image width in pixels; a positive whole number. |
| `image.height` | whole number |  |  | The image height in pixels; a positive whole number. |
| `image.color` | any value |  |  | A #rrggbb colour that fills the box until the photo loads. |
| `image.credit` | mapping |  |  | The photographer's credit; an Unsplash photo needs one. |
| `image.credit.author` | text |  |  | The photographer's name. |
| `image.credit.author_url` | text |  |  | The photographer's page. |
| `image.credit.source` | text |  |  | The photo's source. |
| `image.credit.source_url` | text |  |  | The source's address. |
| `steps` | list of mapping | yes |  | The narrative steps, in reading order. |
| `steps[n].text` | inline Markdown | yes |  | The step's text. |
| `steps[n].title` | text |  |  | The step's heading. |
| `steps[n].view` | mapping |  |  | The part of the image shown when the step scrolls in: x and y as fractions of the image, and a zoom of at least 1. |
| `steps[n].view.x` | number |  | `0` | The view's left edge, as a fraction of the image width from 0 to 1. |
| `steps[n].view.y` | number |  | `0` | The view's top edge, as a fraction of the image height from 0 to 1. |
| `steps[n].view.zoom` | number |  | `1` | How far the view zooms in, from 1 (the whole image) to 20. |
| `steps[n].state` | mapping |  |  | For a chart figure: the control options the step sets, as control id to option value. |
| `steps[n].highlight` | mapping |  |  | A box on the image, as x, y, w and h fractions. |
| `steps[n].highlight.x` | number | yes |  | The box's left edge, as a fraction of the image width. |
| `steps[n].highlight.y` | number | yes |  | The box's top edge, as a fraction of the image height. |
| `steps[n].highlight.w` | number | yes |  | The box's width, as a fraction of the image width. |
| `steps[n].highlight.h` | number | yes |  | The box's height, as a fraction of the image height. |
<!-- registry:end -->

External assets are diagnosed, and `_richview.offline` is false while resources remain external. Unsafe media and link protocols are removed or replaced with visible blocked output. Raw HTML never renders (see Markdown extensions).

## Markdown extensions

CommonMark prose supports GFM tables, strikethrough, disabled task-list checkboxes and definition lists. Footnote references link to numbered endnotes at the end of the body, with links back to their references. A wikilink renders as its label and warns; links between reports use ordinary Markdown links. Quotations use a Markdown block quote with attribution on its last line. The `[!quote]` marker stays text and warns.

Prose uses typographic punctuation: straight quotes become curly quotes and apostrophes, `--` an en dash, `---` an em dash and `...` an ellipsis. Code spans and blocks, math, URLs (link targets, autolinks and bare `http(s)://` or `www.` addresses), command-line flags such as `--style` and ASCII arrows such as `-->` keep their characters. Table column alignment (`:--`, `:-:`, `--:`) is kept as the `center` and `num` cell classes; right-aligned columns use tabular figures.

Raw HTML in Markdown never renders, and source text never yields executable content:

- Each HTML tag is removed with a `security.html` warning at its line that names the Markdown alternative. The text inside the tag stays as text. HTML comments are dropped without a diagnostic and never reach the output. Entities such as `&nbsp;` still work; write sub- and superscripts with Unicode (m², CO₂, 10⁻³), whose digits and plus and minus signs every bundled font draws; a character outside the style's faces warns `style.glyph-missing`.
- A link target is `http`, `https`, `mailto`, `tel`, a `#` anchor or a file in the report folder. Any other target (`javascript:`, `data:`, `vbscript:`, `//host`, a backslash) is not linked: the text stays and `security.link` (warning) is reported.
- A Markdown image is a report file or a base64 `data:image/` URI. A remote one is a `report.asset.remote` error at its line and column, from `validate` and the render alike; the image is dropped and its alt text shows. Any other source is dropped with `security.image`.
- An explicit heading `{#id}` is kept only when a tab panel names it in its start anchor; other authored heading ids warn and are ignored, leaving the heading's generated anchor. A generated anchor alone cannot start a panel.
- Source never sets an id or class in the runtime's namespace. A heading `{#id}` or a footnote label that starts with `rv`, `richview`, `tab-` or `panel-` is dropped (headings) or renamed `fn-<label>` (footnotes) with a `security.id` warning; heading classes and attributes are dropped with `security.class`.
- Every text and attribute value is escaped where it is written, and an allowlist sanitizer runs as a second layer over the generated HTML. The only scripts in a report are its own JSON data elements and the report runtime.
- `tests/hostile/` holds a corpus of hostile sources that renders in every builtin style, in its own screen scheme and on paper, and fails when the output carries a script, an event handler, a script or remote URL, a runtime id or an unescaped marker (DEL-56).

A paragraph that directly follows a rich block (chart, table, metric, callout or media), is wholly emphasised and starts with `Source:`, `Sources:`, `Note:` or `Notes:` is that block's source note. It renders as `<p class="rv-source">` without the emphasis: roman, at the small size and directly under the block, with its label in a `<span class="rv-source__label">` that a style can letter (`components.source-label`). Every other paragraph, including one that only begins with emphasis, is unchanged. `richview check` warns when a chart or table has no such note; synthetic data identifies itself in the note.

`richview check` also notes unnamed tables, Markdown emphasis or link syntax in inline table cells, video without captions, audio or video without a transcript, and header images without a caption or credit. These findings return as structured diagnostics alongside the chart checks.

`$inline$` and `$$display$$` LaTeX become native MathML, with visible diagnostics for unsupported expressions. Financial currency spans remain literal; the flagship's dollar amounts are covered by a regression test. Code and ordinary Markdown links are not rewritten as wikilinks. Raw HTML is sanitized rather than evaluated; report source cannot execute JavaScript/JSX.

## Styles and motion

`.rstyle.yaml` recognizes `tokens`, `layout`, `chart`, `motion` and `components`. Every value passes a closed grammar. A colour is `#` with 3, 4, 6 or 8 hex digits, `rgb()`, `rgba()`, `hsl()`, `hsla()`, `lab()`, `lch()`, `oklab()` or `oklch()`, or one of the CSS Color 4 named colours (148 keywords) plus `transparent` and `currentcolor`, in any case; any other word is refused. A chart `palette` or `ramp` entry must also be a colour the chart can draw, so `transparent` and `currentcolor` are valid only for page tokens. A refused leaf is skipped and reported; the rest of the file still applies. The builtin names and default below come from the same definitions as the style loader. Removed names fail with `style.renamed`; see [renamed and removed names](#renamed-and-removed-names).

<!-- registry:begin styles -->
| Style | Default |
| --- | --- |
| `sleek` | yes |
| `air` |  |
| `scientific` |  |
| `brutalist` |  |
| `hand-drawn` |  |
<!-- registry:end -->

The separate flagship showcase has its own presentation selector.

Style references are applied in order, shallow last-wins by section leaf; arrays replace rather than concatenate. Explicit styles do not implicitly inherit another builtin's component overrides. Unknown top-level sections are ignored. Unknown/invalid token/layout/motion/component leaves diagnose and skip that style; CSS-bound strings use closed color/font/dimension grammars. Retired leaves (`motion.delay` and `components.callout.left-bar-width`) report a `style.retired` warning and are ignored, and the rest of that style still applies. The chart section is supplier-owned, with guards on CSS-bound colors/fonts.

| Section | Vocabulary |
| --- | --- |
| `tokens` | `bg`, `bg-elev`, `bg-chart`, `ink`, `ink-soft`, `ink-faint`, `rule`, `rule-soft`, `accent`, `accent-soft`, `accent-2`, `link`, `up`, `down`, `hold`, `neutral`, `font-text`, `font-display`, `font-ui`, `font-mono`, `size-small`, `size-base`, `size-h3`, `size-h2`, `size-h1`, `size-display`, `line-base`, `measure`, `radius-s`, `radius-m`. Each size token accepts a `-phone` variant (for example `size-display-phone`) that applies at 640 px and narrower. `link` defaults to the accent. |
| `layout` | `report-max-width`, boolean `measure-wide-blocks` (header, prose, headings, tables and charts share one centred `measure` column; blocks with `wide: true` still break out). |
| `chart` | Closed chart styling for palettes, ramps, lettering, bar rounding, interpolation, decals and sign cues. Use canonical hyphenated keys such as `bar-radius`; unsupported leaves produce `style.unknown-key`. The examples above exercise the authoring grammar. |
| `components` | `metric.style`: `rule` (one rule above the stat), `bracket` (rules above and below), `knockout` (accent numerals under a 4 px ink rule) or `underline` (one pencil stroke in the accent under the value); none fills a panel. `callout.border-width`: the rule above callouts; `callout.style`: `rule` (default) or `bracket` (a pencil bracket down the left side instead). `rules.style`: `hairline` (default) or `pencil`, which draws the header, section, footer and table-head rules as pencil strokes. `deck.style`: `italic` or `normal` (default) for the deck, honoured only when the text family embeds a true italic face. `header.border-top`/`border-bottom` rules. `title`, `heading` (h1–h3 and callout titles), `table-head` and `source-label` (the Source/Note label of a source note): `transform`, `weight`, `letter-spacing` and `font` (`display`, `text`, `ui` or `mono`). |
| `motion` | `enabled`: `false` renders charts in their settled state. |

Canonical font faces are embedded from the existing licensed assets; mixed font cascades select the required faces. Native builds compile the font catalogs in; WebAssembly hosts load the staged `richview-fonts.css` and provide it to the core before rendering, and a render without it falls back to system fonts with a `style.fonts-unavailable` warning. Bold is requested only from a family that embeds it: when the display family has no face of weight 600 or more (the `hand-drawn` lettering face), headings, figure titles and stats are set at its own weight rather than in a synthesized bold. Embedded faces use `font-display: block`: they arrive with the report, so text is drawn once its face is decoded instead of first in a fallback face that then reflows. Each generated report carries all canonical font copyright/OFL notices in the inert `richview-font-notices` JSON element, preserving original text on decoding so offline exports retain their notices. Styles retain their distinct serif/sans/mono families and color choices. Report grid children can shrink to the viewport; wide Markdown tables scroll within their own boxes while retaining every column.

The type scale comes from the size tokens: body text uses `size-base`; the title `size-display`, the largest size; h1–h3 `size-h1`, `size-h2` and `size-h3` in the display face, and metric values `size-h1`; source notes, tables, captions and the byline `size-small`. The deck (frontmatter `description` or `subtitle`) is body size in `ink-soft`; it and block quotes are italic only when the text family embeds a true italic face, and the canonical styles set the deck upright in sleek and italic in scientific and brutalist. Reading text, metrics, callouts, the deck, source notes, lists, quotes, figures and tables stay within `measure`; a table, chart, image, frame or player takes the report's full width only with `wide: true`. Interactive charts place their actions, Table, Download PNG, CSV and Expand, as icons at the right end of the title row; each is named by its accessible name and by a tooltip on hover and keyboard focus, and on a figure too narrow for both the icons sit under the title. On desktop, Expand/Contract switches the entire figure between the text measure and report width. On phones, Expand opens the same live chart in a modal; Contract or Escape restores the report and focus. On a phone held upright, a chart whose x scale shows more of the data with more width (time, ordered categories or values) is laid out in landscape and turned a quarter wherever that draws it larger, without requiring orientation-lock support; flows, hierarchies, radial and area-filling charts, maps, networks, word clouds, funnels and 3D scenes stay upright. The modal follows physical rotation. Chart and table state survive without replaying the entrance; facets expand and export together. `strong`, `b` and table headers are set at weight 600.

Report blocks are at rest on first paint; a block that starts below the first screen fades up once, softly, as it comes into view (the style's `motion` block-entrance, duration, stagger and distance set it; `enabled: false`, reduced motion and print leave every block at rest). Interactive chart transitions run in Rust/WebAssembly with the timings in [MOTION.md](MOTION.md). Reduced motion is an unconditional floor. Published/native reports embed runtime assets when available; static SVG remains usable without them, with `_richview.interactive` recording runtime presence. Live browser previews can use a trusted cached runtime URL; downloaded exports embed their runtime. Source frontmatter cannot supply initializer code.

## Diagnostics and public interfaces

`validate` checks source syntax/schema; `check` additionally renders and inspects geometry. Each diagnostic is one record: `level`, `code`, `message` and `path`, with `fix`, `expected`, `retry`, `file`, `line`, `column`, `tag` and `details` where they apply (TYP-53). Lines and columns count from 1, the frontmatter included; when `file` is set they are in that data or style file, otherwise in the `.rmd` (TYP-56). With `--format json`, `init`, `validate`, `check`, `generate`, `publish` and the `report` commands return one object, as do the MCP render, validate, check and publish tools: `ok` is true when no diagnostic is an error, `status` is `issues` when any error or warning was found and `clean` otherwise, `output` is the file written or null, and `diagnostics` lists the records. These commands exit 0 when `ok`, 1 on any error (a draft with block errors is still written) and 2 on a usage error (TYP-54). The other commands return no such object. `learn` prints the guide as text, and `serve`, `server` and `mcp` run until stopped; a failure prints one message and exits 1, or 2 on a usage error. With `--format json`, `asset` and `image` print their own JSON, `{ok: false, code, error}` on a failure; they exit 2 on invalid input or configuration and 3 when a transfer or the Unsplash request fails, and `asset` also exits 4 when verification finds a mismatch (FEA-SF-50). The MCP learn, image and asset tools return what the matching command prints. On a server, publishing (`POST /api/reports`), `POST /api/validate`, `POST /api/check`, regenerating and every error body return the same object. `/health` answers `{ok: true}`, deleting, pinning and unpinning answer `ok` with the change, and the read, session and governance routes (people, tokens, access and the audit log) return their own records. Native checks distinguish emitted mark geometry from estimated font bounds. A clean native check is not proof of readability in every browser. See [chart diagnostics](self-validating-charts.md).

A diagnostic's `path` follows one grammar on every surface: dot-separated keys with `[n]` indices, rooted where the problem is. A block's findings start at its tag (`chart.encoding.y`, `table.columns[2].key`). Other findings start at one of these roots:
- `frontmatter`;
- `markdown`, the body's Markdown;
- `styles[n]`, a style file (`styles[1].tokens.accent`);
- `rows[n].field`, a resolved data row;
- `source`, the source text;
- `slug`, the publication slug;
- `options.<name>`, a host's render options;
- `report_folder`, the report folder itself;
- `output`, what a host writes.

A message names a file by its report-relative path and a folder by its name, never by where either sits on the host. A finding about a file other than the `.rmd` also names that file in `file` (TYP-53).

The core provides `parse`, `render`, `validate`, `validate_publish`, `check`, and owned-AST `render_document`/`check_document`. The site runtime exposes rendering, validation, checking and page boot through generated wasm-bindgen bindings; schema objects and the retired learn, style and chart compatibility exports are absent. The site loader initializes Wasm before synchronous calls. Parsed body placeholders belong to the parser: literal source comments remain inert, and the AST carries an optional collision-free marker prefix when needed. `richview learn` supplies grammar, chart, style, workflow and design context for coding agents.

Core source (`source.limit`) and resolved-dataset (`data.limit`) limits are 16 MiB; native request/file boundaries add their own limits. `MAX_REPORT_BYTES` caps accumulated rich-block and complete HTML at 64 MiB, including embedded media, fonts and runtime. Repeated resolved media is checked before copying payloads; exceeding the budget returns a source-centered `render.output-limit` error, with a block line when available, rather than truncating the report. Recoverable errors stay visible; publication rejects hard source errors. Generated HTML carries the actual `richview/<package-version>` generator, and `_richview` metadata records block/chart counts, offline/runtime presence and chart diagnostics.

Chart treatments use the builtins in the [style reference](#styles-and-motion). `hand-drawn` draws data in greys (story grey, context grey, ink, mid grey) with a paper-to-ink heatmap ramp; its pencil strokes never touch gridlines, axes or data marks, and its chart labels use the bundled Patrick Hand face with browser measurement after fonts load. Axis ink uses the safe `tokens.chart-ink` color token. Compact magnitude suffixes attach directly (`25k`); known `KBD` values above 1,000 display as `MBD`. Values at the author's chosen precision remain available in tooltips and tables.

An omitted chart `style.palette` inherits the report treatment. Explicit chart palettes retain priority. Reports without a style use the default in the [style reference](#styles-and-motion). Chart downloads include a PNG of the current view and a CSV of all original resolved observations at the author's precision.

### Diagnostic codes

Every code keeps its level; renaming or removing one is a format change (TYP-55).

<!-- registry:begin codes -->
| Code | Level | Line | Meaning | Fix |
| --- | --- | --- | --- | --- |
| `source.limit` | error |  | The source exceeds the 16 MiB limit. | Split the report or shrink its inline data below 16 MiB. |
| `frontmatter.yaml` | error | yes | The frontmatter is not YAML. | Rewrite the frontmatter between the --- lines as valid YAML. |
| `block.yaml` | error | yes | A rich block's YAML is not YAML. | Align sibling keys at the same indentation, indent nested fields consistently, use `key: value` mappings, and quote a text value that contains a colon followed by a space or starts with a symbol such as `%`. |
| `yaml.unsupported` | error | yes | The YAML uses a feature outside the RichView profile, such as anchors, tags or several documents. | Remove anchors, aliases, tags and extra documents; write plain YAML. |
| `yaml.duplicate-key` | error | yes | A mapping repeats a key. | Delete or rename the repeated key so each key appears once per mapping. |
| `frontmatter.type` | warning | yes | A frontmatter field has the wrong type and is dropped. | Give the field a value of the type the message names, or remove it. |
| `frontmatter.mapping` | error | yes | Closed frontmatter is not a mapping, so the title block cannot be read. | Write the frontmatter as a YAML mapping, such as `title: A report`, or remove its delimiters to render it as body text. |
| `frontmatter.unknown-key` | warning | yes | A frontmatter key the registry does not know; it stays in the metadata with no effect. | Remove the key, or rename it to the known key the message names. |
| `frontmatter.unclosed` | warning | yes | The frontmatter has no closing line, so it is rendered as Markdown. | Close the frontmatter with a `---` line after its last key. |
| `frontmatter.version` | warning | yes | The version is not semver-shaped and is dropped. | Use a quoted semantic version such as `version: "1.0.0"`; remove literal backslashes around the quotes. |
| `frontmatter.renamed` | error | yes | A frontmatter key the format retired; the fix names its replacement. | Replace the retired frontmatter key with the replacement the message names. |
| `frontmatter.date` | warning | yes | The date is not YYYY-MM-DD or RFC 3339; it renders as written. | Use an ISO date such as `date: 2026-10-30`, or an RFC 3339 date-time such as `date: 2026-10-30T08:00:00Z`. |
| `frontmatter.lang` | warning | yes | The language is not a supported locale; the page language is set but formatting stays English. | Set `lang` to `en`, `en-GB`, `cs`, `de` or `ja` to print numbers in that language's format; any other tag only names the page language. |
| `frontmatter.format` | error | yes | The format marker is not rmd/<major>, or names a newer major version than this renderer reads. | Set `format: rmd/<major>` to a major version this renderer reads. |
| `frontmatter.tabs` | warning | yes | The tabs setting is malformed, so the body renders without tabs. | Write `tabs` as a mapping whose `panels` give each tab a unique `id`, a `label` and the heading anchor it starts `from`. |
| `frontmatter.data` | error | yes | Report header data is no longer supported; data belongs in each block. | Remove the header data and give each block its own `data` source. |
| `publish.metadata` | error |  | Publishing needs a title, a description and a version. | Add a title, a description and a version to the frontmatter. |
| `block.type` | error | yes | A rich block is not a YAML mapping. | Make the block's YAML a mapping of keys to values. |
| `block.schema` | error | yes | A block field is missing or has the wrong type or value. | Give the field the value the message asks for; `richview learn` lists each block's fields and their types. |
| `block.field` | error | yes | A field reference names a column the block's data does not have. | Name a column that exists in the block's data, or add that column. |
| `block.renamed` | error | yes | A block key, tag or value the format retired; the fix names its replacement. | Replace the retired key, tag or value with the replacement the message names. |
| `block.unknown-key` | warning | yes | A block key the registry does not know; it is dropped before rendering. | Remove the key, or rename it to the known key the message names. |
| `schema.invalid` | error |  | A value fails the public schema parse. | Fix the value so it matches the public schema for that field. |
| `data.local` | error | yes | A block does not define exactly one local data source, or uses a header id or remote address. | Keep exactly one `data` source (`inline`, `csv`, `json`, `url`, `sql`, `metricflow` or `cache`) and remove the others. |
| `data.unresolved` | error | yes | A block's data file was not resolved before rendering. | Resolve the data file before rendering, or render through the CLI or host. |
| `data.limit` | error | yes | A resolved dataset exceeds 16 MiB. | Reduce the dataset below 16 MiB by filtering rows or dropping columns. |
| `data.csv` | error | yes | A CSV dataset cannot be read. | Repair the CSV so it parses: consistent columns, valid quoting, UTF-8. |
| `data.csv.header` | error | yes | A CSV dataset's headers are empty or repeat. | Give every CSV column a non-empty, unique header. |
| `data.inline` | error | yes | Inline rows are not flat row mappings. | Write inline data as a list of flat row mappings, one value per key. |
| `frontmatter.slug` | warning | yes | The slug is not valid, so publishing fails until it is fixed. | Write `slug` as 1 to 80 characters of a-z, 0-9 and single hyphens, with no hyphen at either end, such as `q3-sales`. |
| `publish.slug` | error |  | The slug is not valid for publishing. | Use a slug of lowercase letters, digits and hyphens. |
| `publish.version-exists` | error |  | This version is already published with different content. | Bump the version, or publish content identical to the existing one. |
| `publish.version-deleted` | error |  | This version was deleted and its number cannot be published again. | Publish under a new, higher version number. |
| `publish.version-missing` | error |  | The pinned version is not published for this report. | Pin a version that is published for this report, or drop the pin. |
| `publish.new-slug` | error |  | This report address is new and needs --new to be created. | Pass --new to create this report address, or correct the slug. |
| `publish.slug-derived` | warning |  | The slug was derived from the title because the frontmatter has none. | Add a `slug` to the frontmatter so the report keeps its address when its title changes. |
| `publish.quota` | error |  | Publishing would exceed the storage quota. | Delete old versions or shrink the report to fit the storage quota. |
| `data.json` | error | yes | A JSON dataset is not an array of row mappings. | Make the JSON file an array of flat row mappings. |
| `data.provenance` | info | yes | A connector query was run and its result stands in for the block's data. | Nothing to change; this records which connector query supplied the block's data, and the rows are reused on later renders until the query changes unless `cache: {refresh: true}` under `sql` or `metricflow` runs it every time. |
| `data.cache.stale` | warning | yes | A cached snapshot exceeded its freshness limit; the reader's data-source note marks it stale. | Run the query again to refresh the snapshot, or keep it; the reader's source note says it is stale. |
| `data.connector.unknown` | error | yes | A data block names a connector that is not registered. | Use a registered connector name, or register the connector first. |
| `data.connector.rejected` | error | yes | A connector refused the query. | Correct the query or its parameters to what the connector accepts. |
| `data.connector.failed` | error | yes | A connector failed while running the query. | Check the connector's access and the query, then run it again. |
| `data.connector.io` | error | yes | A connector hit a file error. | Check that the connector's file exists and is readable. |
| `data.cache.miss` | error | yes | A data block references a cached result that does not exist. | Run the query again to fill the cache, or remove the cached reference. |
| `image.credit-missing` | error | yes | An Unsplash photo has no credit, so the block stays source. | Add the photographer's credit to the Unsplash image. |
| `image.credit-invalid` | error | yes | An Unsplash photo's credit breaks the rules, so the block stays source. | Fix the Unsplash credit to follow the credit rules. |
| `image.credit-incomplete` | warning | yes | An optional image credit has no non-empty author, so no credit is shown. | Give `credit.author` the creator's name, or remove `credit`. |
| `image.credit-url` | warning | yes | A credit link is not an http(s) address, so the name is not linked. | Make the credit's `author_url` and `source_url` http or https addresses, or remove them. |
| `image.color-invalid` | warning | yes | An image color is not #rrggbb and is dropped. | Write `color` as a `#rrggbb` hex colour, such as `#1f6feb`, or remove it. |
| `security.media` | warning | yes | An audio or video source was blocked as unsafe. | Use an audio or video file in the report folder, or a `data:` URI of an audio or video type. |
| `security.embed` | error | yes | An embed was blocked because its URL is not https, carries credentials or names a host that is not allowed. | Use an https embed URL on an allowed host. |
| `export.video-poster` | warning | yes | A video has no poster, so the PDF prints a text card. | Add a `poster` image from the report folder so the PDF shows a frame of the video. |
| `assets.external-reference` | info | yes | The report needs an external file that is not embedded. | Generate the report with `richview generate`, which embeds the report folder's files, so it needs nothing beside it. |
| `assets.network-dependent` | info | yes | The report needs network access to load an asset. | Nothing to change if readers are online; offline, the photo or embed does not load and its placeholder shows. |
| `markdown.math` | warning |  | Math could not be rendered. | Correct the TeX the message quotes, or write the formula as plain text. |
| `security.html` | warning |  | Unsafe raw HTML was removed. | Write the content in Markdown or a rich block; raw HTML never renders. |
| `security.image` | warning |  | A Markdown image protocol was blocked. | Use an image file in the report folder or a `data:` image URI. |
| `security.link` | warning |  | A Markdown link protocol was blocked. | Link to an http(s), mailto or tel address, a # anchor or a file in the report folder. |
| `asset.mapping` | error |  | A remote asset reference is not a mapping. | Write the remote asset as a mapping with url, bytes, sha256 and type. |
| `asset.unknown-key` | warning |  | A remote asset reference has a key it does not take. | Remove the key; the message lists the keys a remote asset reference takes. |
| `asset.url` | error |  | A remote asset reference has a missing or unusable url. | Set a valid https `url` on the remote asset. |
| `asset.presigned` | error |  | A remote asset url carries a signing parameter. | Use a stable public URL without signing parameters. |
| `asset.bytes` | error |  | A remote asset reference lacks a valid byte size. | Set `bytes` to the asset's exact size as a positive integer. |
| `asset.sha256` | error |  | A remote asset reference lacks a valid SHA-256. | Set `sha256` to the asset's 64-character hex digest. |
| `asset.type` | error |  | A remote asset reference has a media type its use does not allow. | Set a media type that this use of the asset allows. |
| `asset.visibility` | error |  | A remote asset reference is neither public nor private. | Set `visibility` to public or private. |
| `asset.duration` | error |  | A remote asset's duration is invalid or does not apply. | Set a valid positive `duration`, or remove it where it does not apply. |
| `asset.dimensions` | error |  | A remote asset's width or height is invalid or does not apply. | Set valid positive `width` and `height`, or remove them where they do not apply. |
| `style.unresolved` | error |  | A styles file is not in the report folder. | Put the styles file inside the report folder and name it correctly. |
| `style.renamed` | error |  | A style names a builtin that was renamed. | Replace the style name with the one the message names. |
| `style.print-dark` | warning |  | The style is still dark after the print swap, so the page prints with Air's surfaces and inks. | Give the style light surfaces and dark inks to print in its own colours; otherwise paper uses Air's. |
| `style.unknown` | error |  | A styles entry names no built-in style. | Use a built-in style name, or a styles file in the report folder. |
| `style.type` | warning |  | A style file is not a YAML mapping. | Write the style file as a YAML mapping of style keys to values. |
| `style.retired` | warning |  | A style leaf is retired and ignored. | Remove the retired leaf; the message says why it no longer applies. |
| `style.schema` | warning |  | A style leaf is invalid. | Give the leaf a value of the type the message names, or remove it. |
| `style.company-adjusted` | info |  | The brand colour was adjusted to meet the contrast floor. | Keep the adjusted colour for legibility or choose a brand colour with enough contrast. |
| `style.company-secondary` | info |  | The secondary brand colour was too similar for the chart palette. | Choose a second colour that is distinguishable from the first, or keep the derived palette. |
| `style.company-palette` | warning |  | A distinct brand palette could not be derived. | Choose another brand colour or supply a checked explicit palette. |
| `style.company-color` | warning |  | A company colour is translucent or unresolved. | Use an opaque concrete colour such as #0B6E69. |
| `style.logo-path` | error |  | A company logo path is not report-relative. | Use a file path inside the report folder. |
| `style.logo-missing` | error |  | A company logo is not available. | Put the logo file inside the report folder. |
| `style.logo-format` | error |  | A company logo is not a valid raster image. | Use a valid still PNG, JPEG or WebP logo. |
| `style.logo-size` | error |  | A company logo exceeds the file size limit. | Use a logo no larger than 512 KiB. |
| `style.logo-dimensions` | error |  | A company logo exceeds the image dimension limit. | Use a logo no larger than 1024 pixels on either side. |
| `style.logo-animated` | error |  | A company logo is animated. | Use a still PNG, JPEG or WebP logo. |
| `style.logo-name` | warning |  | A company logo has no name and is decorative. | Set company.name to the company name, or leave the mark decorative. |
| `style.properties` | error |  | Custom style properties are invalid. | Fix the custom style properties to the documented names and values. |
| `style.fonts-unavailable` | warning |  | The host has no bundled fonts, so text falls back to system fonts. | Generate the report with the CLI, which embeds the bundled fonts. |
| `style.unknown-key` | warning |  | A style leaf is not in the style vocabulary and is skipped. | Remove the leaf, or rename it to the style key the message names. |
| `style.experimental` | warning |  | A style leaf is experimental: it can break the layout in ways nobody expects. | Remove the leaf, or keep it only after checking the report at phone and desktop widths. |
| `style.internal` | warning |  | A style leaf fixes the page's mechanics and is ignored. | Remove the leaf; the page sets it itself. |
| `style.contrast` | warning |  | A style colour is below its contrast floor on a surface. | Darken or lighten the colour until it reaches the floor the message names on that surface. |
| `style.type-scale` | warning |  | Two neighbouring type sizes are closer than the minimum ratio. | Move the two sizes apart until each step is at least the ratio the message names. |
| `style.weights` | warning |  | The style embeds more than three font weights. | Use at most three font weights across the style. |
| `style.font-missing` | warning |  | A declared font family has no bundled face and no file in the report folder. | Add the font's files to the report folder, or choose a bundled family. |
| `style.glyph-missing` | warning |  | Characters of the report fall outside the faces the style embeds. | Choose a face that covers these characters, or add one to the report folder that does. |
| `options.style` | error |  | The render options' styles are not an ordered array of style references. | Pass the render options' styles as an ordered array of style references. |
| `options.width` | error |  | A chart width is non-finite or outside 240 to 4096. | Set every chart width to a finite number from 240 through 4096. |
| `render.output.invalid` | error |  | The render output option is invalid. | Set the output option to a value the renderer accepts. |
| `render.page.invalid` | error |  | The render page option is invalid. | Set the page option to a value the renderer accepts. |
| `render.output-limit` | error |  | The generated report exceeds the 64 MiB HTML limit. | Shrink the report's data, images or charts below the 64 MiB HTML limit. |
| `assets.runtime-external` | info |  | The preview loads the chart runtime from outside the report. | Nothing to change in a preview; `richview generate` embeds the runtime for offline readers. |
| `security.id` | warning | yes | A heading id or footnote label reserved for the report runtime was dropped or renamed. | Choose an id or label that does not start with rv, richview, tab- or panel-. |
| `security.class` | warning | yes | Heading classes and attributes from source were dropped. | Remove the classes and attributes from the heading, keeping only `{#id}`. |
| `image.alt-missing` | warning |  | The header image has no alt text, so it is treated as decorative. | Describe the image in `alt`, or write `alt: ""` to mark it decorative. |
| `editor.service` | error |  | The editor service could not answer a request. | Retry the request; restart the editor service if it keeps failing. |
| `block.nested` | error | yes | A rich block sits inside a list, quote or note. | Move the rich block out of the list, quote or note to the top level. |
| `block.unknown-tag` | warning | yes | A fence names no rich tag; a close match is suggested. | Change the fence tag to the rich tag the message suggests, or keep the fence as code. |
| `callout.title` | warning | yes | A callout has no title, so colour alone carries the finding. | Add a `title` that names the finding. |
| `block.source` | warning |  | A chart or table has no source note directly after it. | Add a source note directly after the block, such as *Source: data/sales.csv.*, and name synthetic data as such. |
| `table.title` | info |  | A table has no title or accessible name. | Add a `title` that names the table's subject. |
| `image.context` | info |  | The header image has neither a caption nor a credit. | Add a `caption` or a `credit` to the header image. |
| `markdown.heading-id` | warning | yes | An explicit heading id is not named by a tab panel and is ignored. | Name the anchor in a tab panel's `from`, or remove the `{#id}`. |
| `markdown.table-header` | warning | yes | A Markdown table repeats a column header, making its columns ambiguous. | Give each table column a distinct header. |
| `table.markdown` | info |  | An inline table cell contains Markdown syntax and is shown as written. | Write the cell as plain text, or use a Markdown table in the prose when cells need emphasis or links. |
| `media.captions` | info |  | A video has no captions file. | Add `captions` with a WebVTT file from the report folder. |
| `media.transcript` | info |  | An audio or video block has no transcript file. | Add `transcript` with a text or Markdown file from the report folder. |
| `markdown.footnote` | warning | yes | A footnote reference has no definition and shows as text. | Add the footnote's definition, `[^label]: text`, or remove the reference. |
| `markdown.heading-depth` | warning | yes | A sixth-level heading cannot move below the page title and stays at level 6. | Use at most five heading levels, with # for the top level. |
| `markdown.quote` | warning | yes | The pull-quote marker is not supported and shows as text. | Write the quotation as a block quote and end it with — Name. |
| `markdown.wikilink` | warning | yes | A wikilink is not supported and its label shows as text. | Write a Markdown link, `[text](target)`, in place of the wikilink. |
| `render.failed` | error |  | The report could not be rendered. | Fix the errors reported with this one and render again. |
| `style.sink` | error |  | A style value holds characters the CSS output refuses. | Remove quotes, braces, semicolons and other CSS-breaking characters from the style value. |
| `table.total` | warning | yes | A table total skips columns it cannot sum. | List the columns to sum, such as `total: [accounts, revenue]`. |
| `security.runtime-url` | warning |  | The chart runtime address is not allowed. | Use the embedded runtime, or a root-relative path or an https base without credentials, query or fragment. |
| `tabs.anchor-missing` | warning |  | A tab panel's anchor is not in the body, so the body renders without tabs. | Give the body a heading with the panel's anchor, such as `## Costs {#costs}`, or correct the panel's `from`. |
| `tabs.order` | warning |  | Tab anchors are not distinct and in document order, so the body renders without tabs. | List the panels in the order their headings appear, each starting from a different anchor. |
| `chart.invalid` | error | yes | A chart block could not be read. | Fix the chart block's fields to match the chart schema. |
| `chart.failed` | error | yes | A chart failed to draw. | Check the chart's data and encodings, then fix the errors reported with it. |
| `chart.states-unchecked` | warning | yes | Only the first reader states of a chart were checked. | Nothing to change if the checked states are typical; fewer control values let every state be checked. |
| `chart.aggregation` | info | yes | The chart aggregates rows before drawing. | Nothing to change if means and ranges are intended; choose `style.points: all` to draw every observation, accepting its rendering cost. |
| `chart.decimation` | info | yes | The chart draws representative positions for several observations and reports the kept and total counts. | Nothing to change if reduction is intended; choose `style.points: all` to draw every observation, accepting its rendering cost. |
| `chart.ambiguous-bar-slot` | error | yes | Several numeric rows share a bar category and series. | Aggregate the data so each category and series has one row. |
| `chart.ambiguous-missing-cell` | error | yes | Missing heatmap observations repeat an existing coordinate. | Remove the duplicate missing-value rows so each x and y appears once. |
| `chart.ambiguous-numeric-cell` | error | yes | Several numeric rows share a heatmap coordinate. | Aggregate the data so each x and y coordinate has one row. |
| `chart.binning` | info | yes | The histogram's bins and measure, with the observations they hold. | Nothing to change; set `binning.count` or `binning.edges` to choose other bins. |
| `chart.boxplot-summary` | info | yes | How the box plot was summarised: quartiles, whiskers and outliers. | Nothing to change; this records how the boxes and whiskers summarise the data. |
| `chart.clipped-interval` | warning | yes | Axis bounds clip supplied intervals or estimates. | Widen `axes.y.min` and `axes.y.max` to cover every interval, or remove them. |
| `chart.clipping` | warning | yes | A label exceeds the chart's viewport. | Shorten the text or give the chart more room. |
| `chart.collapsed-interval` | warning | yes | An interval has equal bounds and collapses to a point. | Check the row's bounds; an interval narrower than the chart can show is drawn as a point. |
| `chart.contrast` | warning | yes | Chart text falls below 4.5:1 contrast on the chart surface. | Darken or lighten the ink token the message names. |
| `chart.empty-data` | error | yes | The chart has no rows to draw. | Give the chart at least one data row, or remove the chart. |
| `chart.empty-region` | warning | yes | The plot draws nothing. | Check that the data has values in the encoded fields and that not every series is hidden. |
| `chart.missing-observations` | info | yes | Some rows have no value and are drawn as gaps. | Nothing to change if the values are missing; fill them in the data to draw them. |
| `chart.excluded-observations` | warning | yes | Some observations were left out of the chart. | Give every row the values the message names, or remove the rows. |
| `chart.histogram` | error | yes | The histogram options or data are invalid. | Use a numeric field with valid bin options and non-empty data. |
| `chart.invalid-bubble-size` | error | yes | Bubble sizes are negative or not numbers. | Make every bubble size a non-negative number. |
| `chart.invalid-color-values` | error | yes | Heatmap colour values are neither numbers nor recognised missing values. | Make the colour field numeric, or leave missing values empty. |
| `chart.label-collision` | warning | yes | Two labels overlap. | Shorten the labels or widen the chart. |
| `chart.label-dropped` | info | yes | A label was left out for lack of room. | Nothing to change for a value or axis label, whose value stays in the readout, the Table view and the CSV; shorten or move an annotation's text, and read `details.width` for the chart width it was left out at, often the phone layout. |
| `chart.label-thinned` | info | yes | Only some category names are shown. | Nothing to change; shorter category names or a wider chart show more of them. |
| `chart.label-unfit` | warning | yes | A label does not fit at this width. | Shorten the names, or give the chart more room. |
| `chart.missing-bubble-size` | warning | yes | Some observations have no size, so their bubbles are omitted. | Give the row a size value, or remove the row. |
| `chart.missing-color-values` | error | yes | The heatmap's colour field has no numeric values. | Give the heatmap's colour field at least one numeric value. |
| `chart.negative-pie-value` | error | yes | A pie slice has a negative value. | Make every pie value zero or positive. |
| `chart.non-numeric` | error | yes | A value that must be a number is not. | Make the field a number: remove units, text and thousands separators. |
| `chart.limit-exceeded` | error | yes | The chart has more of something than its type takes; nothing is dropped, so it is not drawn. | Filter or aggregate the rows to fit the limit named in the message, or choose a chart type that takes more. |
| `chart.palette-rule` | warning | yes | The palette fails its rule on this chart, so patterns tell series apart. | Pick a palette whose colours differ more, or keep the patterns. |
| `chart.points-overlap` | info | yes | Points overlap because their strips are too narrow to spread them. | Widen the chart, filter the data, or facet it. |
| `chart.series-differentiation` | warning | yes | More than eight series reuse colour slots. | Show fewer series, or split the chart into facets with `encoding.facet`. |
| `chart.source-error` | error | yes | A source value cannot be represented. | Replace the unreadable source value with a plain number or text. |
| `chart.style-invalid` | warning | yes | A chart style setting has a value the type cannot use, so its default applies. | Give the style setting a value this chart type uses, or remove it. |
| `chart.too-few-observations` | warning | yes | A group has too few distinct values for the chart to draw it. | Give the group at least two different values, or leave it out. |
| `chart.unlocated-color-values` | error | yes | Heatmap colour values lack x or y coordinates. | Give every heatmap row both an x and a y value. |
| `chart.unresolved-annotation` | warning | yes | An annotation's coordinates do not resolve against the chart. | Place the annotation at values inside the chart's data range; a range or band needs `from` and `to` at two different positions. |
| `chart.unresolved-highlight` | warning | yes | A highlighted value matches no row. | Match the highlighted value to one in the data, checking its spelling. |
| `chart.value-overflow` | error | yes | A total exceeds the finite numeric range. | Use smaller values or rescale the data so totals stay finite. |
| `report.root.missing` | error |  | No dedicated report folder, or its source is missing. | Put the report in its own folder with a single .rmd source. |
| `report.root.invalid` | error |  | The report root or source name is invalid. | Use a valid report folder and a valid .rmd source name. |
| `report.root.escape` | error |  | A report path escapes the report folder or is ambiguous. | Keep every report path inside the report folder, without .. or links. |
| `report.root.shared` | error |  | The report folder holds duplicate paths or several .rmd sources. | Remove duplicate paths and keep one .rmd source per report folder. |
| `report.root.unresolved` | error |  | The report folder cannot be inspected. | Check that the report folder exists and is readable. |
| `scrolly.state` | warning | yes | A scrollytelling step sets a chart control or option the chart does not have. | Use a control id and option value from the chart's `controls`. |
| `chart.print-height` | warning | yes | A chart is taller on paper than the share of the page it may take. | Show fewer rows or categories, or give the chart a wider `style.ratio`, such as `16:9`, so it prints shorter. |
| `report.asset.remote` | error | yes | An image, audio, video, poster, header image or Markdown image names a remote address. | Download the file into the report folder and reference it by relative path. |
| `report.asset.escape` | error | yes | An asset path escapes the report folder. | Move the asset into the report folder and reference it by relative path. |
| `report.asset.missing` | error |  | An asset is missing from the report folder. | Add the missing asset to the report folder or correct its path. |
| `report.output.escape` | error |  | The output path escapes the Rendered folder. | Write output inside the Rendered folder, without .. in the path. |
| `report.output.invalid` | error |  | The output file name is not allowed. | Use an allowed output file name with a plain relative path. |
| `report.output.collision` | error |  | An authored asset collides with a generated output. | Rename the authored asset so it does not match a generated output. |
| `report.output.runtime` | error |  | The interactive chart runtime is missing, so the report cannot be exported. | Build the chart runtime first, or export without interactive charts. |
| `export.browser-missing` | error |  | No Chromium-based browser was found for PDF export. | Install a Chromium-based browser or point the export at one. |
| `export.platform` | error |  | PDF export is not supported on this platform. | Export the PDF on a supported platform, or use the HTML report. |
| `export.browser-failed` | error |  | The browser failed while printing. | Retry the export; check that the browser starts and can print. |
| `export.settle-timeout` | error |  | The report did not settle before the timeout. | Retry the export, or reduce the report's charts and images. |
| `export.image-failed` | error |  | Images could not be decoded for printing. | Replace the undecodable images with valid PNG, JPEG or SVG files. |
| `export.browser-version` | warning |  | The browser is older than the supported Chrome version. | Use a current Chromium, Google Chrome or Microsoft Edge. |
| `export.photo-unavailable` | warning |  | A photo could not be loaded while printing. | Check that the photo's address loads, then export again. |
| `export.font-fallback` | warning |  | Glyphs were drawn in a system font instead of the embedded ones. | Choose a style whose faces cover these characters, then export again. |
| `export.pdf-untagged` | warning |  | The PDF has no tags or outline. | Export with a current Chromium, Google Chrome or Microsoft Edge, which writes PDF tags and an outline. |
| `export.network-blocked` | warning |  | The report requested an address while printing that PDF output does not load. | Put the file in the report folder so the report embeds it, then export again. |
| `export.local-link` | info |  | A link to a report file prints as text. | Nothing to change; link to a public address if PDF readers should open the file. |
| `export.font-check` | info |  | The font check did not complete. | Export again; until the check completes, the PDF may depend on this machine's fonts. |
| `export.browser-sandbox` | info |  | Chrome printed with its sandbox off because RichView runs as root. | Run RichView as an ordinary user so Chrome prints inside its sandbox. |
| `resource.resolve` | error |  | A report resource could not be resolved. | Check that the resource exists in the report folder and is readable. |
| `authoring.key` | warning |  | A key is not used by the block. | Remove the key, or move it to where the block reads it. |
| `authoring.field` | error |  | A key names a data field that does not exist. | Use a field name that exists in the data. |
<!-- registry:end -->
