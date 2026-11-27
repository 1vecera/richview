---
title: Violin chart
description: Violin chart for comparing the shape of a value's distribution across categories.
styles: [scientific]
---

Use a violin chart when averages hide how the values spread: the width of each shape is how common a value is.

```chart
type: violin
title: Premium scores spread wider
data:
  inline:
    - { plan: Standard, score: 41 }
    - { plan: Standard, score: 50 }
    - { plan: Standard, score: 53 }
    - { plan: Standard, score: 58 }
    - { plan: Standard, score: 63 }
    - { plan: Premium, score: 49 }
    - { plan: Premium, score: 68 }
    - { plan: Premium, score: 77 }
    - { plan: Premium, score: 90 }
    - { plan: Premium, score: 94 }
encoding: { x: plan, y: score }
axes:
  y: { label: Satisfaction score }
```

## Source

````rmd
```chart
type: violin
title: Premium scores spread wider
data:
  inline:
    - { plan: Standard, score: 41 }
    - { plan: Standard, score: 50 }
    - { plan: Standard, score: 53 }
    - { plan: Standard, score: 58 }
    - { plan: Standard, score: 63 }
    - { plan: Premium, score: 49 }
    - { plan: Premium, score: 68 }
    - { plan: Premium, score: 77 }
    - { plan: Premium, score: 90 }
    - { plan: Premium, score: 94 }
encoding: { x: plan, y: score }
axes:
  y: { label: Satisfaction score }
```
````

## Fields that matter

- `type: violin` with `encoding.x` for the category and `encoding.y` for the value; each row is one observation
- `encoding.color` splits each category into groups side by side
- The shape is a Gaussian kernel density with Silverman's bandwidth, drawn from the smallest to the largest value; RichView computes it, and every shape uses one density scale
- The thick bar inside is the lower to upper quartile and the dot is the median
- A category with one value, or equal values, is a dot, not a shape
- `axes.y` sets `label`, `format`, `unit`, `min` and `max`; `style.orientation: horizontal` lays the categories down the page

## Interactions

- Hover or focus a violin to read its observations, maximum, quartiles, median and minimum. A dark outline traces the violin under the pointer.
- Click a legend entry to toggle a group when the chart has several.
- Open the Table view, Download PNG or CSV, or Expand the chart.
