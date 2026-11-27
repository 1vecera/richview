---
title: Network diagram
description: Network diagram for who is linked to whom, with nodes sized by their links and lines between them.
styles: [scientific]
---

Use a network diagram to show how a set of things are linked, such as people who message each other, when the pattern of links matters more than any one value.

```chart
type: network
title: Dee and Gus link the most people in the team
subtitle: Messages sent between colleagues, synthetic
data:
  inline:
    - {from: Ada, to: Ben, msgs: 12}
    - {from: Ada, to: Cai, msgs: 8}
    - {from: Ada, to: Dee, msgs: 15}
    - {from: Ben, to: Cai, msgs: 5}
    - {from: Ben, to: Eli, msgs: 9}
    - {from: Cai, to: Dee, msgs: 6}
    - {from: Dee, to: Fay, msgs: 11}
    - {from: Eli, to: Fay, msgs: 4}
    - {from: Fay, to: Gus, msgs: 7}
    - {from: Gus, to: Hana, msgs: 10}
    - {from: Hana, to: Ivo, msgs: 3}
    - {from: Ivo, to: Jo, msgs: 6}
    - {from: Gus, to: Ivo, msgs: 2}
    - {from: Dee, to: Gus, msgs: 5}
    - {from: Kim, to: Lee, msgs: 9}
encoding: {source: from, target: to, y: msgs}
axes:
  y: {label: Messages}
```

## Source

````rmd
```chart
type: network
title: Dee and Gus link the most people in the team
subtitle: Messages sent between colleagues, synthetic
data:
  inline:
    - {from: Ada, to: Ben, msgs: 12}
    - {from: Ada, to: Cai, msgs: 8}
    - {from: Ada, to: Dee, msgs: 15}
    - {from: Ben, to: Cai, msgs: 5}
    - {from: Ben, to: Eli, msgs: 9}
    - {from: Cai, to: Dee, msgs: 6}
    - {from: Dee, to: Fay, msgs: 11}
    - {from: Eli, to: Fay, msgs: 4}
    - {from: Fay, to: Gus, msgs: 7}
    - {from: Gus, to: Hana, msgs: 10}
    - {from: Hana, to: Ivo, msgs: 3}
    - {from: Ivo, to: Jo, msgs: 6}
    - {from: Gus, to: Ivo, msgs: 2}
    - {from: Dee, to: Gus, msgs: 5}
    - {from: Kim, to: Lee, msgs: 9}
encoding: {source: from, target: to, y: msgs}
axes:
  y: {label: Messages}
```
````

## Fields that matter

- `type: network` with `encoding.source` and `encoding.target` for the two ends of a link, and an optional `encoding.y` that widens the line; without it every line has one width
- A node is as large as the number of links it has; the layout is a fixed force relaxation that starts from the order of the rows, so the same rows always give the same picture, and a position carries no value
- Nodes that are joined take one colour, and each separate part takes the next; the legend names the parts
- Each ordered pair of nodes may have one row; a link from a node to itself, and rows without a node or with a negative value, are left out and counted; a missing value is a link without a weight
- A network takes at most 400 nodes and 2,000 links in a panel; above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- Names stand beside their nodes, most linked first; a name that finds no clear spot is left out and listed
- Link counts and totals are computed by RichView and marked as derived in the readout; `encoding.facet` makes one network per value

## Interactions

- Hover or focus a node to read its number of links and the total on them. A dark ring marks the node under the pointer.
- Hover or focus the middle of a line to read its value, or the two nodes it joins.
- Click a legend entry to remove a connected part, and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
