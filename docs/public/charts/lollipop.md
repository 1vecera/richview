---
title: Lollipop chart
description: Lollipop chart for values per category, drawn with a stem and a dot instead of a bar.
styles: [scientific]
---

Use a lollipop chart for the comparisons a bar chart makes when many bars would carry too much ink.

```chart
type: lollipop
title: North grew while South lost orders
subtitle: Orders per region and year
data:
  inline:
    - {region: North, year: "2025", orders: 120}
    - {region: North, year: "2026", orders: 150}
    - {region: South, year: "2025", orders: 90}
    - {region: South, year: "2026", orders: 70}
    - {region: East, year: "2025", orders: -20}
    - {region: East, year: "2026", orders: 45}
    - {region: West, year: "2025", orders: 60}
    - {region: West, year: "2026", orders: 65}
encoding: {x: region, y: orders, color: year}
axes:
  y: {label: Orders}
```

## Source

````rmd
```chart
type: lollipop
title: North grew while South lost orders
subtitle: Orders per region and year
data:
  inline:
    - {region: North, year: "2025", orders: 120}
    - {region: North, year: "2026", orders: 150}
    - {region: South, year: "2025", orders: 90}
    - {region: South, year: "2026", orders: 70}
    - {region: East, year: "2025", orders: -20}
    - {region: East, year: "2026", orders: 45}
    - {region: West, year: "2025", orders: 60}
    - {region: West, year: "2026", orders: 65}
encoding: {x: region, y: orders, color: year}
axes:
  y: {label: Orders}
```
````

## Fields that matter

- `type: lollipop` with `encoding.x` for the category and `encoding.y` for the value
- `encoding.color` groups the dots side by side within each category
- Stems start at the zero baseline, and negative values hang below it
- One lollipop per category prints its value beyond the dot; `auto_annotations: false` leaves the values to the readout
- A repeated category and group is rejected
- `axes.y` sets `label`, `format`, `unit`, `min` and `max`; `style.ratio` sets the width to height ratio

## Interactions

- Hover or focus a dot to read its value; the pointer picks the stem or dot it is over.
- Click a legend entry to toggle a series when the chart has several.
- Open the Table view, Download PNG or CSV, or Expand the chart.
