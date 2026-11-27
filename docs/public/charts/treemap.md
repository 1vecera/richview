---
title: Treemap
description: Treemap that gives each part an area in proportion to its value, nested inside its group.
styles: [scientific]
---

Use a treemap to show the parts of a whole by area, with groups nested inside each other.

```chart
type: treemap
title: Engineering is the largest group, then sales
subtitle: Thousands of dollars; area is budget, colour is the top-level group
data:
  inline:
    - {part: Engineering}
    - {part: Platform, group: Engineering, budget: 420}
    - {part: Product, group: Engineering, budget: 310}
    - {part: Sales}
    - {part: EMEA, group: Sales, budget: 240}
    - {part: Americas, group: Sales, budget: 260}
    - {part: Operations, budget: 180}
    - {part: People, budget: 90}
encoding: {label: part, parent: group, y: budget}
axes:
  y: {unit: k}
```

## Source

````rmd
```chart
type: treemap
title: Engineering is the largest group, then sales
subtitle: Thousands of dollars; area is budget, colour is the top-level group
data:
  inline:
    - {part: Engineering}
    - {part: Platform, group: Engineering, budget: 420}
    - {part: Product, group: Engineering, budget: 310}
    - {part: Sales}
    - {part: EMEA, group: Sales, budget: 240}
    - {part: Americas, group: Sales, budget: 260}
    - {part: Operations, budget: 180}
    - {part: People, budget: 90}
encoding: {label: part, parent: group, y: budget}
axes:
  y: {unit: k}
```
````

## Fields that matter

- `type: treemap` with `encoding.label` for the part name, `encoding.parent` for the name of its group and `encoding.y` for its value
- A part with children takes the sum of them and needs no value of its own; a row with no parent is a root
- A parent that is not in the data, a repeated name, a cycle or a negative value is reported and the chart is not drawn
- Colour follows the top-level group, and a legend appears when there is more than one
- The five largest rectangles, three on a narrow chart, print their value under the name where both lines fit; `auto_annotations: false` leaves it to the readout
- `axes.y.unit` and `format` describe the value; `encoding.facet` makes one panel per value; `style.ratio` sets the width to height ratio

## Interactions

- Hover or focus a rectangle to read its value and its share of the whole; the pointer picks the rectangle it is inside and a dark outline traces it.
- Open the Table view, Download PNG or CSV, or Expand the chart.
