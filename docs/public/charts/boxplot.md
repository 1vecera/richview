---
title: Box plot
description: Box plot for the distribution of a value within each category.
styles: [scientific]
---

Use a box plot when median and spread matter more than the mean, compared across categories.

```chart
type: boxplot
title: Response latency by region (ms)
data:
  inline:
    - { region: eu, ms: 142 }
    - { region: eu, ms: 168 }
    - { region: eu, ms: 185 }
    - { region: eu, ms: 211 }
    - { region: eu, ms: 240 }
    - { region: eu, ms: 268 }
    - { region: us, ms: 118 }
    - { region: us, ms: 128 }
    - { region: us, ms: 141 }
    - { region: us, ms: 165 }
    - { region: us, ms: 189 }
    - { region: us, ms: 221 }
encoding: { x: region, y: ms }
```

## Source

````rmd
```chart
type: boxplot
title: Response latency by region (ms)
data:
  inline:
    - { region: eu, ms: 142 }
    - { region: eu, ms: 168 }
    - { region: eu, ms: 185 }
    - { region: eu, ms: 211 }
    - { region: eu, ms: 240 }
    - { region: eu, ms: 268 }
    - { region: us, ms: 118 }
    - { region: us, ms: 128 }
    - { region: us, ms: 141 }
    - { region: us, ms: 165 }
    - { region: us, ms: 189 }
    - { region: us, ms: 221 }
encoding: { x: region, y: ms }
```
````

## Fields that matter

- `type: boxplot` with numeric `encoding.y` and optional categorical `encoding.x`
- Without `encoding.x`, one box covers all observations
- `encoding.color` and `encoding.facet` split boxes and panels
- Quartiles use R-7 interpolation; whiskers end at the most extreme observed values within 1.5 IQR
- Observations beyond the fences are drawn individually
- Up to five boxes, three on a narrow chart, print their median beside the box, all boxes or none; `auto_annotations: false` leaves it to the readout
- `style.orientation` is `auto` (default), `vertical` or `horizontal`

## Interactions

- Hover or focus a box to read its derived summary, kept separate from individual outliers.
- Scroll a long readout with the pointer or Page Up and Page Down while the chart has focus.
- Open the Table view, Download PNG or CSV, or Expand the chart.
