---
title: Line chart
description: Line chart for time series and ordered comparisons, with one series per category and an optional second axis.
styles: [scientific]
---

Use a line chart for a time series or any ordered x axis where the shape of change matters.

```chart
type: line
title: Monthly company mentions
data:
  inline:
    - { month: "2026-04", company: Fieldwork, mentions: 1120 }
    - { month: "2026-05", company: Fieldwork, mentions: 1340 }
    - { month: "2026-06", company: Fieldwork, mentions: 1580 }
    - { month: "2026-04", company: Northstar, mentions: 2210 }
    - { month: "2026-05", company: Northstar, mentions: 2180 }
    - { month: "2026-06", company: Northstar, mentions: 2060 }
encoding: { x: month, y: mentions, color: company }
```

## Source

````rmd
```chart
type: line
title: Monthly company mentions
data:
  inline:
    - { month: "2026-04", company: Fieldwork, mentions: 1120 }
    - { month: "2026-05", company: Fieldwork, mentions: 1340 }
    - { month: "2026-06", company: Fieldwork, mentions: 1580 }
    - { month: "2026-04", company: Northstar, mentions: 2210 }
    - { month: "2026-05", company: Northstar, mentions: 2180 }
    - { month: "2026-06", company: Northstar, mentions: 2060 }
encoding: { x: month, y: mentions, color: company }
```
````

## Fields that matter

- `type: line` with `encoding.x` for the ordered axis and `encoding.y` for the measure
- `encoding.color` splits one series per category
- `encoding.y2` adds a second value axis when units differ
- `axes.x` and `axes.y` set `label`, `format`, `unit`, and `axes.y.min` / `axes.y.max` fix the value axis
- `style.curve` is `monotone` (default), `linear`, `step`, `step_before` or `step_after`
- `style.curve: step_after` holds a value until the next observation
- `style.ratio` sets the width to height ratio (16:9 by default)

Primary values appear automatically in complete groups across the shown series: last values first, then first values, then extremes. The total is at most five, or three below 480 px of plot width. If all last-value labels cannot fit clearly, none are drawn. Series names remain separate. `auto_annotations: false` turns the values off and keeps the series names.

## Interactions

- Point at the line to read the nearest observation. A ring surrounds each filled dot at that position, including where the dot crosses an axis. Use the arrow keys to select a mark and see the same ring and readout.
- Click a legend entry to target or toggle a series when the chart has several.
- Zoom and pan on the time axis with brush, wheel, pinch, keys or double-click; Escape or reset returns to the full range.
- Open the Table view, Download PNG or CSV, or Expand the chart.
