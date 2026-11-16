---
title: Heatmap
description: Heatmap for a numeric value across two categorical axes.
styles: [scientific]
---

Use a heatmap to see how a numeric value varies across two categories, such as weekday and hour.

```chart
type: heatmap
title: Deploys by weekday and hour (UTC)
data:
  inline:
    - { weekday: Mon, hour: "09", deploys: 2 }
    - { weekday: Mon, hour: "12", deploys: 4 }
    - { weekday: Mon, hour: "15", deploys: 6 }
    - { weekday: Tue, hour: "09", deploys: 4 }
    - { weekday: Tue, hour: "12", deploys: 7 }
    - { weekday: Tue, hour: "15", deploys: 9 }
    - { weekday: Wed, hour: "09", deploys: 3 }
    - { weekday: Wed, hour: "12", deploys: 5 }
    - { weekday: Wed, hour: "15", deploys: 7 }
encoding: { x: hour, y: weekday, color: deploys }
```

## Source

````rmd
```chart
type: heatmap
title: Deploys by weekday and hour (UTC)
data:
  inline:
    - { weekday: Mon, hour: "09", deploys: 2 }
    - { weekday: Mon, hour: "12", deploys: 4 }
    - { weekday: Mon, hour: "15", deploys: 6 }
    - { weekday: Tue, hour: "09", deploys: 4 }
    - { weekday: Tue, hour: "12", deploys: 7 }
    - { weekday: Tue, hour: "15", deploys: 9 }
    - { weekday: Wed, hour: "09", deploys: 3 }
    - { weekday: Wed, hour: "12", deploys: 5 }
    - { weekday: Wed, hour: "15", deploys: 7 }
encoding: { x: hour, y: weekday, color: deploys }
```
````

## Fields that matter

- `type: heatmap` with `encoding.x` and `encoding.y` as the two categorical axes
- `encoding.color` is the cell value
- `axes.color` sets `format` and `unit` for cell values
- One numeric value per cell; repeated coordinates reject the chart
- Missing values show as a neutral `n/a` cell
- The default palette is monotone, and heatmaps have no color bar

## Interactions

- Hover or focus a cell to read its coordinates and value.
- Open the Table view, Download PNG or CSV, or Expand the chart.
