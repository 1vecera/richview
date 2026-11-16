---
title: Interval chart
description: Interval chart for authored ranges such as forecasts and confidence intervals.
styles: [scientific]
---

Use an interval chart to show a range per category, such as a forecast, a confidence interval or a planned range.

```chart
type: interval
title: Quarterly revenue forecast range
subtitle: Authored lower/upper bounds; dots show the point estimate
data:
  inline:
    - { quarter: "Q1", lower: 1.2, upper: 1.6, estimate: 1.4 }
    - { quarter: "Q2", lower: 1.5, upper: 2.0, estimate: 1.75 }
    - { quarter: "Q3", lower: 1.7, upper: 2.4, estimate: 2.05 }
    - { quarter: "Q4", lower: 2.0, upper: 2.9, estimate: 2.45 }
encoding: { x: quarter, lower: lower, upper: upper, y: estimate }
axes: { y: { label: Revenue (CZK m) } }
```

## Source

````rmd
```chart
type: interval
title: Quarterly revenue forecast range
subtitle: Authored lower/upper bounds; dots show the point estimate
data:
  inline:
    - { quarter: "Q1", lower: 1.2, upper: 1.6, estimate: 1.4 }
    - { quarter: "Q2", lower: 1.5, upper: 2.0, estimate: 1.75 }
    - { quarter: "Q3", lower: 1.7, upper: 2.4, estimate: 2.05 }
    - { quarter: "Q4", lower: 2.0, upper: 2.9, estimate: 2.45 }
encoding: { x: quarter, lower: lower, upper: upper, y: estimate }
axes: { y: { label: Revenue (CZK m) } }
```
````

## Fields that matter

- `type: interval` with categorical `encoding.x`
- `encoding.lower` and `encoding.upper` are the required numeric bounds
- `encoding.y` optionally supplies a point estimate
- `encoding.color` and `encoding.facet` split groups and panels
- `axes.y.label`, `format` and `unit` describe the value axis
- `style.orientation` is `auto` (default), `vertical` or `horizontal`
- Each category and color pair supplies exactly one interval; lower must not exceed upper

## Interactions

- Hover or focus an interval to read its lower and upper bounds separately, with the estimate if given.
- Open the Table view, Download PNG or CSV, or Expand the chart.
