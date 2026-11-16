---
title: Radar chart
description: Radar chart for several measures of one or more groups on spokes of one scale.
styles: [scientific]
---

Use a radar chart to compare groups across a handful of measures that share a scale.

```chart
type: radar
title: Beta is steadier and cheaper, Alpha is faster
subtitle: Score per skill and team
data:
  inline:
    - {skill: Speed, team: Alpha, score: 80}
    - {skill: Reliability, team: Alpha, score: 65}
    - {skill: Cost, team: Alpha, score: 40}
    - {skill: Docs, team: Alpha, score: 70}
    - {skill: Support, team: Alpha, score: 55}
    - {skill: Speed, team: Beta, score: 50}
    - {skill: Reliability, team: Beta, score: 90}
    - {skill: Cost, team: Beta, score: 75}
    - {skill: Docs, team: Beta, score: 45}
    - {skill: Support, team: Beta, score: 85}
encoding: {x: skill, y: score, color: team}
axes:
  y: {label: Score}
```

## Source

````rmd
```chart
type: radar
title: Beta is steadier and cheaper, Alpha is faster
subtitle: Score per skill and team
data:
  inline:
    - {skill: Speed, team: Alpha, score: 80}
    - {skill: Reliability, team: Alpha, score: 65}
    - {skill: Cost, team: Alpha, score: 40}
    - {skill: Docs, team: Alpha, score: 70}
    - {skill: Support, team: Alpha, score: 55}
    - {skill: Speed, team: Beta, score: 50}
    - {skill: Reliability, team: Beta, score: 90}
    - {skill: Cost, team: Beta, score: 75}
    - {skill: Docs, team: Beta, score: 45}
    - {skill: Support, team: Beta, score: 85}
encoding: {x: skill, y: score, color: team}
axes:
  y: {label: Score}
```
````

## Fields that matter

- `type: radar` with `encoding.x` for the axis (spoke) name, `encoding.y` for the value and `encoding.color` for one polygon per group
- At least three axes are needed, and a repeated axis is rejected
- A missing value breaks the polygon at its spoke and is disclosed; it is never joined across
- All spokes share one scale; `axes.y` sets `label`, `format`, `unit`, `min` and `max`, and values outside a fixed scale are drawn at its edge and counted
- `encoding.facet` makes one radar per value; `style.ratio` sets the width to height ratio and `style.facet_scale: free` gives each facet its own scale

## Interactions

- Hover or focus a point to read its value; the pointer picks the nearest point within reach.
- Click a legend entry to toggle a series when the chart has several.
- Open the Table view, Download PNG or CSV, or Expand the chart.
