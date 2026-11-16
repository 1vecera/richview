---
title: Tree
description: Tree for a hierarchy drawn as nodes joined by elbow curves, with an optional value per node.
styles: [scientific]
---

Use a tree to show how parts belong to the parts above them, read from left to right.

```chart
type: tree
title: Engineering has the most teams
subtitle: Each node is a part; colour is the top-level unit
data:
  inline:
    - {unit: Engineering}
    - {unit: Platform, reports_to: Engineering, headcount: 14}
    - {unit: Product, reports_to: Engineering, headcount: 22}
    - {unit: Sales}
    - {unit: EMEA, reports_to: Sales, headcount: 12}
    - {unit: Americas, reports_to: Sales, headcount: 16}
encoding: {label: unit, parent: reports_to, y: headcount}
```

## Source

````rmd
```chart
type: tree
title: Engineering has the most teams
subtitle: Each node is a part; colour is the top-level unit
data:
  inline:
    - {unit: Engineering}
    - {unit: Platform, reports_to: Engineering, headcount: 14}
    - {unit: Product, reports_to: Engineering, headcount: 22}
    - {unit: Sales}
    - {unit: EMEA, reports_to: Sales, headcount: 12}
    - {unit: Americas, reports_to: Sales, headcount: 16}
encoding: {label: unit, parent: reports_to, y: headcount}
```
````

## Fields that matter

- `type: tree` with `encoding.label` for the part name and `encoding.parent` for the name of its parent
- A row with no parent is a root; leaves are spaced evenly in source order and each parent sits centred over its children
- `encoding.y` is optional and only shown in the readout; it never sizes a node, and a part with children needs none
- Colour follows the top-level part, and a legend appears when there is more than one root
- `encoding.facet` makes one tree per value; `style.ratio` sets the width to height ratio

## Interactions

- Hover or focus a node to read its path, its value where given and how many parts sit under it.
- Open the Table view, Download PNG or CSV, or Expand the chart.
