---
title: 2-D bin chart
description: 2-D bin chart for showing where many observations of two numeric fields pile up.
styles: [scientific]
---

Use a 2-D bin chart when a scatter plot has too many points to read: each cell counts the observations inside it and is coloured by that count.

```chart
type: bin2d
title: Larger flats cost more
data:
  inline:
    - { area: 74.7, price: 73.5 }
    - { area: 34.4, price: 43.0 }
    - { area: 60.8, price: 64.1 }
    - { area: 31.4, price: 44.5 }
    - { area: 46.8, price: 60.1 }
    - { area: 48.2, price: 79.0 }
    - { area: 55.8, price: 82.5 }
    - { area: 64.1, price: 80.1 }
    - { area: 60.4, price: 54.9 }
    - { area: 25.1, price: 36.8 }
    - { area: 33.8, price: 46.6 }
    - { area: 71.1, price: 58.7 }
    - { area: 26.7, price: 41.3 }
    - { area: 55.5, price: 72.7 }
    - { area: 43.1, price: 37.7 }
    - { area: 55.2, price: 68.5 }
    - { area: 42.3, price: 45.9 }
    - { area: 27.7, price: 44.0 }
    - { area: 15.7, price: 44.2 }
    - { area: 29.1, price: 42.2 }
    - { area: 49.9, price: 43.7 }
    - { area: 73.0, price: 65.4 }
    - { area: 38.9, price: 44.3 }
    - { area: 36.3, price: 40.5 }
    - { area: 38.8, price: 41.3 }
    - { area: 49.7, price: 84.8 }
    - { area: 74.0, price: 74.0 }
    - { area: 38.0, price: 36.9 }
    - { area: 38.0, price: 39.3 }
    - { area: 15.7, price: 34.6 }
    - { area: 21.9, price: 37.6 }
    - { area: 34.5, price: 35.1 }
    - { area: 30.5, price: 48.1 }
    - { area: 62.0, price: 73.2 }
    - { area: 23.7, price: 33.2 }
    - { area: 23.8, price: 41.3 }
    - { area: 55.1, price: 86.0 }
    - { area: 63.6, price: 76.7 }
    - { area: 28.7, price: 25.8 }
    - { area: 18.2, price: 40.0 }
encoding: { x: area, y: price }
axes:
  x: { label: Floor area (m2) }
  y: { label: Price (thousand EUR) }
```

## Source

````rmd
```chart
type: bin2d
title: Larger flats cost more
data:
  inline:
    - { area: 74.7, price: 73.5 }
    - { area: 34.4, price: 43.0 }
    - { area: 60.8, price: 64.1 }
    - { area: 31.4, price: 44.5 }
    - { area: 46.8, price: 60.1 }
    - { area: 48.2, price: 79.0 }
    - { area: 55.8, price: 82.5 }
    - { area: 64.1, price: 80.1 }
    - { area: 60.4, price: 54.9 }
    - { area: 25.1, price: 36.8 }
    - { area: 33.8, price: 46.6 }
    - { area: 71.1, price: 58.7 }
    - { area: 26.7, price: 41.3 }
    - { area: 55.5, price: 72.7 }
    - { area: 43.1, price: 37.7 }
    - { area: 55.2, price: 68.5 }
    - { area: 42.3, price: 45.9 }
    - { area: 27.7, price: 44.0 }
    - { area: 15.7, price: 44.2 }
    - { area: 29.1, price: 42.2 }
    - { area: 49.9, price: 43.7 }
    - { area: 73.0, price: 65.4 }
    - { area: 38.9, price: 44.3 }
    - { area: 36.3, price: 40.5 }
    - { area: 38.8, price: 41.3 }
    - { area: 49.7, price: 84.8 }
    - { area: 74.0, price: 74.0 }
    - { area: 38.0, price: 36.9 }
    - { area: 38.0, price: 39.3 }
    - { area: 15.7, price: 34.6 }
    - { area: 21.9, price: 37.6 }
    - { area: 34.5, price: 35.1 }
    - { area: 30.5, price: 48.1 }
    - { area: 62.0, price: 73.2 }
    - { area: 23.7, price: 33.2 }
    - { area: 23.8, price: 41.3 }
    - { area: 55.1, price: 86.0 }
    - { area: 63.6, price: 76.7 }
    - { area: 28.7, price: 25.8 }
    - { area: 18.2, price: 40.0 }
encoding: { x: area, y: price }
axes:
  x: { label: Floor area (m2) }
  y: { label: Price (thousand EUR) }
```
````

## Fields that matter

- `type: bin2d` with `encoding.x` and `encoding.y`, both numeric; each row is one observation
- The plane is cut into equal-width rectangles, as many along each axis as the square root of the number of observations (5 to 24); a cell includes its lower edges
- Colour is the count, on the style's sequential ramp from one observation to the fullest cell, and each cell prints its count
- When a cell is too small for its count, none is printed and a key under the chart names both ends of the colour; `auto_annotations: false` asks for the key instead of the counts
- Empty cells stay plain, so nothing is drawn where no observation is
- Every panel of a faceted chart shares the same cells and the same colour scale
- `axes.x` and `axes.y` set `label` and `format`

## Interactions

- Hover or focus a cell to read its x range, its y range and the number of observations.
- Open the Table view, Download PNG or CSV, or Expand the chart.
