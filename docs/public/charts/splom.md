---
title: Scatterplot matrix
description: Scatterplot matrix for the relationships between several numeric fields, with a small scatterplot for every pair.
styles: [scientific]
---

Use a scatterplot matrix to look at every pair of several measures at once and to see which groups or relationships stand out.

```chart
type: splom
title: Gentoo penguins are heavier, with longer flippers and shallower bills
subtitle: Twelve penguins, every pair of four measurements
data:
  inline:
    - {species: Adelie, bill: 39, flipper: 181, mass: 3750, depth: 18.7}
    - {species: Adelie, bill: 40, flipper: 186, mass: 3800, depth: 18.0}
    - {species: Adelie, bill: 38, flipper: 190, mass: 3250, depth: 17.4}
    - {species: Adelie, bill: 36, flipper: 195, mass: 3450, depth: 19.3}
    - {species: Chinstrap, bill: 46, flipper: 192, mass: 3500, depth: 17.9}
    - {species: Chinstrap, bill: 50, flipper: 196, mass: 3900, depth: 19.5}
    - {species: Chinstrap, bill: 49, flipper: 193, mass: 3650, depth: 19.1}
    - {species: Chinstrap, bill: 51, flipper: 195, mass: 3750, depth: 18.7}
    - {species: Gentoo, bill: 46, flipper: 211, mass: 4500, depth: 13.7}
    - {species: Gentoo, bill: 48, flipper: 222, mass: 5400, depth: 14.3}
    - {species: Gentoo, bill: 50, flipper: 230, mass: 5700, depth: 15.2}
    - {species: Gentoo, bill: 47, flipper: 215, mass: 4750, depth: 14.5}
encoding:
  dimensions: [bill, flipper, mass, depth]
  color: species
```

## Source

````rmd
```chart
type: splom
title: Gentoo penguins are heavier, with longer flippers and shallower bills
subtitle: Twelve penguins, every pair of four measurements
data:
  inline:
    - {species: Adelie, bill: 39, flipper: 181, mass: 3750, depth: 18.7}
    - {species: Adelie, bill: 40, flipper: 186, mass: 3800, depth: 18.0}
    - {species: Adelie, bill: 38, flipper: 190, mass: 3250, depth: 17.4}
    - {species: Adelie, bill: 36, flipper: 195, mass: 3450, depth: 19.3}
    - {species: Chinstrap, bill: 46, flipper: 192, mass: 3500, depth: 17.9}
    - {species: Chinstrap, bill: 50, flipper: 196, mass: 3900, depth: 19.5}
    - {species: Chinstrap, bill: 49, flipper: 193, mass: 3650, depth: 19.1}
    - {species: Chinstrap, bill: 51, flipper: 195, mass: 3750, depth: 18.7}
    - {species: Gentoo, bill: 46, flipper: 211, mass: 4500, depth: 13.7}
    - {species: Gentoo, bill: 48, flipper: 222, mass: 5400, depth: 14.3}
    - {species: Gentoo, bill: 50, flipper: 230, mass: 5700, depth: 15.2}
    - {species: Gentoo, bill: 47, flipper: 215, mass: 4750, depth: 14.5}
encoding:
  dimensions: [bill, flipper, mass, depth]
  color: species
```
````

## Fields that matter

- `type: splom` with `encoding.dimensions` listing two or more numeric columns; the grid has a row and a column for each
- Below the diagonal each cell plots one point per row, with the column's field across and the row's field up
- The diagonal names each field and gives its range; every cell of a field uses that range
- Above the diagonal each cell gives the Pearson correlation of its two fields, computed by RichView and marked as derived in the readout
- A scatterplot matrix takes at most six dimensions; above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- `encoding.color` colours the points by group and `encoding.label` names each row in its readout
- A row with a missing value in any field is left out and reported; a value that is not a number is an error
- Field names that do not fit their cell are left out and reported, so keep names short
- `encoding.facet` makes one grid per value, all on the same ranges unless `style.facet_scale` is `free`

## Interactions

- Hover or focus a point to read the row's values in that cell, or a diagonal or upper cell for the range or the correlation.
- Click a legend entry to remove a group and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
