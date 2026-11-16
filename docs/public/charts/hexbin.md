---
title: Hexbin chart
description: Hexbin chart for showing where many observations of two numeric fields pile up.
styles: [scientific]
---

Use a hexbin chart when a scatter plot has too many points to read: each hexagon counts the observations inside it and is coloured by that count.

```chart
type: hexbin
title: Longer rides cost more
data:
  inline:
    - { distance: 12.0, fare: 28.2 }
    - { distance: 46.0, fare: 63.6 }
    - { distance: 61.8, fare: 59.3 }
    - { distance: 14.2, fare: 30.2 }
    - { distance: 28.2, fare: 35.3 }
    - { distance: 24.4, fare: 31.3 }
    - { distance: 30.4, fare: 27.1 }
    - { distance: 13.5, fare: 26.6 }
    - { distance: 22.5, fare: 25.1 }
    - { distance: 44.9, fare: 62.4 }
    - { distance: 17.3, fare: 32.1 }
    - { distance: 49.0, fare: 54.2 }
    - { distance: 21.0, fare: 25.1 }
    - { distance: 45.8, fare: 58.2 }
    - { distance: 14.1, fare: 34.1 }
    - { distance: 49.6, fare: 48.3 }
    - { distance: 48.8, fare: 60.8 }
    - { distance: 55.9, fare: 53.4 }
    - { distance: 25.4, fare: 31.2 }
    - { distance: 24.1, fare: 35.5 }
    - { distance: 59.4, fare: 48.4 }
    - { distance: 57.1, fare: 56.9 }
    - { distance: 20.9, fare: 28.0 }
    - { distance: 52.9, fare: 62.7 }
    - { distance: 44.6, fare: 68.6 }
    - { distance: 24.9, fare: 29.8 }
    - { distance: 56.5, fare: 54.2 }
    - { distance: 21.9, fare: 42.1 }
    - { distance: 54.8, fare: 52.1 }
    - { distance: 21.2, fare: 39.4 }
    - { distance: 15.1, fare: 14.0 }
    - { distance: 19.8, fare: 35.8 }
    - { distance: 24.7, fare: 19.4 }
    - { distance: 22.1, fare: 27.1 }
    - { distance: 43.3, fare: 50.8 }
    - { distance: 25.2, fare: 40.7 }
    - { distance: 61.0, fare: 59.0 }
    - { distance: 12.1, fare: 26.4 }
    - { distance: 17.5, fare: 30.1 }
    - { distance: 20.2, fare: 32.0 }
    - { distance: 47.0, fare: 57.1 }
    - { distance: 14.3, fare: 37.6 }
    - { distance: 50.5, fare: 68.4 }
    - { distance: 24.6, fare: 28.0 }
    - { distance: 45.3, fare: 49.9 }
encoding: { x: distance, y: fare }
axes:
  x: { label: Distance (km) }
  y: { label: Fare (EUR) }
```

## Source

````rmd
```chart
type: hexbin
title: Longer rides cost more
data:
  inline:
    - { distance: 12.0, fare: 28.2 }
    - { distance: 46.0, fare: 63.6 }
    - { distance: 61.8, fare: 59.3 }
    - { distance: 14.2, fare: 30.2 }
    - { distance: 28.2, fare: 35.3 }
    - { distance: 24.4, fare: 31.3 }
    - { distance: 30.4, fare: 27.1 }
    - { distance: 13.5, fare: 26.6 }
    - { distance: 22.5, fare: 25.1 }
    - { distance: 44.9, fare: 62.4 }
    - { distance: 17.3, fare: 32.1 }
    - { distance: 49.0, fare: 54.2 }
    - { distance: 21.0, fare: 25.1 }
    - { distance: 45.8, fare: 58.2 }
    - { distance: 14.1, fare: 34.1 }
    - { distance: 49.6, fare: 48.3 }
    - { distance: 48.8, fare: 60.8 }
    - { distance: 55.9, fare: 53.4 }
    - { distance: 25.4, fare: 31.2 }
    - { distance: 24.1, fare: 35.5 }
    - { distance: 59.4, fare: 48.4 }
    - { distance: 57.1, fare: 56.9 }
    - { distance: 20.9, fare: 28.0 }
    - { distance: 52.9, fare: 62.7 }
    - { distance: 44.6, fare: 68.6 }
    - { distance: 24.9, fare: 29.8 }
    - { distance: 56.5, fare: 54.2 }
    - { distance: 21.9, fare: 42.1 }
    - { distance: 54.8, fare: 52.1 }
    - { distance: 21.2, fare: 39.4 }
    - { distance: 15.1, fare: 14.0 }
    - { distance: 19.8, fare: 35.8 }
    - { distance: 24.7, fare: 19.4 }
    - { distance: 22.1, fare: 27.1 }
    - { distance: 43.3, fare: 50.8 }
    - { distance: 25.2, fare: 40.7 }
    - { distance: 61.0, fare: 59.0 }
    - { distance: 12.1, fare: 26.4 }
    - { distance: 17.5, fare: 30.1 }
    - { distance: 20.2, fare: 32.0 }
    - { distance: 47.0, fare: 57.1 }
    - { distance: 14.3, fare: 37.6 }
    - { distance: 50.5, fare: 68.4 }
    - { distance: 24.6, fare: 28.0 }
    - { distance: 45.3, fare: 49.9 }
encoding: { x: distance, y: fare }
axes:
  x: { label: Distance (km) }
  y: { label: Fare (EUR) }
```
````

## Fields that matter

- `type: hexbin` with `encoding.x` and `encoding.y`, both numeric; each row is one observation
- Regular hexagons tile the plot, about as many across as the square root of the number of observations (5 to 24)
- Colour is the count, on the style's sequential ramp from one observation to the fullest hexagon; the key under the chart names both ends
- Empty hexagons are not drawn, and hexagons at the edge are cut at the plot
- Every panel of a faceted chart shares the same colour scale
- `axes.x` and `axes.y` set `label` and `format`

## Interactions

- Hover or focus a hexagon to read the centre of its x and y and the number of observations.
- Open the Table view, Download PNG or CSV, or Expand the chart.
