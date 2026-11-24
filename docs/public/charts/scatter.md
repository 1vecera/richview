---
title: Scatter plot
description: Scatter plot for the relationship between two continuous variables.
styles: [scientific]
---

Use a scatter plot to show how two continuous variables relate, optionally split by a category.

```chart
type: scatter
title: Latency vs throughput by region
data:
  inline:
    - { region: eu, rps: 820, p95_ms: 180 }
    - { region: eu, rps: 1240, p95_ms: 215 }
    - { region: eu, rps: 1680, p95_ms: 258 }
    - { region: us, rps: 980, p95_ms: 155 }
    - { region: us, rps: 1510, p95_ms: 198 }
    - { region: us, rps: 2050, p95_ms: 232 }
encoding: { x: rps, y: p95_ms, color: region }
```

## Source

````rmd
```chart
type: scatter
title: Latency vs throughput by region
data:
  inline:
    - { region: eu, rps: 820, p95_ms: 180 }
    - { region: eu, rps: 1240, p95_ms: 215 }
    - { region: eu, rps: 1680, p95_ms: 258 }
    - { region: us, rps: 980, p95_ms: 155 }
    - { region: us, rps: 1510, p95_ms: 198 }
    - { region: us, rps: 2050, p95_ms: 232 }
encoding: { x: rps, y: p95_ms, color: region }
```
````

## Fields that matter

- `type: scatter` with `encoding.x` and `encoding.y` as the two numeric fields
- `encoding.color` adds a categorical dimension
- `encoding.size` makes bubble area proportional to a nonnegative value
- `encoding.label` names points; top-level `highlight` prioritizes values of `highlight.by` through `include` and optional `priority`
- `auto_annotations: false` names only the `highlight` points instead of the automatic few
- `axes.x` and `axes.y` set `label`, `format` and `unit`
- `style.ratio` defaults to 4:3 for scatter

## Interactions

- Point at a mark to read its values. A small point grows by 60%, or by 25% when a field sets its size. The ring follows its new outline across an axis, within the same 8 px allowance as the point. Use the arrow keys to select a mark and see the same ring and readout.
- Click a legend entry to toggle a group when the chart has several.
- Zoom and pan on the x axis with brush, wheel, pinch, keys or double-click; Escape or reset returns to the full range.
- Every observation stays in the readouts and the Table view even when its label is hidden.
- Download PNG or CSV, or Expand the chart.
