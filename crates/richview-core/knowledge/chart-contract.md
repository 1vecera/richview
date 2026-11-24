A chart block takes a `type`, a `title`, `data` and an `encoding` that maps data columns to the channels the type reads:

```chart
type: line
title: Brno recovered within a month of reopening
subtitle: Monthly revenue by store, CZK
data: { csv: data/revenue.csv }
encoding: { x: month, y: revenue, color: store }
axes:
  y: { label: Revenue, unit: CZK, format: number_compact }
annotations:
  - { kind: line, at: { x: "2026-07" }, text: Brno closed }
style: { curve: linear }
```

*Source: Store revenue export, January 2025 to October 2026.*

The title states the claim and is required; subtitle is the interpretation.

### Chart types

Each type with its channels (* required) and what it draws:

@CHARTS@

Channels outside the type's list and column names the data lacks are errors (block.schema, block.field).

- **Category and value.** On a bar, `x` is the category and `y` the value, whichever way the bars lie. `style.orientation` is `auto` by default, which may lay the bars down to fit the category names; set `vertical` or `horizontal` to decide.
- **Series.** `encoding.color` is the series channel, and `stack: true` stacks the series of a bar or area chart.
- **Order.** Categories are drawn in the order of the data rows. To sort a chart, sort its data.
- **Lines are smoothed by default** (`style.curve: monotone`). For counts, or a series with a sharp drop or spike, use `style: { curve: linear }`, which draws straight segments.
- **Detail.** `encoding.detail` lists extra columns for readouts, Table views and downloads; columns no block references never reach the report.
- **Size.** Charts are responsive: set style.ratio as a positive number or W:H from 0.5 to 6, never a fixed pixel height, and wide: true takes the full report width.

### Data

data takes exactly one source: inline rows, a CSV or JSON file in the report folder, or a connector query such as SQL. The grammar section's Data and SQL parts give the rules; the keys are below.

### Chart keys

Every key a chart reads (* required), nested keys under their parent:

@CHARTFIELDS@

### Annotations

Annotation kinds are point, range, line, band and text; anchors use at/from/to, each a mapping from an encoding channel to a position (at: {x: 2026-05, y: 40}, never a bare value), the words go in text (there is no label key), and rotate applies to text only.

- **Mark one moment** with a line: `{ kind: line, at: { x: "2026-07" }, text: Store closed }`. A date names its first instant, where that month's point of monthly data sits.
- **Mark a span** with a range: `from` and `to` must be two different positions. Equal ones match nothing and warn `chart.unresolved-annotation`. To shade July on monthly data: `{ kind: range, from: { x: "2026-07-01" }, to: { x: "2026-08-01" }, text: Closed }`.
- **Check that the text was drawn.** An annotation's text is placed only where it is readable. When it finds no place, the mark is still drawn, the words are left out, and `check` lists them under the info `chart.label-dropped`. Then shorten the text, or anchor it elsewhere: a `point` takes an `x` and a `y` in data units and sets its text beside that spot.

### Labels and what check reports about them

RichView places value labels, line-end labels and annotation texts only where they are readable, and leaves the others out. `check` reports each omission as an info: `chart.label-dropped` or `chart.label-thinned`.

- For value, line-end and axis labels these infos need no change. The values stay in the chart's readout, its Table view and its CSV download. No key turns these labels on or off.
- Many omissions are in the phone layout, which fits fewer labels. With `--format json` each finding's `details.width` gives the chart width it was found at; the entries under `charts` describe the desktop layout only.
- When a label that the story needs is dropped, shorten it, show fewer categories or series, or state the number in the text or a table. `wide: true` does not change the phone layout.
- `richview check report.rmd --chart-width 390` measures charts at a width of your choice instead.

### Per-type notes

- **Scatter.** Names are sparse by default (isolated points first, at most 16, no leader lines); the top-level highlight: {by: field, include: [values], priority: [values]} always attempts those names first, case-insensitively, with a leader only when a highlight cannot sit beside its point.
- **Highlight and mute.** On line, area, bar and lollipop charts the same `highlight` makes the matching series or bars the subject: `mute` (default true; false on scatter) draws the rest in the style's quiet ink, without automatic names or values. Write the claim in the title; highlight what it names.
- **Histogram.** Uses encoding.x alone for numeric observations and optional binning: {count, extent, measure: count|proportion|density}; explicit edges replace count/extent. Groups stack within shared intervals and original rows remain in Table/CSV.
- **Boxplot.** Uses numeric encoding.y and optional categorical encoding.x, color and facets; R-7 quartiles, observed whiskers inside inclusive 1.5-IQR fences and original potential-outlier records. Its orientation may be auto, vertical or horizontal; axes.x/y retain category/value meanings.
- **Interval.** Requires categorical encoding.x and authored numeric encoding.lower/encoding.upper; optional encoding.y supplies an estimate. The renderer does not infer confidence or a distribution. Duplicate category/group pairs in a panel and reversed bounds are errors.
- **Waterfall.** `encoding.measure` names a column whose values are `delta`, `start` or `total`; any other value is an error. A `delta` row, or one whose cell is empty, is a step added to the running level. A `start` row sets the level the steps run from, and a `total` row states the level reached, which must equal the steps added up (`chart.waterfall-total` otherwise). Without the column every row is a step.
- **Calendar axes.** axes.x.locale supports en-US (default), en-GB, cs-CZ, de-DE and ja-JP compact UTC calendar labels.

### Dense data

Charts draw every valid point by default up to their declared ceiling: @POINTLIMITS@. Above its automatic ceiling, a line or area uses M4; scatter keeps screen-space representatives per series and the extremes. Each discloses its density and kept/total counts. Explicit style.points: all bypasses that ceiling and draws every point; the author or embedding host accepts its cost. For a line that retains every observation without smoothing, use style: {points: all, curve: linear}. Choosing a reduced point mode never changes the author-selected precision of readouts, Table or CSV, and each actual reduction reports its kept and total counts.

Rust owns chart layout and browser interactions. Native/browser hosts resolve local CSV/JSON before entering core.
