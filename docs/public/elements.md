---
title: The elements gallery
description: Charts, flow charts, tables, type, pictures and players, each live on one page with a way in.
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

A table keeps exact values readable, with aligned numbers and named columns. A pivot table sums one measure for every pair of a row field and a column field, with totals. [Blocks & media](https://richview.ai/docs/blocks) documents the table block; the [pivot table page](https://richview.ai/docs/charts/pivot) has the pivot's source, fields and every style.

```table
title: Every region sold more in Q4 than in Q3
data:
  inline:
    - { region: North, q3: 150, q4: 190, change: 40 }
    - { region: South, q3: 105, q4: 140, change: 35 }
    - { region: East, q3: 120, q4: 135, change: 15 }
    - { region: West, q3: 160, q4: 210, change: 50 }
columns:
  - { key: region, label: Region }
  - { key: q3, label: Q3 sales, format: number, align: right }
  - { key: q4, label: Q4 sales, format: number, align: right }
  - { key: change, label: Change, format: number, align: right }
```

{{example:pivot}}

*Source: [the Pivot table example](https://richview.ai/docs/charts/pivot), illustrative data; the table above shows two of its quarters.*

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
