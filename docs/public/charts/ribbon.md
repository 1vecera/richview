---
title: Ribbon chart
description: Ribbon chart for how groups rank in each category, with ribbons that follow each group from one category to the next.
styles: [scientific]
---

Use a ribbon chart to show how groups rank within each category, such as month, and when that ranking changes.

```chart
type: ribbon
title: South overtakes North in March and East follows in April
subtitle: Sales per month and region, ranked in each month
data:
  inline:
    - {month: Jan, region: North, sales: 50}
    - {month: Feb, region: North, sales: 48}
    - {month: Mar, region: North, sales: 44}
    - {month: Apr, region: North, sales: 39}
    - {month: May, region: North, sales: 35}
    - {month: Jan, region: South, sales: 42}
    - {month: Feb, region: South, sales: 47}
    - {month: Mar, region: South, sales: 51}
    - {month: Apr, region: South, sales: 55}
    - {month: May, region: South, sales: 58}
    - {month: Jan, region: East, sales: 35}
    - {month: Feb, region: East, sales: 33}
    - {month: Mar, region: East, sales: 38}
    - {month: Apr, region: East, sales: 46}
    - {month: May, region: East, sales: 52}
    - {month: Jan, region: West, sales: 20}
    - {month: Feb, region: West, sales: 28}
    - {month: Mar, region: West, sales: 30}
    - {month: Apr, region: West, sales: 32}
    - {month: May, region: West, sales: 41}
encoding: {x: month, y: sales, color: region}
axes:
  y: {label: Sales}
```

## Source

````rmd
```chart
type: ribbon
title: South overtakes North in March and East follows in April
subtitle: Sales per month and region, ranked in each month
data:
  inline:
    - {month: Jan, region: North, sales: 50}
    - {month: Feb, region: North, sales: 48}
    - {month: Mar, region: North, sales: 44}
    - {month: Apr, region: North, sales: 39}
    - {month: May, region: North, sales: 35}
    - {month: Jan, region: South, sales: 42}
    - {month: Feb, region: South, sales: 47}
    - {month: Mar, region: South, sales: 51}
    - {month: Apr, region: South, sales: 55}
    - {month: May, region: South, sales: 58}
    - {month: Jan, region: East, sales: 35}
    - {month: Feb, region: East, sales: 33}
    - {month: Mar, region: East, sales: 38}
    - {month: Apr, region: East, sales: 46}
    - {month: May, region: East, sales: 52}
    - {month: Jan, region: West, sales: 20}
    - {month: Feb, region: West, sales: 28}
    - {month: Mar, region: West, sales: 30}
    - {month: Apr, region: West, sales: 32}
    - {month: May, region: West, sales: 41}
encoding: {x: month, y: sales, color: region}
axes:
  y: {label: Sales}
```
````

## Fields that matter

- `type: ribbon` with `encoding.x` for the category, `encoding.y` for the value and `encoding.color` for the group
- Categories keep the order of the source and run left to right; in each one the groups are stacked by value, the largest on top
- A ribbon joins a group's segment to its segment in the next category; a group with no row in a category, or a value of zero, has no ribbon into or out of it
- A value cannot be negative, and each category and group may have one row per panel
- Values are printed in the segments that have room; rank and share in the readout are computed by RichView and marked as derived
- `encoding.facet` makes one ribbon chart per value; `axes.y` sets `label`, `format`, `unit`, `min` and `max`

## Interactions

- Hover or focus a segment to read its value, rank and share.
- Click a legend entry to remove a group from the stack and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
