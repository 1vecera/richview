---
title: Dumbbell chart
description: Dumbbell chart for the change between two measurements per category.
styles: [scientific]
---

Use a dumbbell chart to compare a start and an end value for each category, where the direction of change matters.

```chart
type: dumbbell
title: Three of four teams scored higher after training
subtitle: Score per team; hollow dot before, filled dot after
data:
  inline:
    - {team: Alpha, before: 62, after: 78}
    - {team: Beta, before: 55, after: 51}
    - {team: Gamma, before: 70, after: 88}
    - {team: Delta, before: 48, after: 66}
encoding: {x: team, lower: before, upper: after}
axes:
  y: {label: Score}
```

## Source

````rmd
```chart
type: dumbbell
title: Three of four teams scored higher after training
subtitle: Score per team; hollow dot before, filled dot after
data:
  inline:
    - {team: Alpha, before: 62, after: 78}
    - {team: Beta, before: 55, after: 51}
    - {team: Gamma, before: 70, after: 88}
    - {team: Delta, before: 48, after: 66}
encoding: {x: team, lower: before, upper: after}
axes:
  y: {label: Score}
```
````

## Fields that matter

- `type: dumbbell` with `encoding.x` for the category
- `encoding.lower` is the start value (hollow dot) and `encoding.upper` the end value (filled dot); the change is signed, so the order matters
- `encoding.color` splits each category into groups
- `axes.y` sets `label`, `format`, `unit`, `min` and `max`; the value axis does not need zero
- Up to five rows, three on a narrow chart, print both values beyond their dots, all rows or none; `auto_annotations: false` leaves them to the readout
- `style.ratio` sets the width to height ratio, and `style.facet_scale: free` gives each facet its own scale

## Interactions

- Hover or focus a dumbbell to read its start, end and signed change.
- Click a legend entry to toggle a series when the chart has several.
- Open the Table view, Download PNG or CSV, or Expand the chart.
