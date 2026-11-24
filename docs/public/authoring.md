---
title: Rich Markdown
description: Plain Markdown for the argument, validated YAML for the evidence.
styles: [scientific]
---

A `.rmd` file is CommonMark with YAML frontmatter and tagged YAML fences. Ordinary Markdown tools can read it: a rich block appears as code there. RichView turns a supported block into a chart, table, metric or media element. Unknown fences stay code; invalid rich fences stay visible with diagnostics.

## Write the smallest complete source

Save this as `report.rmd` in its own folder:

````rmd
---
title: Orders rose each month
description: Three fictional observations for a tutorial.
version: 0.0.1
styles: [scientific]
---

## The evidence

```chart
type: bar
title: Orders increased from 120 to 180
data:
  inline:
    - { month: Jul, orders: 120 }
    - { month: Aug, orders: 150 }
    - { month: Sep, orders: 180 }
encoding: { x: month, y: orders }
axes:
  y: { label: Orders }
```

*Source: fictional tutorial observations.*
````

Keep the title about the evidence. Add an emphasized Source paragraph immediately after each chart or data table. Put important requested calculations in a metric, table row or labelled chart; a source series alone does not state a calculation.

## Numbers: what readers see

Readers never get more precision than you chose, and the default is three significant digits. The same rules apply to metrics, table columns and chart axes.

| You write | Readers see |
| --- | --- |
| `value: 13341.31` | 13.3 thousand |
| `value: 13341.31` with `format: number` | 13,300 |
| the same with `digits: 6` | 13,341.3 |
| `value: "13,341.31 EUR"` | 13,341.31 EUR |
| `value: 0.368` with `format: percent` | 36.8% |
| `delta: 0.12` on a metric | +12.0% |

`digits` counts significant digits, from 2 to 6, not decimal places. A percent format and a metric's `delta` take fractions, so `delta: 12` prints +1200%. Text prints as written: put an exact amount in quotes.

## Frontmatter and identity

Local drafts can omit metadata. Publishing requires a nonempty title, description and a SemVer-shaped version such as the text `0.0.1`. A slug can be supplied explicitly or derived from the title. Unknown metadata is preserved with a warning; malformed known values produce diagnostics. The [generated metadata reference](https://richview.ai/docs/reference#document-metadata) defines every field and its type.

Frontmatter and rich blocks share a restricted YAML 1.2 profile. Duplicate keys, aliases, merge keys, tags and multiple YAML documents fail. Numbers must be finite. Values such as `yes`, `on` and unquoted dates are text. Quote metadata when its type might otherwise be ambiguous. See the [YAML profile](https://richview.ai/docs/reference#yaml-profile).

## A bounded report folder

```text
orders/
  report.rmd
  report.rstyle.yaml
  data/orders.csv
  media/photo.jpg
  Rendered/report.html
```

Keep exactly one `.rmd` anywhere under the report folder. Paths are relative to the source and cannot escape through `..` or symlinks. Reference optional styles explicitly. Native generation embeds local media and copies linked local documents into `Rendered/`; an output name stays inside that directory. See [data and connectors](https://richview.ai/docs/data) and [export](https://richview.ai/docs/export).

## Keep Markdown useful

Use ordinary headings, lists, links, block quotations, GFM tables, footnotes, definition lists and strikethrough. Task checkboxes display their saved state. Inline and display LaTeX become native MathML, with diagnostics for unsupported expressions. Code remains literal. Source notes get a consistent treatment without custom HTML.

````rmd
A fraction: $p = x / n$.

A definition:

Retention
: The share of the eligible cohort still active.

The cohort excludes missing observations.[^cohort]

[^cohort]: State the exclusion and its reason beside the analysis.
````

Use ordinary links for other reports. Raw HTML is stripped with diagnostics; source cannot execute CSS or JavaScript. Unsafe link and media protocols are blocked. [Blocks and media](https://richview.ai/docs/blocks) provides the supported building blocks.

## Contents and reading progress

A report with three or more `##` sections lists them in a table of contents: beside the text on a wide screen, where it follows the scroll and marks the section being read, and folded under the header on a phone. A thin bar along the bottom of the window shows how far the reader has scrolled, with a small gap where each section starts. Both are on by default and stay off paper. Set `toc: false` or `progress: false` in the frontmatter to turn either off; a tabbed report has no table of contents, because its tabs do that job.

## Split a long report into tabs

Name explicit heading anchors in the panel configuration:

````rmd
---
title: Two views of one report
tabs:
  default: overview
  export: all
  panels:
    - { id: overview, label: Overview, from: overview }
    - { id: detail, label: Detail, from: detail }
---

## Overview {#overview}

The main finding.

## Detail {#detail}

The supporting explanation.
````

Readers can use arrow keys, Home and End or a `#tab=detail` deep link. Charts retain their state across tabs. Without the runtime, every panel remains readable. Export includes all panels by default; the supported active-panel option is defined in the [tabs contract](https://richview.ai/docs/reference#tabs).

Use the [full format reference](https://richview.ai/docs/reference) for every block, chart type, field and diagnostic. Its tables are generated from the schema registry that validates the source. To present a report, see the [slides guide](https://richview.ai/docs/slides).
