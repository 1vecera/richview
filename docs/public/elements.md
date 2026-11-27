---
title: The elements gallery
description: Charts, flow charts, tables, expands, code, columns, type, pictures and players, each live on one page with a way in.
styles: [scientific]
---

A report is made of a few kinds of element. Each one is live below, and each has a way in to its own examples and source. The [block tour source and local files](https://richview.ai/docs/gallery/blocks/) let you reuse the complete bundle.

{{elements}}

## Charts

A chart block names its type, its data and which field goes where. RichView draws it, labels it and lets the reader inspect every value. The [chart gallery](https://richview.ai/docs/charts) shows every chart type live, after famous charts you can explore.

{{example:streamgraph}}

*Source: [the Streamgraph example](https://richview.ai/docs/charts/streamgraph), illustrative data.*

## Flow charts

A flow chart shows how a process runs: what happens in which order, where it decides and what runs as one job. You write the cards and the arrows between them; RichView measures every card, places them and routes the lines. The [flow chart page](https://richview.ai/docs/charts/flow) has the source, the fields and every style.

{{example:flow}}

*Source: [the Flow chart example](https://richview.ai/docs/charts/flow), illustrative data.*

## Tables

A table keeps exact values readable, with aligned numbers and named columns. `heat` shades a numeric column by value in the style's colours, a reader sorts by any heading, and a filter box above the rows keeps the ones that hold the typed text. A pivot table sums one measure for every pair of a row field and a column field, with totals. [Blocks & media](https://richview.ai/docs/blocks) documents the table block; the [pivot table page](https://richview.ai/docs/charts/pivot) has the pivot's source, fields and every style.

A numeric column with a declared scale uses one number of decimal places. Set `decimals` on the column to choose how many; a `year` column ignores it and prints whole years. Otherwise a typed source supplies the scale: a price of `123.45` in a `DECIMAL(12,2)` column keeps both cents digits. Set `digits` to round more coarsely. A column with a declared scale shows every number as a plain number at that scale, with separators and the unit, however large: `$12,345.67`, `$987.50` and `$1,234,567.89` in one column. With neither, each cell keeps the precision of its own value, three significant digits by default, so 9.26, 9 and 8.98 read as they are. `format: number_compact` with `decimals` asks for the words outright. A number the scale would print as zero keeps its own digits, so nothing nonzero reads as zero. In this example, `decimals: 2` makes the price written as `9` read as `$9.00`.

```table
title: Price per kilogram
data:
  inline:
    - { variety: Gala, price: 9.26 }
    - { variety: Fuji, price: 9 }
    - { variety: Braeburn, price: 8.98 }
columns:
  - { key: variety, label: Apple }
  - { key: price, label: Price per kg, unit: $, decimals: 2, heat: true }
filter: true
```

{{example:pivot}}

*Source: [the Pivot table example](https://richview.ai/docs/charts/pivot), illustrative data. Apple prices are illustrative.*

## Expands

An expand keeps a long part of the report one tap away. The reader sees its title, opens it when they want the detail, and the page stays short for everyone else. On paper and in a PDF every expand prints open, so nothing is lost to a fold.

```expand
title: How the sample was drawn
body: |
  We drew 1,200 orders at random from the quarter's 48,316, by region, so that each region has at least 150. Orders refunded within a day are left out, since their amounts are reversed.

  The margin of error on a regional share is at most 4 points at 95% confidence.
```

```expand
title: What the columns mean
open: true
body: |
  - **region**: the sales region of the store that took the order.
  - **amount**: the order's value in euros, after discounts, before tax.
  - **channel**: `store`, `web` or `phone`.
```

A whole section can fold the same way, charts, tables and all: name its heading's anchor under `collapsed_sections` in the frontmatter. The heading becomes the title the reader taps, and the section runs to the next heading of its level.

````rmd
---
title: Quarterly orders
collapsed_sections: [sources-cited]
---

## Sources cited

Every source, with the table of what each one gave.
````

## Code blocks

A code block shows code as text and can name its file, number its lines and mark the lines that matter. SQL, YAML, Rich Markdown and shell use the style's own inks to distinguish values from surrounding code, on screen and in print. Other languages appear as plain text.

```code
title: orders_by_region.sql
lang: sql
lines: true
mark: [4, 5]
text: |
  -- Orders in the West region
  SELECT id, amount, NULL AS note
  FROM orders
  WHERE region = 'West'
    AND amount >= 100
    AND refunded = FALSE;
```

```code
title: orders.yaml
lang: yaml
text: |
  region: "West"
  minimum_amount: 100
  include_refunds: false
  report_date: 2026-11-23
```

````code
title: report.rmd
lang: rmd
text: |
  ---
  title: "West region orders"
  date: 2026-11-23
  ---

  ```expand
  title: "100 orders"
  open: true
  body: See the source data.
  ```
````

An ordinary fence, such as ` ```sql `, stays the short way to write code and is highlighted the same way.

## Columns

Columns put two to four blocks side by side: two charts, a chart beside the sentence that explains it, a row of numbers. Each column holds prose and blocks written as anywhere else, between a `::: column` line and a closing `:::`. On a phone the columns stack in their order, and on paper they stay side by side while each column has room. [The format reference](https://richview.ai/docs/reference#columns) has the options.

The two **columns** below compare total and mobile orders for the same periods on the same scale.

::: columns

::: column
```chart
type: bar
title: Total orders
data:
  inline:
    - {period: January, orders: 1810}
    - {period: February, orders: 1795}
    - {period: March, orders: 610}
    - {period: April, orders: 590}
    - {period: Launch week, orders: 1240}
encoding: {x: period, y: orders}
axes: {y: {min: 0, max: 2000}}
```

*Source: illustrative data.*
:::

::: column
```chart
type: bar
title: Mobile orders
data:
  inline:
    - {period: January, orders: 840}
    - {period: February, orders: 865}
    - {period: March, orders: 290}
    - {period: April, orders: 280}
    - {period: Launch week, orders: 710}
encoding: {x: period, y: orders}
axes: {y: {min: 0, max: 2000}}
```

*Source: illustrative data.*
:::

:::

[See two charts in columns and a method note in an expand](https://richview.ai/docs/gallery/same-numbers/Rendered/report#a-second-distribution) in a complete report.

## Type and fonts

Each sample is a real report rendered in a builtin style. Open it to read the complete hierarchy, body text, numbers and elements in that treatment. The report keeps its own fonts inside the preview.

{{typography}}

### Reading text, code and mathematics

An explanation can use **strong emphasis**, *italics*, a [source link](https://richview.ai/docs/reference), `literal code` and footnotes.[^note] Use headings to express the argument, a short list for parallel points and a quotation when the speaker matters.

> That's one small step for man, one giant leap for mankind.
>
> — Neil Armstrong, Apollo 11, 21 July 1969 UTC; [NASA Historical Sounds](https://www.nasa.gov/historical-sounds/).

An equation can name an estimate, $\hat p$, and keep the assumptions beside it. The standard-error example at the end of this gallery shows a complete formula.

[^note]: A footnote keeps a useful qualification within reach.

## Pictures

The [report gallery](https://richview.ai/docs/) uses credited Unsplash header photographs in every style. The same credit mapping works in an image block. This photograph is a hotlink; the box, alt text and credit stay when the network is unavailable.

```image
src: https://images.unsplash.com/photo-1593455071238-92dd081a39b1?crop=entropy&cs=tinysrgb&fit=max&fm=jpg&ixid=M3wxMTE0NDQ4fDB8MXxhbGx8fHx8fHx8fHwxNzk0MjQ3MjMwfA&ixlib=rb-4.1.0&q=80&w=1080
alt: Golden grain in a field
caption: Golden grain in a field, photographed by Sarah Mason.
fit: cover
width: 1080
height: 432
credit:
  author: Sarah Mason
  author_url: https://unsplash.com/@sarahthemason?utm_source=richview&utm_medium=referral
  source: Unsplash
  source_url: https://unsplash.com/photos/brown-grass-field-during-daytime-cUY7u2Fskk4?utm_source=richview&utm_medium=referral
```

## Players and blocks

The following report uses Nightingale’s historical observations, her original public-domain diagram and NASA Apollo 11 audio and video. The source keeps captions and a transcript beside the players. External embeds remain dependent on their destination.
