---
title: Map
description: Map of countries coloured by value, or points placed by latitude and longitude.
styles: [scientific]
---

Use a map when the place is the point: countries coloured by a value, or points at known coordinates.

```chart
type: map
title: Broadband reaches nine in ten households in much of Europe
subtitle: Share of households; darker is higher
data:
  inline:
    - {country: Czechia, share: 91}
    - {country: Germany, share: 92}
    - {country: France, share: 86}
    - {country: Poland, share: 88}
    - {country: Spain, share: 96}
    - {country: Italy, share: 85}
    - {country: Norway, share: 98}
    - {country: Turkey, share: 90}
    - {country: Egypt, share: 74}
    - {country: India, share: 52}
encoding: {region: country, y: share}
axes:
  y: {label: Households with broadband, unit: "%"}
```

## Source

````rmd
```chart
type: map
title: Broadband reaches nine in ten households in much of Europe
subtitle: Share of households; darker is higher
data:
  inline:
    - {country: Czechia, share: 91}
    - {country: Germany, share: 92}
    - {country: France, share: 86}
    - {country: Poland, share: 88}
    - {country: Spain, share: 96}
    - {country: Italy, share: 85}
    - {country: Norway, share: 98}
    - {country: Turkey, share: 90}
    - {country: Egypt, share: 74}
    - {country: India, share: 52}
encoding: {region: country, y: share}
axes:
  y: {label: Households with broadband, unit: "%"}
```
````

## Fields that matter

- `type: map` with `encoding.region` for a country name or ISO code and `encoding.y` for the value that colours it
- Or `encoding.lat` and `encoding.lon` for points, with `encoding.size` for dot size and `encoding.color` for groups; a map needs one of the two
- A region the data does not name stays plain, and a name that matches no boundary is reported
- `style.projection` is `natural-earth` (default), `equal-earth`, `mercator` or `equirectangular`
- `axes.y` sets `label`, `format` and `unit` for the value; `encoding.facet` makes one panel per value; `style.ratio` sets the width to height ratio

## Interactions

- Hover or focus a country or a point to read its value; the pointer picks the shape it is inside.
- Click a legend entry to toggle a group when points are coloured by group.
- Open the Table view, Download PNG or CSV, or Expand the chart.
