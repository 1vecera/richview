---
title: Pivot table
description: Pivot table for sums by row and column, with row, column and grand totals.
styles: [scientific]
---

Use a pivot table to read one measure for every pair of a row field and a column field, with totals.

```chart
type: pivot
title: The West sells the most and every region grows into Q4
subtitle: Sales by region and quarter, illustrative figures
data:
  inline:
    - {region: North, quarter: Q1, sales: 120}
    - {region: North, quarter: Q2, sales: 135}
    - {region: North, quarter: Q3, sales: 150}
    - {region: North, quarter: Q4, sales: 190}
    - {region: South, quarter: Q1, sales: 95}
    - {region: South, quarter: Q2, sales: 110}
    - {region: South, quarter: Q3, sales: 105}
    - {region: South, quarter: Q4, sales: 140}
    - {region: East, quarter: Q1, sales: 80}
    - {region: East, quarter: Q2, sales: 90}
    - {region: East, quarter: Q3, sales: 120}
    - {region: East, quarter: Q4, sales: 100}
    - {region: East, quarter: Q4, sales: 35}
    - {region: West, quarter: Q1, sales: 150}
    - {region: West, quarter: Q2, sales: 140}
    - {region: West, quarter: Q3, sales: 160}
    - {region: West, quarter: Q4, sales: 210}
encoding: {rows: region, columns: quarter, values: sales}
style: {shade: true}
```

## Source

````rmd
```chart
type: pivot
title: The West sells the most and every region grows into Q4
subtitle: Sales by region and quarter, illustrative figures
data:
  inline:
    - {region: North, quarter: Q1, sales: 120}
    - {region: North, quarter: Q2, sales: 135}
    - {region: North, quarter: Q3, sales: 150}
    - {region: North, quarter: Q4, sales: 190}
    - {region: South, quarter: Q1, sales: 95}
    - {region: South, quarter: Q2, sales: 110}
    - {region: South, quarter: Q3, sales: 105}
    - {region: South, quarter: Q4, sales: 140}
    - {region: East, quarter: Q1, sales: 80}
    - {region: East, quarter: Q2, sales: 90}
    - {region: East, quarter: Q3, sales: 120}
    - {region: East, quarter: Q4, sales: 100}
    - {region: East, quarter: Q4, sales: 35}
    - {region: West, quarter: Q1, sales: 150}
    - {region: West, quarter: Q2, sales: 140}
    - {region: West, quarter: Q3, sales: 160}
    - {region: West, quarter: Q4, sales: 210}
encoding: {rows: region, columns: quarter, values: sales}
style: {shade: true}
```
````

## Fields that matter

- `type: pivot` with `encoding.rows` for the field down the side, `encoding.columns` for the field across the top and `encoding.values` for the number to sum
- Each cell is the sum of the rows that fall in it; a cell, a row total, a column total and the grand total are computed by RichView and marked as derived in the readout
- Rows and columns keep the order of the source; a pair with no row stays empty
- A pivot table takes at most 40 rows and 12 columns; above either the table is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count, so a total always covers every row shown
- A figure too narrow for the columns narrows them all alike; a number or header that no longer fits is reported as left out
- A row without a row name, a column name or a value is left out and reported; a value that is not a number is an error
- `style.shade: true` tints each cell along the style's ramp by its value
- `axes.y` sets the format and unit of every number in the table

## Interactions

- Hover or focus a cell to read its row, column and value; a dark outline traces the cell under the pointer.
- Open the Table view, Download CSV, or Expand the chart.
