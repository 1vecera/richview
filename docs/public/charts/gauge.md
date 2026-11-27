---
title: Gauge chart
description: Gauge chart for one value on an arc, with optional bands that split the scale.
styles: [scientific]
---

Use a gauge to show where a single value stands against a scale the reader already knows, such as a score out of 100 or a target.

```chart
type: gauge
title: Customer satisfaction is fair, short of good
subtitle: Survey score out of 100
data:
  inline:
    - {measure: Satisfaction, score: 72}
encoding: {x: measure, y: score}
axes:
  y: {label: Score}
style:
  min: 0
  max: 100
  bands:
    - {to: 50, label: Poor}
    - {to: 75, label: Fair}
    - {to: 100, label: Good}
```

## Source

````rmd
```chart
type: gauge
title: Customer satisfaction is fair, short of good
subtitle: Survey score out of 100
data:
  inline:
    - {measure: Satisfaction, score: 72}
encoding: {x: measure, y: score}
axes:
  y: {label: Score}
style:
  min: 0
  max: 100
  bands:
    - {to: 50, label: Poor}
    - {to: 75, label: Fair}
    - {to: 100, label: Good}
```
````

## Fields that matter

- `type: gauge` with `encoding.y` for the value; `encoding.x` is an optional name shown under it
- A gauge holds one row per panel; use `encoding.facet` for one gauge per row
- `style.min` and `style.max` set the scale (they default to 0 and the end of the last band, or the next round number above the value); a value outside the scale is drawn at its edge and counted
- `style.bands` is a list of `{to, label, color}`, each band running from the end of the one before; `label` defaults to the range, and `color` picks a palette slot, otherwise bands take the style's ramp from light to dark
- With bands the arc is split and a needle points at the value, and a key under the arc names every band; without them the arc fills up to the value
- RichView draws the bands as written and grades nothing; `axes.y` sets `label`, `format` and `unit`

## Interactions

- Hover or focus the chart to read the value, its band and the scale; a dark outline traces the band the needle is in.
- Open the Table view, Download PNG or CSV, or Expand the chart.
