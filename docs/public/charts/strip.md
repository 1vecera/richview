---
title: Strip chart
description: Strip chart that draws every observation as a point, one strip per category.
styles: [scientific]
---

Use a strip chart to show every observation in each category, so the spread, the clusters and the ties stay visible.

```chart
type: strip
title: West scores bunch tightly; North and South spread wider
subtitle: One point per score; points that would overlap are spread sideways
data:
  inline:
    - {team: North, score: 62}
    - {team: North, score: 65}
    - {team: North, score: 65}
    - {team: North, score: 68}
    - {team: North, score: 70}
    - {team: North, score: 70}
    - {team: North, score: 70}
    - {team: North, score: 72}
    - {team: North, score: 75}
    - {team: North, score: 78}
    - {team: North, score: 81}
    - {team: North, score: 90}
    - {team: South, score: 55}
    - {team: South, score: 58}
    - {team: South, score: 60}
    - {team: South, score: 61}
    - {team: South, score: 63}
    - {team: South, score: 63}
    - {team: South, score: 66}
    - {team: South, score: 69}
    - {team: South, score: 74}
    - {team: South, score: 77}
    - {team: South, score: 80}
    - {team: South, score: 84}
    - {team: West, score: 70}
    - {team: West, score: 72}
    - {team: West, score: 73}
    - {team: West, score: 73}
    - {team: West, score: 74}
    - {team: West, score: 75}
    - {team: West, score: 75}
    - {team: West, score: 76}
    - {team: West, score: 78}
    - {team: West, score: 79}
    - {team: West, score: 83}
    - {team: West, score: 88}
encoding: {x: team, y: score}
axes:
  y: {label: Score}
```

## Source

````rmd
```chart
type: strip
title: West scores bunch tightly; North and South spread wider
subtitle: One point per score; points that would overlap are spread sideways
data:
  inline:
    - {team: North, score: 62}
    - {team: North, score: 65}
    - {team: North, score: 65}
    - {team: North, score: 68}
    - {team: North, score: 70}
    - {team: North, score: 70}
    - {team: North, score: 70}
    - {team: North, score: 72}
    - {team: North, score: 75}
    - {team: North, score: 78}
    - {team: North, score: 81}
    - {team: North, score: 90}
    - {team: South, score: 55}
    - {team: South, score: 58}
    - {team: South, score: 60}
    - {team: South, score: 61}
    - {team: South, score: 63}
    - {team: South, score: 63}
    - {team: South, score: 66}
    - {team: South, score: 69}
    - {team: South, score: 74}
    - {team: South, score: 77}
    - {team: South, score: 80}
    - {team: South, score: 84}
    - {team: West, score: 70}
    - {team: West, score: 72}
    - {team: West, score: 73}
    - {team: West, score: 73}
    - {team: West, score: 74}
    - {team: West, score: 75}
    - {team: West, score: 75}
    - {team: West, score: 76}
    - {team: West, score: 78}
    - {team: West, score: 79}
    - {team: West, score: 83}
    - {team: West, score: 88}
encoding: {x: team, y: score}
axes:
  y: {label: Score}
```
````

## Fields that matter

- `type: strip` with `encoding.y` for the numeric value, one point per row
- `encoding.x` names the category, one strip each; without it all points share one strip
- `encoding.color` places groups side by side within each category; `encoding.facet` splits the chart into panels
- Points that would overlap are spread sideways by a fixed beeswarm: values in order, each at the smallest offset that clears the earlier points; the same data always gives the same picture and the sideways position carries no value
- Readouts give the value as you wrote it, its category and its row
- `axes.y` sets `label`, `format` and `unit`; `style.orientation: horizontal` lays the strips out sideways

## Interactions

- Hover or focus a point to read its value, category and row.
- Click a legend entry to toggle a group when the chart has several.
- Open the Table view, Download PNG or CSV, or Expand the chart.
