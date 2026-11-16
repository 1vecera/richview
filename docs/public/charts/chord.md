---
title: Chord diagram
description: Chord diagram for flows between a few nodes, with arcs around a circle and ribbons as wide as the flow between them.
styles: [scientific]
---

Use a chord diagram to show how much flows between every pair among a small set of nodes, such as trade between regions or moves between teams.

```chart
type: chord
title: Europe, Asia and the Americas dominate trade between five regions
subtitle: Exports in bn USD from one region to another, synthetic
data:
  inline:
    - {from: Europe, to: Asia, bn: 40}
    - {from: Europe, to: Americas, bn: 55}
    - {from: Europe, to: Africa, bn: 20}
    - {from: Asia, to: Europe, bn: 35}
    - {from: Asia, to: Americas, bn: 60}
    - {from: Asia, to: Oceania, bn: 25}
    - {from: Americas, to: Europe, bn: 45}
    - {from: Americas, to: Asia, bn: 30}
    - {from: Americas, to: Africa, bn: 10}
    - {from: Africa, to: Europe, bn: 15}
    - {from: Africa, to: Asia, bn: 12}
    - {from: Oceania, to: Asia, bn: 18}
    - {from: Oceania, to: Americas, bn: 8}
encoding: {source: from, target: to, y: bn}
axes:
  y: {label: bn USD}
```

## Source

````rmd
```chart
type: chord
title: Europe, Asia and the Americas dominate trade between five regions
subtitle: Exports in bn USD from one region to another, synthetic
data:
  inline:
    - {from: Europe, to: Asia, bn: 40}
    - {from: Europe, to: Americas, bn: 55}
    - {from: Europe, to: Africa, bn: 20}
    - {from: Asia, to: Europe, bn: 35}
    - {from: Asia, to: Americas, bn: 60}
    - {from: Asia, to: Oceania, bn: 25}
    - {from: Americas, to: Europe, bn: 45}
    - {from: Americas, to: Asia, bn: 30}
    - {from: Americas, to: Africa, bn: 10}
    - {from: Africa, to: Europe, bn: 15}
    - {from: Africa, to: Asia, bn: 12}
    - {from: Oceania, to: Asia, bn: 18}
    - {from: Oceania, to: Americas, bn: 8}
encoding: {source: from, target: to, y: bn}
axes:
  y: {label: bn USD}
```
````

## Fields that matter

- `type: chord` with `encoding.source` for the node a flow starts at, `encoding.target` for the node it ends at and `encoding.y` for its value; each ordered pair may have one row, so two directions between the same nodes are two ribbons, and a node may link to itself
- Each node is an arc around the circle, as long as the flow that leaves and enters it; arcs follow the order in which the rows first name the nodes
- A ribbon is as wide at each end as its flow and takes the colour of its source; the ends along an arc follow the circle, so routes to neighbours leave from the side that faces them
- Names stand outside the ring; a name that does not fit is left out and listed, and the legend appears in its place
- Rows without a node, with a missing or negative value, or with a value of zero are left out and counted
- A chord diagram takes at most 300 nodes and 2,000 links in a panel; above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- Arc sizes and shares are computed by RichView and marked as derived in the readout; `encoding.facet` makes one diagram per value

## Interactions

- Hover or focus an arc to read what the node sends, what it receives and its share of the circle.
- Hover or focus a ribbon to read its value and its share of all flows.
- Click a legend entry to remove a node and its ribbons, and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
