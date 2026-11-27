---
title: Area chart
description: Area chart for magnitude over an ordered axis, one band per series, overlaid or stacked.
styles: [scientific]
---

Use an area chart for the magnitude of a total and its parts over an ordered axis; prefer a line chart when comparing categories.

```chart
type: area
title: Daily active users by tier
data:
  inline:
    - { day: "Mon", tier: free, users: 820 }
    - { day: "Tue", tier: free, users: 910 }
    - { day: "Wed", tier: free, users: 870 }
    - { day: "Thu", tier: free, users: 940 }
    - { day: "Mon", tier: paid, users: 240 }
    - { day: "Tue", tier: paid, users: 265 }
    - { day: "Wed", tier: paid, users: 290 }
    - { day: "Thu", tier: paid, users: 305 }
encoding: { x: day, y: users, color: tier }
```

## Source

````rmd
```chart
type: area
title: Daily active users by tier
data:
  inline:
    - { day: "Mon", tier: free, users: 820 }
    - { day: "Tue", tier: free, users: 910 }
    - { day: "Wed", tier: free, users: 870 }
    - { day: "Thu", tier: free, users: 940 }
    - { day: "Mon", tier: paid, users: 240 }
    - { day: "Tue", tier: paid, users: 265 }
    - { day: "Wed", tier: paid, users: 290 }
    - { day: "Thu", tier: paid, users: 305 }
encoding: { x: day, y: users, color: tier }
```
````

## Fields that matter

- `type: area` with `encoding.x` for the ordered axis and `encoding.y` for the measure
- `encoding.color` splits the area into one band per category; bands overlay from the baseline, and `stack: true` stacks them and adds a total to the readout
- `style.curve` is `monotone` (default), `linear`, `step`, `step_before` or `step_after`
- `style.curve: step_after` holds a value until the next observation
- `axes.y.label`, `format` and `unit` describe the value axis
- `axes.y.min` and `axes.y.max` fix the ends of the value axis

Unstacked areas use automatic primary values in complete groups across the shown series: last values first, then first values, then extremes. The total is at most five, or three below 480 px of plot width; if all last values cannot fit, none are drawn. A value stands on the far side of its own line from its fill, on the page or on another band's tint, and never inside its own fill; with no room there it is left out and recorded. Stacked areas keep their part values in readouts and do not add numeric labels. `auto_annotations: false` turns the values off.

## Interactions

- Hover or focus a point to read the values of every band at that position. A ring surrounds the filled dot on each band's line, as it does on a line chart. In a stacked chart the band under the pointer keeps its colour, the others fade, and the dot with its ring marks that band's upper edge.
- Click a legend entry to toggle a band.
- Zoom and pan on the ordered axis with brush, wheel, pinch, keys or double-click.
- Open the Table view, Download PNG or CSV, or Expand the chart.
