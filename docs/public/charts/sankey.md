---
title: Sankey diagram
description: Sankey diagram for flows from sources through stages to uses, with ribbons as wide as the flow they carry.
styles: [scientific]
---

Use a sankey diagram to show how a quantity splits and merges as it moves from where it starts to where it ends, such as energy, money or visitors.

```chart
type: sankey
title: Coal and gas feed most of the power that homes and industry use
subtitle: Energy flows in TWh, synthetic
data:
  inline:
    - {from: Coal, to: Power, twh: 35}
    - {from: Gas, to: Power, twh: 30}
    - {from: Gas, to: Heat, twh: 25}
    - {from: Wind, to: Power, twh: 20}
    - {from: Solar, to: Power, twh: 10}
    - {from: Power, to: Homes, twh: 45}
    - {from: Power, to: Industry, twh: 40}
    - {from: Power, to: Losses, twh: 10}
    - {from: Heat, to: Homes, twh: 15}
    - {from: Heat, to: Industry, twh: 10}
encoding: {source: from, target: to, y: twh}
axes:
  y: {label: TWh}
```

## Source

````rmd
```chart
type: sankey
title: Coal and gas feed most of the power that homes and industry use
subtitle: Energy flows in TWh, synthetic
data:
  inline:
    - {from: Coal, to: Power, twh: 35}
    - {from: Gas, to: Power, twh: 30}
    - {from: Gas, to: Heat, twh: 25}
    - {from: Wind, to: Power, twh: 20}
    - {from: Solar, to: Power, twh: 10}
    - {from: Power, to: Homes, twh: 45}
    - {from: Power, to: Industry, twh: 40}
    - {from: Power, to: Losses, twh: 10}
    - {from: Heat, to: Homes, twh: 15}
    - {from: Heat, to: Industry, twh: 10}
encoding: {source: from, target: to, y: twh}
axes:
  y: {label: TWh}
```
````

## Fields that matter

- `type: sankey` with `encoding.source` for the node a flow starts at, `encoding.target` for the node it ends at and `encoding.y` for its value; each pair of nodes may have one row
- A node stands in the column of its longest path from a source, and is as tall as the larger of what enters and what leaves it
- A ribbon is as wide as its flow; ribbons and nodes take the colour of the source that a node mostly receives from, and the legend names the sources
- A flow that points back and closes a cycle is left out with a finding, so what is drawn flows one way; rows without a node, with a missing or negative value, or with a value of zero are left out and counted
- A sankey takes at most 300 nodes and 2,000 links in a panel; above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- Node sizes, shares and the sides of a node are computed by RichView and marked as derived in the readout
- `encoding.facet` makes one diagram per value on a shared scale unless `style.facet_scale` is `free`; names that do not fit beside their node are left out and listed

## Interactions

- Hover or focus a node to read its size, what enters and leaves it and its share of the column. The node or ribbon under the pointer takes a dark outline, and the rest of the chart stays as it is.
- Hover or focus a ribbon to read its value and its share of its source and of its target.
- Point at a legend entry, or focus it with the keyboard, to outline that source's ribbons and nodes in the same dark outline. Nothing dims, where every other chart dims the other series: try it on the diagram above.
- Click a legend entry to remove a source and what it feeds, and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
