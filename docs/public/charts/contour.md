---
title: Contour chart
description: Contour chart for the lines of equal value of a measure over two coordinates.
styles: [scientific]
---

Use a contour chart to show how a value changes over a plane, such as height over a map or a score over two settings.

```chart
type: contour
title: Two hills rise above the valley floor
subtitle: Height on a 1 km grid, illustrative values
data:
  inline:
    - {east: 0, north: 0, height: 13.6}
    - {east: 1, north: 0, height: 17.9}
    - {east: 2, north: 0, height: 24.2}
    - {east: 3, north: 0, height: 29.7}
    - {east: 4, north: 0, height: 30.9}
    - {east: 5, north: 0, height: 27.9}
    - {east: 6, north: 0, height: 24.2}
    - {east: 7, north: 0, height: 22.4}
    - {east: 8, north: 0, height: 22.4}
    - {east: 9, north: 0, height: 23.3}
    - {east: 10, north: 0, height: 24.2}
    - {east: 0, north: 1, height: 15.3}
    - {east: 1, north: 1, height: 23.0}
    - {east: 2, north: 1, height: 34.7}
    - {east: 3, north: 1, height: 44.8}
    - {east: 4, north: 1, height: 46.1}
    - {east: 5, north: 1, height: 38.8}
    - {east: 6, north: 1, height: 30.1}
    - {east: 7, north: 1, height: 25.5}
    - {east: 8, north: 1, height: 24.5}
    - {east: 9, north: 1, height: 24.7}
    - {east: 10, north: 1, height: 25.1}
    - {east: 0, north: 2, height: 16.7}
    - {east: 1, north: 2, height: 27.3}
    - {east: 2, north: 2, height: 43.6}
    - {east: 3, north: 2, height: 57.7}
    - {east: 4, north: 2, height: 59.3}
    - {east: 5, north: 2, height: 48.7}
    - {east: 6, north: 2, height: 36.7}
    - {east: 7, north: 2, height: 30.8}
    - {east: 8, north: 2, height: 29.4}
    - {east: 9, north: 2, height: 28.6}
    - {east: 10, north: 2, height: 27.5}
    - {east: 0, north: 3, height: 16.7}
    - {east: 1, north: 3, height: 27.3}
    - {east: 2, north: 3, height: 43.6}
    - {east: 3, north: 3, height: 57.8}
    - {east: 4, north: 3, height: 59.9}
    - {east: 5, north: 3, height: 50.6}
    - {east: 6, north: 3, height: 41.2}
    - {east: 7, north: 3, height: 38.2}
    - {east: 8, north: 3, height: 38.1}
    - {east: 9, north: 3, height: 36.0}
    - {east: 10, north: 3, height: 31.9}
    - {east: 0, north: 4, height: 15.3}
    - {east: 1, north: 4, height: 23.0}
    - {east: 2, north: 4, height: 34.8}
    - {east: 3, north: 4, height: 45.2}
    - {east: 4, north: 4, height: 47.8}
    - {east: 5, north: 4, height: 44.0}
    - {east: 6, north: 4, height: 42.0}
    - {east: 7, north: 4, height: 45.2}
    - {east: 8, north: 4, height: 47.8}
    - {east: 9, north: 4, height: 44.4}
    - {east: 10, north: 4, height: 37.1}
    - {east: 0, north: 5, height: 13.6}
    - {east: 1, north: 5, height: 17.9}
    - {east: 2, north: 5, height: 24.3}
    - {east: 3, north: 5, height: 30.2}
    - {east: 4, north: 5, height: 33.0}
    - {east: 5, north: 5, height: 34.5}
    - {east: 6, north: 5, height: 39.3}
    - {east: 7, north: 5, height: 47.4}
    - {east: 8, north: 5, height: 52.0}
    - {east: 9, north: 5, height: 48.3}
    - {east: 10, north: 5, height: 39.4}
    - {east: 0, north: 6, height: 12.5}
    - {east: 1, north: 6, height: 14.8}
    - {east: 2, north: 6, height: 17.8}
    - {east: 3, north: 6, height: 20.7}
    - {east: 4, north: 6, height: 23.3}
    - {east: 5, north: 6, height: 27.0}
    - {east: 6, north: 6, height: 33.8}
    - {east: 7, north: 6, height: 42.4}
    - {east: 8, north: 6, height: 47.1}
    - {east: 9, north: 6, height: 44.3}
    - {east: 10, north: 6, height: 37.0}
    - {east: 0, north: 7, height: 12.1}
    - {east: 1, north: 7, height: 13.6}
    - {east: 2, north: 7, height: 15.2}
    - {east: 3, north: 7, height: 16.9}
    - {east: 4, north: 7, height: 19.0}
    - {east: 5, north: 7, height: 22.2}
    - {east: 6, north: 7, height: 27.5}
    - {east: 7, north: 7, height: 33.6}
    - {east: 8, north: 7, height: 37.0}
    - {east: 9, north: 7, height: 35.8}
    - {east: 10, north: 7, height: 31.9}
encoding: {x: east, y: north, z: height}
axes:
  x: {label: East (km)}
  y: {label: North (km)}
  color: {label: Height (m)}
```

## Source

````rmd
```chart
type: contour
title: Two hills rise above the valley floor
subtitle: Height on a 1 km grid, illustrative values
data:
  inline:
    - {east: 0, north: 0, height: 13.6}
    - {east: 1, north: 0, height: 17.9}
    - {east: 2, north: 0, height: 24.2}
    - {east: 3, north: 0, height: 29.7}
    - {east: 4, north: 0, height: 30.9}
    - {east: 5, north: 0, height: 27.9}
    - {east: 6, north: 0, height: 24.2}
    - {east: 7, north: 0, height: 22.4}
    - {east: 8, north: 0, height: 22.4}
    - {east: 9, north: 0, height: 23.3}
    - {east: 10, north: 0, height: 24.2}
    - {east: 0, north: 1, height: 15.3}
    - {east: 1, north: 1, height: 23.0}
    - {east: 2, north: 1, height: 34.7}
    - {east: 3, north: 1, height: 44.8}
    - {east: 4, north: 1, height: 46.1}
    - {east: 5, north: 1, height: 38.8}
    - {east: 6, north: 1, height: 30.1}
    - {east: 7, north: 1, height: 25.5}
    - {east: 8, north: 1, height: 24.5}
    - {east: 9, north: 1, height: 24.7}
    - {east: 10, north: 1, height: 25.1}
    - {east: 0, north: 2, height: 16.7}
    - {east: 1, north: 2, height: 27.3}
    - {east: 2, north: 2, height: 43.6}
    - {east: 3, north: 2, height: 57.7}
    - {east: 4, north: 2, height: 59.3}
    - {east: 5, north: 2, height: 48.7}
    - {east: 6, north: 2, height: 36.7}
    - {east: 7, north: 2, height: 30.8}
    - {east: 8, north: 2, height: 29.4}
    - {east: 9, north: 2, height: 28.6}
    - {east: 10, north: 2, height: 27.5}
    - {east: 0, north: 3, height: 16.7}
    - {east: 1, north: 3, height: 27.3}
    - {east: 2, north: 3, height: 43.6}
    - {east: 3, north: 3, height: 57.8}
    - {east: 4, north: 3, height: 59.9}
    - {east: 5, north: 3, height: 50.6}
    - {east: 6, north: 3, height: 41.2}
    - {east: 7, north: 3, height: 38.2}
    - {east: 8, north: 3, height: 38.1}
    - {east: 9, north: 3, height: 36.0}
    - {east: 10, north: 3, height: 31.9}
    - {east: 0, north: 4, height: 15.3}
    - {east: 1, north: 4, height: 23.0}
    - {east: 2, north: 4, height: 34.8}
    - {east: 3, north: 4, height: 45.2}
    - {east: 4, north: 4, height: 47.8}
    - {east: 5, north: 4, height: 44.0}
    - {east: 6, north: 4, height: 42.0}
    - {east: 7, north: 4, height: 45.2}
    - {east: 8, north: 4, height: 47.8}
    - {east: 9, north: 4, height: 44.4}
    - {east: 10, north: 4, height: 37.1}
    - {east: 0, north: 5, height: 13.6}
    - {east: 1, north: 5, height: 17.9}
    - {east: 2, north: 5, height: 24.3}
    - {east: 3, north: 5, height: 30.2}
    - {east: 4, north: 5, height: 33.0}
    - {east: 5, north: 5, height: 34.5}
    - {east: 6, north: 5, height: 39.3}
    - {east: 7, north: 5, height: 47.4}
    - {east: 8, north: 5, height: 52.0}
    - {east: 9, north: 5, height: 48.3}
    - {east: 10, north: 5, height: 39.4}
    - {east: 0, north: 6, height: 12.5}
    - {east: 1, north: 6, height: 14.8}
    - {east: 2, north: 6, height: 17.8}
    - {east: 3, north: 6, height: 20.7}
    - {east: 4, north: 6, height: 23.3}
    - {east: 5, north: 6, height: 27.0}
    - {east: 6, north: 6, height: 33.8}
    - {east: 7, north: 6, height: 42.4}
    - {east: 8, north: 6, height: 47.1}
    - {east: 9, north: 6, height: 44.3}
    - {east: 10, north: 6, height: 37.0}
    - {east: 0, north: 7, height: 12.1}
    - {east: 1, north: 7, height: 13.6}
    - {east: 2, north: 7, height: 15.2}
    - {east: 3, north: 7, height: 16.9}
    - {east: 4, north: 7, height: 19.0}
    - {east: 5, north: 7, height: 22.2}
    - {east: 6, north: 7, height: 27.5}
    - {east: 7, north: 7, height: 33.6}
    - {east: 8, north: 7, height: 37.0}
    - {east: 9, north: 7, height: 35.8}
    - {east: 10, north: 7, height: 31.9}
encoding: {x: east, y: north, z: height}
axes:
  x: {label: East (km)}
  y: {label: North (km)}
  color: {label: Height (m)}
```
````

## Fields that matter

- `type: contour` with `encoding.x`, `encoding.y` and `encoding.z`, all numeric columns
- Rows that fill every point of a grid are drawn as they are; scattered rows are first interpolated onto a 48 by 36 grid by inverse-distance weighting, and the readouts then say the values are derived; a panel takes at most 4,000 scattered rows (a complete grid of more than 250,000 nodes counts as scattered rows), and above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- Lines are drawn at round levels between the smallest and largest value, about seven of them, coloured along the style's ramp, and a key under the chart decodes the colour; `axes.color.label` and its format name and format the value
- At least three rows spread over x and y are needed; a row with a missing x, y or z is left out and reported
- `encoding.facet` makes one contour chart per value with the same levels and colours

## Interactions

- Hover or focus a place to read x, y and the value at the nearest grid point.
- Open the Table view, Download PNG or CSV, or Expand the chart.
