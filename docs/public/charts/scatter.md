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
- Zoom and pan on both axes: drag a rectangle, use Ctrl or ⌘ with the wheel or a pinch, press + and −, and Shift-drag to pan; double-click, Escape or Reset zoom returns to the full range. [Zoom on both axes](#zoom-on-both-axes) has a chart to try it on.
- Every observation stays in the readouts and the Table view even when its label is hidden.
- Download PNG or CSV, or Expand the chart.

## Zoom on both axes

Drag a rectangle across the chart below to zoom into it. Its ticks, gridlines and labels follow the window, so zooming into the crowded middle shows finer steps on both axes.

```chart
type: scatter
title: Fuel use against engine power
subtitle: Illustrative figures for 96 cars
data:
  inline:
    - {fuel: petrol, power_kw: 92, l_100km: 7.4}
    - {fuel: petrol, power_kw: 100, l_100km: 8.6}
    - {fuel: petrol, power_kw: 128, l_100km: 9.2}
    - {fuel: petrol, power_kw: 93, l_100km: 7.6}
    - {fuel: petrol, power_kw: 81, l_100km: 6.9}
    - {fuel: petrol, power_kw: 56, l_100km: 5.7}
    - {fuel: petrol, power_kw: 114, l_100km: 8.4}
    - {fuel: petrol, power_kw: 114, l_100km: 7.8}
    - {fuel: petrol, power_kw: 97, l_100km: 7.9}
    - {fuel: petrol, power_kw: 106, l_100km: 8.1}
    - {fuel: petrol, power_kw: 112, l_100km: 8.8}
    - {fuel: petrol, power_kw: 121, l_100km: 7.8}
    - {fuel: petrol, power_kw: 65, l_100km: 7.4}
    - {fuel: petrol, power_kw: 149, l_100km: 9.9}
    - {fuel: petrol, power_kw: 85, l_100km: 8.0}
    - {fuel: petrol, power_kw: 97, l_100km: 8.0}
    - {fuel: petrol, power_kw: 122, l_100km: 8.8}
    - {fuel: petrol, power_kw: 54, l_100km: 6.1}
    - {fuel: petrol, power_kw: 106, l_100km: 7.8}
    - {fuel: petrol, power_kw: 100, l_100km: 7.8}
    - {fuel: petrol, power_kw: 104, l_100km: 7.9}
    - {fuel: petrol, power_kw: 112, l_100km: 8.2}
    - {fuel: petrol, power_kw: 68, l_100km: 7.1}
    - {fuel: petrol, power_kw: 100, l_100km: 8.1}
    - {fuel: petrol, power_kw: 59, l_100km: 6.4}
    - {fuel: petrol, power_kw: 114, l_100km: 8.6}
    - {fuel: petrol, power_kw: 66, l_100km: 7.4}
    - {fuel: petrol, power_kw: 102, l_100km: 8.1}
    - {fuel: petrol, power_kw: 82, l_100km: 7.3}
    - {fuel: petrol, power_kw: 103, l_100km: 7.1}
    - {fuel: petrol, power_kw: 84, l_100km: 6.9}
    - {fuel: petrol, power_kw: 102, l_100km: 8.1}
    - {fuel: petrol, power_kw: 93, l_100km: 7.9}
    - {fuel: petrol, power_kw: 129, l_100km: 9.8}
    - {fuel: diesel, power_kw: 77, l_100km: 6.5}
    - {fuel: diesel, power_kw: 115, l_100km: 6.1}
    - {fuel: diesel, power_kw: 149, l_100km: 7.4}
    - {fuel: diesel, power_kw: 94, l_100km: 6.6}
    - {fuel: diesel, power_kw: 106, l_100km: 6.6}
    - {fuel: diesel, power_kw: 147, l_100km: 7.8}
    - {fuel: diesel, power_kw: 45, l_100km: 4.8}
    - {fuel: diesel, power_kw: 74, l_100km: 5.2}
    - {fuel: diesel, power_kw: 75, l_100km: 5.8}
    - {fuel: diesel, power_kw: 115, l_100km: 6.7}
    - {fuel: diesel, power_kw: 141, l_100km: 7.6}
    - {fuel: diesel, power_kw: 104, l_100km: 6.9}
    - {fuel: diesel, power_kw: 128, l_100km: 7.3}
    - {fuel: diesel, power_kw: 143, l_100km: 7.9}
    - {fuel: diesel, power_kw: 118, l_100km: 6.9}
    - {fuel: diesel, power_kw: 100, l_100km: 6.0}
    - {fuel: diesel, power_kw: 178, l_100km: 8.6}
    - {fuel: diesel, power_kw: 46, l_100km: 5.0}
    - {fuel: diesel, power_kw: 91, l_100km: 5.8}
    - {fuel: diesel, power_kw: 102, l_100km: 6.6}
    - {fuel: diesel, power_kw: 141, l_100km: 7.4}
    - {fuel: diesel, power_kw: 107, l_100km: 6.8}
    - {fuel: diesel, power_kw: 46, l_100km: 4.3}
    - {fuel: diesel, power_kw: 71, l_100km: 6.0}
    - {fuel: diesel, power_kw: 102, l_100km: 6.0}
    - {fuel: diesel, power_kw: 86, l_100km: 5.5}
    - {fuel: diesel, power_kw: 120, l_100km: 6.9}
    - {fuel: diesel, power_kw: 125, l_100km: 7.3}
    - {fuel: diesel, power_kw: 133, l_100km: 7.4}
    - {fuel: diesel, power_kw: 94, l_100km: 6.3}
    - {fuel: diesel, power_kw: 109, l_100km: 6.6}
    - {fuel: diesel, power_kw: 150, l_100km: 7.4}
    - {fuel: diesel, power_kw: 127, l_100km: 7.3}
    - {fuel: diesel, power_kw: 113, l_100km: 6.5}
    - {fuel: hybrid, power_kw: 98, l_100km: 4.1}
    - {fuel: hybrid, power_kw: 78, l_100km: 3.6}
    - {fuel: hybrid, power_kw: 67, l_100km: 3.7}
    - {fuel: hybrid, power_kw: 147, l_100km: 4.3}
    - {fuel: hybrid, power_kw: 118, l_100km: 4.5}
    - {fuel: hybrid, power_kw: 120, l_100km: 4.8}
    - {fuel: hybrid, power_kw: 101, l_100km: 4.4}
    - {fuel: hybrid, power_kw: 101, l_100km: 4.2}
    - {fuel: hybrid, power_kw: 97, l_100km: 4.4}
    - {fuel: hybrid, power_kw: 65, l_100km: 3.7}
    - {fuel: hybrid, power_kw: 95, l_100km: 3.8}
    - {fuel: hybrid, power_kw: 97, l_100km: 4.8}
    - {fuel: hybrid, power_kw: 94, l_100km: 4.6}
    - {fuel: hybrid, power_kw: 121, l_100km: 4.3}
    - {fuel: hybrid, power_kw: 92, l_100km: 4.1}
    - {fuel: hybrid, power_kw: 95, l_100km: 5.0}
    - {fuel: hybrid, power_kw: 104, l_100km: 4.3}
    - {fuel: hybrid, power_kw: 86, l_100km: 3.9}
    - {fuel: hybrid, power_kw: 87, l_100km: 3.9}
    - {fuel: hybrid, power_kw: 101, l_100km: 4.5}
    - {fuel: hybrid, power_kw: 92, l_100km: 3.6}
    - {fuel: hybrid, power_kw: 126, l_100km: 4.6}
    - {fuel: hybrid, power_kw: 100, l_100km: 3.8}
    - {fuel: hybrid, power_kw: 100, l_100km: 4.2}
    - {fuel: hybrid, power_kw: 106, l_100km: 4.2}
    - {fuel: hybrid, power_kw: 104, l_100km: 4.2}
    - {fuel: hybrid, power_kw: 154, l_100km: 4.1}
    - {fuel: hybrid, power_kw: 101, l_100km: 3.4}
encoding: {x: power_kw, y: l_100km, color: fuel}
axes:
  x: {label: Engine power (kW)}
  y: {label: Fuel use (L/100 km)}
```

- **Drag a rectangle** to zoom to it. A drag along one axis, within a few pixels of level or upright, zooms that axis alone and keeps the other window.
- **Wheel and pinch.** Hold Ctrl or ⌘ and turn the wheel to zoom around the pointer on both axes; a plain wheel still scrolls the page. On a phone, pinch and drag in the expanded view, where the page does not scroll under the chart.
- **Pan.** Shift-drag moves the window along both axes; in the expanded view on a phone, one finger does.
- **Keys.** With the chart focused, + and − zoom around the middle of the plot, and Escape returns to the full range, a selection first.
- **Reset.** Double-click, press Escape or choose Reset zoom.
- Only the points inside the window are drawn and read, in the hover readout, with the arrow keys and in the labels. The Table and CSV always hold every row.
- A zoom stops where one step of the data's position grid spans about two pixels, on each axis, so the chart never shows more digits than the author chose.
- Facets that set `style.facet_scale: free` have a value scale for each panel and zoom along x only.
