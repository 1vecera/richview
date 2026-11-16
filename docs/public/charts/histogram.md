---
title: Histogram
description: Histogram for the distribution of one continuous variable.
styles: [scientific]
---

Use a histogram to see the shape of one continuous variable, such as durations or sizes.

```chart
type: histogram
title: Build durations (seconds)
data:
  inline:
    - { duration: 42 }
    - { duration: 48 }
    - { duration: 55 }
    - { duration: 58 }
    - { duration: 61 }
    - { duration: 63 }
    - { duration: 66 }
    - { duration: 71 }
    - { duration: 73 }
    - { duration: 78 }
    - { duration: 89 }
    - { duration: 94 }
    - { duration: 112 }
encoding: { x: duration }
```

## Source

````rmd
```chart
type: histogram
title: Build durations (seconds)
data:
  inline:
    - { duration: 42 }
    - { duration: 48 }
    - { duration: 55 }
    - { duration: 58 }
    - { duration: 61 }
    - { duration: 63 }
    - { duration: 66 }
    - { duration: 71 }
    - { duration: 73 }
    - { duration: 78 }
    - { duration: 89 }
    - { duration: 94 }
    - { duration: 112 }
encoding: { x: duration }
```
````

## Fields that matter

- `type: histogram` with `encoding.x` as the numeric observation field
- `binning.count` sets 1 to 256 equal-width bins; the default is Sturges
- `binning.extent` limits the range; `binning.edges` sets explicit boundaries
- `binning.measure` is `count` (default), `proportion` or `density`
- `encoding.color` stacks groups within each bin and `encoding.facet` splits panels
- `encoding.y` is not supported; the engine calculates the measure

## Interactions

- Hover or focus a bin to read its range and measure.
- Click a legend entry to toggle a group when colors are used.
- Open the Table view, Download PNG or CSV, or Expand the chart.
