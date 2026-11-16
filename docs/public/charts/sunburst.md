---
title: Sunburst chart
description: Sunburst chart for a hierarchy of parts, with angle in proportion to value and rings for depth.
styles: [scientific]
---

Use a sunburst chart to show the parts of a whole and the parts inside them as rings.

```chart
type: sunburst
title: Engineering is the largest group, then sales
subtitle: Thousands of dollars; angle is budget, colour is the top-level group
data:
  inline:
    - {part: Engineering}
    - {part: Platform, group: Engineering}
    - {part: Core, group: Platform, budget: 260}
    - {part: Product, group: Engineering, budget: 310}
    - {part: Sales}
    - {part: EMEA, group: Sales, budget: 240}
    - {part: Americas, group: Sales, budget: 260}
    - {part: Operations, budget: 180}
encoding: {label: part, parent: group, y: budget}
axes:
  y: {unit: k}
```

## Source

````rmd
```chart
type: sunburst
title: Engineering is the largest group, then sales
subtitle: Thousands of dollars; angle is budget, colour is the top-level group
data:
  inline:
    - {part: Engineering}
    - {part: Platform, group: Engineering}
    - {part: Core, group: Platform, budget: 260}
    - {part: Product, group: Engineering, budget: 310}
    - {part: Sales}
    - {part: EMEA, group: Sales, budget: 240}
    - {part: Americas, group: Sales, budget: 260}
    - {part: Operations, budget: 180}
encoding: {label: part, parent: group, y: budget}
axes:
  y: {unit: k}
```
````

## Fields that matter

- `type: sunburst` with `encoding.label` for the part name, `encoding.parent` for the name of its group and `encoding.y` for its value
- A part with children takes the sum of them and needs no value of its own; a row with no parent is a root
- A parent that is not in the data, a repeated name, a cycle or a negative value is reported and the chart is not drawn
- Colour follows the top-level group, and a legend appears when there is more than one
- `axes.y.unit` and `format` describe the value; `encoding.facet` makes one panel per value; `style.ratio` sets the width to height ratio

## Interactions

- Hover or focus a ring segment to read its value and its share of the whole; the pointer picks the segment it is inside.
- Open the Table view, Download PNG or CSV, or Expand the chart.
