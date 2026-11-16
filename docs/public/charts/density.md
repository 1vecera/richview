---
title: Density chart
description: Density chart for how a numeric sample is spread, one smooth curve per group.
styles: [scientific]
---

Use a density chart to show where the values of a sample cluster and how far they trail, with one curve per group.

```chart
type: density
title: Replies cluster near seven minutes, with a long slow tail
subtitle: Minutes to first reply
data:
  inline:
    - {minutes: 3.1}
    - {minutes: 4.2}
    - {minutes: 4.8}
    - {minutes: 5.0}
    - {minutes: 5.3}
    - {minutes: 5.9}
    - {minutes: 6.1}
    - {minutes: 6.4}
    - {minutes: 6.6}
    - {minutes: 7.0}
    - {minutes: 7.2}
    - {minutes: 7.5}
    - {minutes: 7.9}
    - {minutes: 8.3}
    - {minutes: 8.8}
    - {minutes: 9.4}
    - {minutes: 10.1}
    - {minutes: 10.9}
    - {minutes: 12.0}
    - {minutes: 13.8}
    - {minutes: 15.5}
    - {minutes: 18.9}
encoding: {x: minutes}
axes:
  x: {label: Minutes to first reply}
```

## Source

````rmd
```chart
type: density
title: Replies cluster near seven minutes, with a long slow tail
subtitle: Minutes to first reply
data:
  inline:
    - {minutes: 3.1}
    - {minutes: 4.2}
    - {minutes: 4.8}
    - {minutes: 5.0}
    - {minutes: 5.3}
    - {minutes: 5.9}
    - {minutes: 6.1}
    - {minutes: 6.4}
    - {minutes: 6.6}
    - {minutes: 7.0}
    - {minutes: 7.2}
    - {minutes: 7.5}
    - {minutes: 7.9}
    - {minutes: 8.3}
    - {minutes: 8.8}
    - {minutes: 9.4}
    - {minutes: 10.1}
    - {minutes: 10.9}
    - {minutes: 12.0}
    - {minutes: 13.8}
    - {minutes: 15.5}
    - {minutes: 18.9}
encoding: {x: minutes}
axes:
  x: {label: Minutes to first reply}
```
````

## Fields that matter

- `type: density` with `encoding.x` for the numeric field, one value per row
- `encoding.color` draws one curve per group; `encoding.facet` splits the chart into panels
- `style.bandwidth` sets the kernel bandwidth as a positive number; without it RichView uses Silverman's rule of thumb and says so in the note under the chart
- RichView computes the curve (a Gaussian kernel estimate on 128 points, padded by three bandwidths either side), so every value in a readout is marked as derived
- A group with fewer than two distinct values is reported and not drawn; rows without an x are listed as excluded
- `axes.x` and `axes.y` set `label`, `format` and `unit`

A curve may show one existing recorded density evaluation near its sampled peak, at the readout’s precision. This is a derived sampled value, not an analytic maximum. Groups share a total budget of five labels, or three below 480 px of plot width; a value with no clear position stays unlabelled. `auto_annotations: false` turns the value off.

## Interactions

- Hover or focus the chart to read the values at the nearest x; the crosshair snaps to the nearest observation.
- Click a legend entry to toggle a group when the chart has several.
- Zoom and pan on the x axis with brush, wheel, pinch, keys or double-click; Escape or reset returns to the full range.
- Open the Table view, Download PNG or CSV, or Expand the chart.
