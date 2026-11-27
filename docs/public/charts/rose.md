---
title: Rose chart
description: Rose chart with one wedge per category around a circle, its reach the value.
styles: [scientific]
---

Use a rose chart for a cycle, such as months of a year, where the shape around the circle matters.

```chart
type: rose
title: Rain peaks in July
subtitle: Rainfall per month
data:
  inline:
    - { month: Jan, mm: 30 }
    - { month: Feb, mm: 25 }
    - { month: Mar, mm: 40 }
    - { month: Apr, mm: 55 }
    - { month: May, mm: 80 }
    - { month: Jun, mm: 95 }
    - { month: Jul, mm: 110 }
    - { month: Aug, mm: 100 }
    - { month: Sep, mm: 70 }
    - { month: Oct, mm: 50 }
    - { month: Nov, mm: 45 }
    - { month: Dec, mm: 35 }
encoding: {x: month, y: mm}
axes:
  y: {label: Rainfall (mm)}
```

## Source

````rmd
```chart
type: rose
title: Rain peaks in July
subtitle: Rainfall per month
data:
  inline:
    - { month: Jan, mm: 30 }
    - { month: Feb, mm: 25 }
    - { month: Mar, mm: 40 }
    - { month: Apr, mm: 55 }
    - { month: May, mm: 80 }
    - { month: Jun, mm: 95 }
    - { month: Jul, mm: 110 }
    - { month: Aug, mm: 100 }
    - { month: Sep, mm: 70 }
    - { month: Oct, mm: 50 }
    - { month: Nov, mm: 45 }
    - { month: Dec, mm: 35 }
encoding: {x: month, y: mm}
axes:
  y: {label: Rainfall (mm)}
```
````

## Fields that matter

- `type: rose` with `encoding.x` for the category and `encoding.y` for its value
- Wedges run clockwise from 12 o'clock in source order, and the radius grows linearly from a zero centre
- `style.scale: area` makes the wedge area proportional to the value instead, and the chart says which it drew
- `encoding.color` splits each category into groups side by side inside its sector, with a legend
- `axes.y` sets `label`, `format`, `unit` and `max`; `encoding.facet` makes one rose per value and `style.facet_scale: free` gives each its own scale

## Interactions

- Hover or focus a wedge to read its category and value; a dark outline traces the wedge and the others keep their colour.
- Open the Table view, Download PNG or CSV, or Expand the chart.
