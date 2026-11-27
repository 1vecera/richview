---
title: Bullet chart
description: Bullet chart for one actual value per row against a target and shaded ranges.
styles: [scientific]
---

Use a bullet chart to compare an actual value with its target for several categories on one scale.

```chart
type: bullet
title: South is the only region above its sales target
subtitle: Sales per region; the line marks each region's target
data:
  inline:
    - {region: North, sales: 72, goal: 85}
    - {region: South, sales: 91, goal: 80}
    - {region: East, sales: 55, goal: 70}
    - {region: West, sales: 64, goal: 60}
encoding: {x: region, y: sales, target: goal}
axes:
  y: {label: Sales}
style:
  ranges: [60, 80, 100]
```

## Source

````rmd
```chart
type: bullet
title: South is the only region above its sales target
subtitle: Sales per region; the line marks each region's target
data:
  inline:
    - {region: North, sales: 72, goal: 85}
    - {region: South, sales: 91, goal: 80}
    - {region: East, sales: 55, goal: 70}
    - {region: West, sales: 64, goal: 60}
encoding: {x: region, y: sales, target: goal}
axes:
  y: {label: Sales}
style:
  ranges: [60, 80, 100]
```
````

## Fields that matter

- `type: bullet` with `encoding.x` for the row name and `encoding.y` for the actual value
- `encoding.target` names the field drawn as the line across the bar
- `style.ranges` takes up to three ascending numbers and shades the bands between them; anything else is rejected
- `axes.y` sets `label`, `format`, `unit`, `min` and `max`
- Up to five rows, three on a narrow chart, print the actual value past the end of the bar, all rows or none; a value that would stand at the target line or a band's edge prints none; `auto_annotations: false` leaves them to the readout
- `style.ratio` sets the width to height ratio, and `style.facet_scale: free` gives each facet its own scale

## Interactions

- Hover or focus a bar to read the actual value, the target and the ranges. A dark outline marks the bar under the pointer.
- Open the Table view, Download PNG or CSV, or Expand the chart.
