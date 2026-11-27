---
title: Empirical distribution chart
description: Empirical distribution chart for the exact share of a sample at or below each value.
styles: [scientific]
---

Use an empirical distribution chart (ECDF) to read what share of a sample falls at or below any value, with nothing smoothed.

```chart
type: ecdf
title: Over half of replies arrive within eight minutes; about one in seven takes longer than twelve
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
type: ecdf
title: Over half of replies arrive within eight minutes; about one in seven takes longer than twelve
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

- `type: ecdf` with `encoding.x` for the numeric field, one value per row
- `encoding.color` draws one step line per group; `encoding.facet` splits the chart into panels
- The value axis runs from 0 % to 100 %; each jump sits exactly at an observed value
- Readouts give the share at or below the value and the number of observations exactly at it; RichView computes both, so they are marked as derived
- Rows without an x are listed as excluded; text that is not a number is an error
- `axes.x` sets `label`, `format` and `unit`

## Interactions

- Hover or focus the chart to read the values at the nearest x; the crosshair snaps to the nearest observation, and a ring surrounds the filled dot on each step line, as it does on a line chart.
- Click a legend entry to toggle a group when the chart has several.
- Zoom and pan on the x axis with brush, wheel, pinch, keys or double-click; Escape or reset returns to the full range.
- Open the Table view, Download PNG or CSV, or Expand the chart.
