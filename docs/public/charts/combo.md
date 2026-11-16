---
title: Combo chart
description: Combo chart with bars for one measure and a line for another on its own axis.
styles: [scientific]
---

Use a combo chart to read two measures with different units against the same categories.

```chart
type: combo
title: Revenue grew by half and the margin gained nine points
subtitle: Revenue in thousands of dollars, margin in percent
data:
  inline:
    - { quarter: Q1, revenue: 120, margin: 18 }
    - { quarter: Q2, revenue: 150, margin: 22 }
    - { quarter: Q3, revenue: 135, margin: 20 }
    - { quarter: Q4, revenue: 180, margin: 27 }
encoding: {x: quarter, y: revenue, line: margin}
axes:
  y: {label: Revenue, unit: $}
  y2: {label: Margin, unit: "%"}
```

## Source

````rmd
```chart
type: combo
title: Revenue grew by half and the margin gained nine points
subtitle: Revenue in thousands of dollars, margin in percent
data:
  inline:
    - { quarter: Q1, revenue: 120, margin: 18 }
    - { quarter: Q2, revenue: 150, margin: 22 }
    - { quarter: Q3, revenue: 135, margin: 20 }
    - { quarter: Q4, revenue: 180, margin: 27 }
encoding: {x: quarter, y: revenue, line: margin}
axes:
  y: {label: Revenue, unit: $}
  y2: {label: Margin, unit: "%"}
```
````

## Fields that matter

- `type: combo` with `encoding.x` for the category, `encoding.y` for the bars and `encoding.line` for the line; all three are required
- `axes.y` labels the bar axis at the left and `axes.y2` the line axis at the right; both take `label`, `unit`, `format`, `digits`, `min` and `max`, and each has its own scale
- A missing line value breaks the line unless `style.gaps` is `bridge`; nothing is interpolated
- The bars print their values and each line its last value, each in its own unit; a value with no clear place is left to the readout, and `auto_annotations: false` prints none
- `encoding.color` splits both measures by group, with a legend
- `encoding.facet` makes one panel per value; `style.facet_scale: free` gives each its own scales and `style.ratio` sets the width to height ratio

## Interactions

- Hover or focus a category to read its bar value and its line value together.
- Open the Table view, Download PNG or CSV, or Expand the chart.
