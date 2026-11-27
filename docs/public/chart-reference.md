---
title: Charts and interaction
description: Map observations to marks, preserve their meaning and let readers inspect them.
styles: [scientific]
---

Start with the [chart gallery](https://richview.ai/docs/charts) to choose a form. Each type has a live example, its source, the fields that matter and the interactions it supports. The [generated format reference](https://richview.ai/docs/reference#charts) defines every accepted field and each type's required channels.

## Map columns to marks

A chart needs a type, a title, one data source and an encoding. This example is fictional:

```chart
type: line
title: Both cohorts improved
subtitle: Fraction retained after one month; fictional observations
data:
  inline:
    - { month: '2025-01', retained: 0.72, cohort: New }
    - { month: '2025-02', retained: 0.76, cohort: New }
    - { month: '2025-03', retained: 0.81, cohort: New }
    - { month: '2025-01', retained: 0.82, cohort: Existing }
    - { month: '2025-02', retained: 0.84, cohort: Existing }
    - { month: '2025-03', retained: 0.87, cohort: Existing }
encoding: { x: month, y: retained, color: cohort }
axes:
  y: { label: Retained, format: percent }
annotations:
  - { kind: line, at: { y: 0.8 }, text: Target }
```

*Source: fictional tutorial observations. Retention is a fraction, so percent formatting multiplies it by 100.*

`color` separates series; `facet` makes panels. Scatter can size bubbles and label selected points. Other types require other channels: a range needs lower and upper bounds, a flow needs source and target, and a combo needs its bar and line measures. Choose those from the type's page rather than guessing fields.

## Axes, gaps and precision

Bar and area baselines start at zero. Scatter and line axes can focus on their data range; explicit axis bounds carry responsibility for what is clipped. A numeric year can use year formatting without grouping separators. Calendar strings retain their authored time precision, with UTC calendar arithmetic. Missing observations stay gaps unless the author explicitly bridges them.

Printed and readout precision use significant digits, not decimal places. `readout_digits` cannot be below `digits`. A percentage expects a fraction; a unit labels values and does not convert them. A source note should name the population, period, source and any transformation. Readers do not get invented precision.

Lines use bounded monotone smoothing by default. Choose `style.curve: linear` for straight segments or `previous` for steps. Sparse scatter highlights can prioritize named observations without labelling every point. On line, area, bar and lollipop charts, `highlight` makes the series or bars a title names the subject: the rest draw in the style's quiet ink without their own names and values (`highlight.mute: false` keeps their colours). Bars, lines, unstacked areas and density curves print a few exact values on their own, lollipop, pareto, waterfall, funnel, ribbon, heatmap and pie charts print theirs, hexbin and 2-D bin charts print the count of each cell, and a scatter with `encoding.label` names a few points; `auto_annotations: false` prints no value the author did not write and keeps the names. Your own `annotations` are placed first: a value or point name the chart adds moves out of an annotation's way or is left out, and the annotation stays. Use the type pages and generated reference for exact options.

## Give readers a useful control

The [Gapminder report](https://richview.ai/docs/gallery/families-and-lives/) has real country observations and a year playback control. Controls belong to the chart: filters select rows, metric choices select a measure, variants select an authored representation, and playback steps through a field. Playback stops on the last frame, and pressing Play there starts again from the first. These do not turn a report into an arbitrary dashboard query engine.

This small filter example works without a data service:

```chart
type: bar
title: Compare the two fictional regions
data:
  inline:
    - { region: North, month: Jan, orders: 12 }
    - { region: North, month: Feb, orders: 18 }
    - { region: South, month: Jan, orders: 10 }
    - { region: South, month: Feb, orders: 14 }
encoding: { x: month, y: orders }
controls:
  - { kind: filter, id: region, field: region, label: Region, options: [North, South], initial: North, as: buttons }
```

*Source: fictional tutorial observations.*

## Inspect, expand and download

Hover or keyboard focus exposes readouts. Drag across a chart over a numeric or calendar x to zoom, and double-click or press Escape to return; a scatter plot zooms a rectangle along both axes. Surface, 3-D scatter and 3-D bar charts turn: drag the chart, or press Shift with an arrow key, and double-click, press R or choose Reset view to return to the author's view. Point at a legend entry, or focus it with the keyboard, to emphasise its series: the other series step back, except in a Sankey or an alluvial diagram, which outlines the series' ribbons and nodes in the dark outline of a hovered mark and dims nothing. Series controls preserve focus as the chart redraws. The Table view and CSV expose observations; the image download captures the current view. Expand widens the figure on desktop and opens a modal on a phone. Some phone charts use a landscape presentation to show more of the x axis; the chart's state survives closing it. Facets export together.

Dense charts may reduce drawing geometry using M4 for lines/areas or screen-space representatives for scatter. Readouts, Table and CSV retain the observations at the author's precision. The chart discloses its retained density. On a recent laptop, a line of about 50,000 points zooms and pans smoothly, also at phone speed; 200,000 points stay usable on a desktop only, and about 340,000 is the most one report holds before it reaches the 64 MiB page limit. Explicit `style.points: all` accepts the cost of drawing every point. Native `check` estimates text bounds; the browser measures actual glyphs. Read [diagnostics](https://richview.ai/docs/recovery) and inspect the result at its intended width.
