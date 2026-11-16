---
title: Pareto chart
description: Pareto chart that ranks causes from the largest down and adds a running share of the total.
styles: [scientific]
---

Use a Pareto chart to show how few causes account for most of a total, such as defects by cause.

```chart
type: pareto
title: Misalignment and discolouration cause most defects
subtitle: Defects per cause, with the running share of the total
data:
  inline:
    - { cause: Scratches, defects: 18 }
    - { cause: Misalignment, defects: 52 }
    - { cause: Cracks, defects: 9 }
    - { cause: Discolouration, defects: 31 }
    - { cause: Other, defects: 6 }
encoding: {x: cause, y: defects}
axes:
  y: {label: Defects}
```

## Source

````rmd
```chart
type: pareto
title: Misalignment and discolouration cause most defects
subtitle: Defects per cause, with the running share of the total
data:
  inline:
    - { cause: Scratches, defects: 18 }
    - { cause: Misalignment, defects: 52 }
    - { cause: Cracks, defects: 9 }
    - { cause: Discolouration, defects: 31 }
    - { cause: Other, defects: 6 }
encoding: {x: cause, y: defects}
axes:
  y: {label: Defects}
```
````

## Fields that matter

- `type: pareto` with `encoding.x` for the cause and `encoding.y` for its nonnegative value
- Bars are sorted from the largest down, and a line at the right adds them up as a share of the total on a 0 to 100 % axis
- Only sorting and adding up are done for you; a negative value fails the chart
- The chart summary says how many causes were needed to pass most of the total
- `axes.y` sets `label`, `format`, `unit`, `min` and `max`; `encoding.facet` makes one panel per value and `style.facet_scale: free` gives each its own value scale; `style.ratio` sets the width to height ratio

## Interactions

- Hover or focus a bar to read its value, its share and the share of the causes up to it.
- Open the Table view, Download PNG or CSV, or Expand the chart.
