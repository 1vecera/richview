---
title: Waffle chart
description: Waffle chart that gives each category the squares its share of the whole earns.
styles: [scientific]
---

Use a waffle chart to show shares of a whole as a count of squares.

```chart
type: waffle
title: Cars carry four in ten trips
subtitle: Each square is 1% of the trips
data:
  inline:
    - { mode: Car, trips: 412 }
    - { mode: Train, trips: 236 }
    - { mode: Bicycle, trips: 118 }
    - { mode: Walking, trips: 97 }
    - { mode: Bus, trips: 61 }
encoding: {x: mode, y: trips}
axes:
  y: {label: Trips}
```

## Source

````rmd
```chart
type: waffle
title: Cars carry four in ten trips
subtitle: Each square is 1% of the trips
data:
  inline:
    - { mode: Car, trips: 412 }
    - { mode: Train, trips: 236 }
    - { mode: Bicycle, trips: 118 }
    - { mode: Walking, trips: 97 }
    - { mode: Bus, trips: 61 }
encoding: {x: mode, y: trips}
axes:
  y: {label: Trips}
```
````

## Fields that matter

- `type: waffle` with `encoding.x` for the category and `encoding.y` for its nonnegative value
- Squares are shared out by the largest remainder method, so they always add up to the whole grid; they fill column by column from the bottom left in source order
- `style.cells` is 25, 50 or 100 (the default is 100); any other number draws 100 with a warning
- `encoding.color` sets the colour field, and a legend appears when there is more than one category
- `encoding.facet` makes one grid per value; `style.ratio` sets the width to height ratio

## Interactions

- Hover or focus a square or a category to read its value, its share and its squares.
- Open the Table view, Download PNG or CSV, or Expand the chart.
