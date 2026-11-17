---
title: Hexbin chart
description: Hexbin chart for showing where many observations of two numeric fields pile up.
styles: [scientific]
---

Each hexagon counts the observations that fall inside it and prints the count; the fuller the hexagon, the darker it is. Use a hexbin chart to show where many observations of two numbers pile up, when a scatter plot has too many points to read.

```chart
type: hexbin
title: Longer rides cost more
data:
  inline:
    - { distance: 23.7, fare: 16.8 }
    - { distance: 6.0, fare: 7.1 }
    - { distance: 38.0, fare: 46.7 }
    - { distance: 27.4, fare: 13.3 }
    - { distance: 11.0, fare: 14.5 }
    - { distance: 35.8, fare: 38.4 }
    - { distance: 30.2, fare: 37.1 }
    - { distance: 49.3, fare: 43.4 }
    - { distance: 38.0, fare: 10.0 }
    - { distance: 49.3, fare: 60.2 }
    - { distance: 27.0, fare: 34.6 }
    - { distance: 26.1, fare: 21.8 }
    - { distance: 50.5, fare: 72.9 }
    - { distance: 51.7, fare: 89.8 }
    - { distance: 53.3, fare: 76.5 }
    - { distance: 17.9, fare: 16.4 }
    - { distance: 35.0, fare: 37.6 }
    - { distance: 54.5, fare: 64.1 }
    - { distance: 37.9, fare: 48.5 }
    - { distance: 49.0, fare: 64.1 }
    - { distance: 26.5, fare: 28.7 }
    - { distance: 54.1, fare: 40.0 }
    - { distance: 54.9, fare: 54.8 }
    - { distance: 9.5, fare: 16.7 }
    - { distance: 19.4, fare: 25.3 }
    - { distance: 22.8, fare: 29.5 }
    - { distance: 50.1, fare: 38.7 }
    - { distance: 38.3, fare: 53.0 }
    - { distance: 34.3, fare: 15.1 }
    - { distance: 28.3, fare: 13.7 }
    - { distance: 19.2, fare: 16.1 }
    - { distance: 33.8, fare: 72.0 }
    - { distance: 19.6, fare: 36.3 }
    - { distance: 10.8, fare: 16.9 }
    - { distance: 29.9, fare: 29.1 }
    - { distance: 42.7, fare: 32.7 }
    - { distance: 8.9, fare: 18.2 }
    - { distance: 35.9, fare: 37.2 }
    - { distance: 18.1, fare: 15.0 }
    - { distance: 18.5, fare: 31.5 }
    - { distance: 24.6, fare: 26.2 }
    - { distance: 19.2, fare: 29.5 }
    - { distance: 20.2, fare: 31.1 }
    - { distance: 41.4, fare: 53.0 }
    - { distance: 16.0, fare: 14.8 }
    - { distance: 29.2, fare: 47.8 }
    - { distance: 11.2, fare: 20.3 }
    - { distance: 16.6, fare: 34.8 }
    - { distance: 38.6, fare: 46.8 }
    - { distance: 16.1, fare: 14.0 }
    - { distance: 17.6, fare: 14.8 }
    - { distance: 15.3, fare: 23.8 }
    - { distance: 28.4, fare: 57.6 }
    - { distance: 33.5, fare: 28.1 }
    - { distance: 32.4, fare: 33.5 }
    - { distance: 9.2, fare: 19.4 }
    - { distance: 53.3, fare: 92.0 }
    - { distance: 35.1, fare: 25.6 }
    - { distance: 33.5, fare: 31.4 }
    - { distance: 27.4, fare: 47.3 }
    - { distance: 36.8, fare: 34.6 }
    - { distance: 30.4, fare: 42.2 }
    - { distance: 24.5, fare: 33.0 }
    - { distance: 16.1, fare: 29.9 }
    - { distance: 51.3, fare: 66.1 }
    - { distance: 27.1, fare: 11.8 }
    - { distance: 15.6, fare: 12.4 }
    - { distance: 44.5, fare: 44.8 }
    - { distance: 8.6, fare: 15.2 }
    - { distance: 48.2, fare: 55.9 }
    - { distance: 22.3, fare: 23.1 }
    - { distance: 35.6, fare: 51.0 }
    - { distance: 13.3, fare: 24.5 }
    - { distance: 25.9, fare: 39.7 }
    - { distance: 13.0, fare: 8.3 }
    - { distance: 38.0, fare: 26.2 }
    - { distance: 25.5, fare: 13.1 }
    - { distance: 40.6, fare: 44.7 }
    - { distance: 19.5, fare: 10.7 }
    - { distance: 19.9, fare: 21.3 }
    - { distance: 52.0, fare: 26.9 }
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
    - { distance: 23.7, fare: 16.8 }
    - { distance: 6.0, fare: 7.1 }
    - { distance: 38.0, fare: 46.7 }
    - { distance: 27.4, fare: 13.3 }
    - { distance: 11.0, fare: 14.5 }
    - { distance: 35.8, fare: 38.4 }
    - { distance: 30.2, fare: 37.1 }
    - { distance: 49.3, fare: 43.4 }
    - { distance: 38.0, fare: 10.0 }
    - { distance: 49.3, fare: 60.2 }
    - { distance: 27.0, fare: 34.6 }
    - { distance: 26.1, fare: 21.8 }
    - { distance: 50.5, fare: 72.9 }
    - { distance: 51.7, fare: 89.8 }
    - { distance: 53.3, fare: 76.5 }
    - { distance: 17.9, fare: 16.4 }
    - { distance: 35.0, fare: 37.6 }
    - { distance: 54.5, fare: 64.1 }
    - { distance: 37.9, fare: 48.5 }
    - { distance: 49.0, fare: 64.1 }
    - { distance: 26.5, fare: 28.7 }
    - { distance: 54.1, fare: 40.0 }
    - { distance: 54.9, fare: 54.8 }
    - { distance: 9.5, fare: 16.7 }
    - { distance: 19.4, fare: 25.3 }
    - { distance: 22.8, fare: 29.5 }
    - { distance: 50.1, fare: 38.7 }
    - { distance: 38.3, fare: 53.0 }
    - { distance: 34.3, fare: 15.1 }
    - { distance: 28.3, fare: 13.7 }
    - { distance: 19.2, fare: 16.1 }
    - { distance: 33.8, fare: 72.0 }
    - { distance: 19.6, fare: 36.3 }
    - { distance: 10.8, fare: 16.9 }
    - { distance: 29.9, fare: 29.1 }
    - { distance: 42.7, fare: 32.7 }
    - { distance: 8.9, fare: 18.2 }
    - { distance: 35.9, fare: 37.2 }
    - { distance: 18.1, fare: 15.0 }
    - { distance: 18.5, fare: 31.5 }
    - { distance: 24.6, fare: 26.2 }
    - { distance: 19.2, fare: 29.5 }
    - { distance: 20.2, fare: 31.1 }
    - { distance: 41.4, fare: 53.0 }
    - { distance: 16.0, fare: 14.8 }
    - { distance: 29.2, fare: 47.8 }
    - { distance: 11.2, fare: 20.3 }
    - { distance: 16.6, fare: 34.8 }
    - { distance: 38.6, fare: 46.8 }
    - { distance: 16.1, fare: 14.0 }
    - { distance: 17.6, fare: 14.8 }
    - { distance: 15.3, fare: 23.8 }
    - { distance: 28.4, fare: 57.6 }
    - { distance: 33.5, fare: 28.1 }
    - { distance: 32.4, fare: 33.5 }
    - { distance: 9.2, fare: 19.4 }
    - { distance: 53.3, fare: 92.0 }
    - { distance: 35.1, fare: 25.6 }
    - { distance: 33.5, fare: 31.4 }
    - { distance: 27.4, fare: 47.3 }
    - { distance: 36.8, fare: 34.6 }
    - { distance: 30.4, fare: 42.2 }
    - { distance: 24.5, fare: 33.0 }
    - { distance: 16.1, fare: 29.9 }
    - { distance: 51.3, fare: 66.1 }
    - { distance: 27.1, fare: 11.8 }
    - { distance: 15.6, fare: 12.4 }
    - { distance: 44.5, fare: 44.8 }
    - { distance: 8.6, fare: 15.2 }
    - { distance: 48.2, fare: 55.9 }
    - { distance: 22.3, fare: 23.1 }
    - { distance: 35.6, fare: 51.0 }
    - { distance: 13.3, fare: 24.5 }
    - { distance: 25.9, fare: 39.7 }
    - { distance: 13.0, fare: 8.3 }
    - { distance: 38.0, fare: 26.2 }
    - { distance: 25.5, fare: 13.1 }
    - { distance: 40.6, fare: 44.7 }
    - { distance: 19.5, fare: 10.7 }
    - { distance: 19.9, fare: 21.3 }
    - { distance: 52.0, fare: 26.9 }
encoding: { x: distance, y: fare }
axes:
  x: { label: Distance (km) }
  y: { label: Fare (EUR) }
```
````

## Fields that matter

- `type: hexbin` with `encoding.x` and `encoding.y`, both numeric; each row is one observation
- Regular hexagons tile the plot, about as many across as the square root of the number of observations (5 to 24)
- Colour is the count, on the style's sequential ramp from one observation to the fullest hexagon, and each hexagon prints its count
- When a hexagon is too small for its count, none is printed and a key under the chart names both ends of the colour; `auto_annotations: false` asks for the key instead of the counts
- Empty hexagons are not drawn, and the plot grows to hold every drawn hexagon whole
- Every panel of a faceted chart shares the same colour scale
- `axes.x` and `axes.y` set `label` and `format`

## Interactions

- Hover or focus a hexagon to read the centre of its x and y and the number of observations.
- Open the Table view, Download PNG or CSV, or Expand the chart.
