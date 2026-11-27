---
title: Band chart
description: Band chart for a range between two bounds over time or a number, with an optional centre line.
styles: [scientific]
---

Use a band chart to show a range, such as a low and a high, along a time or numeric x axis.

```chart
type: band
title: Daily highs and lows drift upward through March
subtitle: Degrees Celsius; the line is the daily mean
data:
  inline:
    - {day: 2025-03-01, low: 6.1, high: 15.2, mean: 10.4}
    - {day: 2025-03-02, low: 7.0, high: 16.8, mean: 11.9}
    - {day: 2025-03-03, low: 7.9, high: 17.5, mean: 12.6}
    - {day: 2025-03-04, low: 8.3, high: 18.2, mean: 13.1}
    - {day: 2025-03-05, low: 8.8, high: 18.9, mean: 13.7}
    - {day: 2025-03-06, low: 9.4, high: 19.6, mean: 14.2}
    - {day: 2025-03-07, low: 9.1, high: 19.1, mean: 13.9}
encoding: {x: day, lower: low, upper: high, y: mean}
axes:
  y: {label: Temperature, unit: °C}
```

## Source

````rmd
```chart
type: band
title: Daily highs and lows drift upward through March
subtitle: Degrees Celsius; the line is the daily mean
data:
  inline:
    - {day: 2025-03-01, low: 6.1, high: 15.2, mean: 10.4}
    - {day: 2025-03-02, low: 7.0, high: 16.8, mean: 11.9}
    - {day: 2025-03-03, low: 7.9, high: 17.5, mean: 12.6}
    - {day: 2025-03-04, low: 8.3, high: 18.2, mean: 13.1}
    - {day: 2025-03-05, low: 8.8, high: 18.9, mean: 13.7}
    - {day: 2025-03-06, low: 9.4, high: 19.6, mean: 14.2}
    - {day: 2025-03-07, low: 9.1, high: 19.1, mean: 13.9}
encoding: {x: day, lower: low, upper: high, y: mean}
axes:
  y: {label: Temperature, unit: °C}
```
````

## Fields that matter

- `type: band` with `encoding.x` for a date or a number
- `encoding.lower` and `encoding.upper` are the bounds; a lower bound above its upper bound is rejected
- `encoding.y` adds an optional centre line, such as a mean; RichView draws the bounds it is given and infers none
- `encoding.color` draws one band per group
- `axes.x` and `axes.y` set `label`, `format` and `unit`; `axes.y.min` / `axes.y.max` fix the value axis

## Interactions

- Hover or focus the chart to read the bounds at the nearest x value; the crosshair snaps to the nearest observation, and a ring surrounds the filled dot on each band's centre line, as it does on a line chart.
- Click a legend entry to toggle a series when the chart has several.
- Zoom and pan on the x axis with brush, wheel, pinch, keys or double-click; Escape or reset returns to the full range.
- Open the Table view, Download PNG or CSV, or Expand the chart.
