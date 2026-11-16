---
title: 3-D bar chart
description: 3-D bar chart for a value over two categories, with bars as boxes on a grid seen from an angle.
styles: [scientific]
---

Use a 3-D bar chart to compare a value across two categories when the grid of pairs matters more than reading exact heights.

```chart
type: bar3d
title: Online leads every region except the South
subtitle: Revenue by region and channel, illustrative figures
data:
  inline:
    - {region: North, channel: Online, revenue: 52}
    - {region: North, channel: Retail, revenue: 38}
    - {region: North, channel: Partner, revenue: 21}
    - {region: South, channel: Online, revenue: 31}
    - {region: South, channel: Retail, revenue: 44}
    - {region: South, channel: Partner, revenue: 18}
    - {region: East, channel: Online, revenue: 47}
    - {region: East, channel: Retail, revenue: 35}
    - {region: East, channel: Partner, revenue: 26}
    - {region: West, channel: Online, revenue: 58}
    - {region: West, channel: Retail, revenue: 41}
    - {region: West, channel: Partner, revenue: 30}
encoding: {x: region, y: revenue, color: channel}
axes:
  x: {label: Region}
  y: {label: Revenue (EUR m)}
  color: {label: Channel}
```

## Source

````rmd
```chart
type: bar3d
title: Online leads every region except the South
subtitle: Revenue by region and channel, illustrative figures
data:
  inline:
    - {region: North, channel: Online, revenue: 52}
    - {region: North, channel: Retail, revenue: 38}
    - {region: North, channel: Partner, revenue: 21}
    - {region: South, channel: Online, revenue: 31}
    - {region: South, channel: Retail, revenue: 44}
    - {region: South, channel: Partner, revenue: 18}
    - {region: East, channel: Online, revenue: 47}
    - {region: East, channel: Retail, revenue: 35}
    - {region: East, channel: Partner, revenue: 26}
    - {region: West, channel: Online, revenue: 58}
    - {region: West, channel: Retail, revenue: 41}
    - {region: West, channel: Partner, revenue: 30}
encoding: {x: region, y: revenue, color: channel}
axes:
  x: {label: Region}
  y: {label: Revenue (EUR m)}
  color: {label: Channel}
```
````

## Fields that matter

- `type: bar3d` with `encoding.x` for the category along the front, `encoding.y` for the numeric value and an optional `encoding.color` for a second category in depth
- Without `encoding.color` there is one row of bars
- Bars stand on zero, so a negative value hangs below it; several rows for one pair are added up and the readout marks the sum as derived
- Each bar shows the three faces that face the viewer, the top lighter and the two sides shaded apart
- A 3-D bar chart takes at most 24 x categories and 12 groups; above either the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- `style.azimuth` (default 35) turns the view round the vertical axis and `style.elevation` (default 28) tilts it, both in degrees
- `axes.x`, `axes.y` and `axes.color` name and format the three axes; a category label that does not fit is left out and reported

## Interactions

- Hover or focus a bar to read its two categories and its value.
- Click a legend entry to remove a group and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
