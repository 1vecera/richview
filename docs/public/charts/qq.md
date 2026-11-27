---
title: Quantile-quantile chart
description: Normal quantile-quantile chart that checks whether a numeric sample follows a normal curve.
styles: [scientific]
---

Use a quantile-quantile chart (Q-Q plot) to check how closely a sample follows a normal distribution: points near the line are normal-like, points that bend away are not.

```chart
type: qq
title: Most deliveries track a normal curve, but the slowest few fall well above it
subtitle: Days to deliver against the normal quantile; the dashed line passes through the quartiles
data:
  inline:
    - {days: 2.1}
    - {days: 2.6}
    - {days: 2.9}
    - {days: 3.1}
    - {days: 3.3}
    - {days: 3.4}
    - {days: 3.6}
    - {days: 3.7}
    - {days: 3.9}
    - {days: 4.0}
    - {days: 4.2}
    - {days: 4.3}
    - {days: 4.5}
    - {days: 4.6}
    - {days: 4.9}
    - {days: 5.1}
    - {days: 5.4}
    - {days: 5.8}
    - {days: 6.3}
    - {days: 7.2}
    - {days: 8.9}
encoding: {y: days}
axes:
  y: {label: Days to deliver}
```

## Source

````rmd
```chart
type: qq
title: Most deliveries track a normal curve, but the slowest few fall well above it
subtitle: Days to deliver against the normal quantile; the dashed line passes through the quartiles
data:
  inline:
    - {days: 2.1}
    - {days: 2.6}
    - {days: 2.9}
    - {days: 3.1}
    - {days: 3.3}
    - {days: 3.4}
    - {days: 3.6}
    - {days: 3.7}
    - {days: 3.9}
    - {days: 4.0}
    - {days: 4.2}
    - {days: 4.3}
    - {days: 4.5}
    - {days: 4.6}
    - {days: 4.9}
    - {days: 5.1}
    - {days: 5.4}
    - {days: 5.8}
    - {days: 6.3}
    - {days: 7.2}
    - {days: 8.9}
encoding: {y: days}
axes:
  y: {label: Days to deliver}
```
````

## Fields that matter

- `type: qq` with `encoding.y` for the numeric sample, one value per row
- `encoding.color` draws one set of points and one line per group; `encoding.facet` splits the chart into panels
- x is the standard-normal quantile at plotting position (rank - 0.5) / n; the dashed reference line runs through the first and third quartile pairs
- Readouts give the sample value as you wrote it, its theoretical quantile and its rank; RichView computes the last two, so they are marked as derived
- Rows without a value are listed as excluded; text that is not a number is an error
- `axes.y` sets `label`, `format` and `unit`; the chart is square by default (`style.ratio` changes it)

## Interactions

- Hover or focus a point to read its value, quantile and rank. The point grows and takes a ring in its own colour, as a scatter point does.
- Click a legend entry to toggle a group when the chart has several.
- Zoom and pan on the x axis with brush, wheel, pinch, keys or double-click; Escape or reset returns to the full range.
- Open the Table view, Download PNG or CSV, or Expand the chart.
