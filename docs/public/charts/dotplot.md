---
title: Dot plot
description: Dot plot for counting every observation of a numeric field in equal-width intervals.
styles: [scientific]
---

Use a dot plot for a small sample, where each observation deserves its own mark and the gaps matter.

```chart
type: dotplot
title: Trams run to time, buses wander
data:
  inline:
    - { route: Tram, minutes: 14.8 }
    - { route: Tram, minutes: 13.2 }
    - { route: Tram, minutes: 15.1 }
    - { route: Tram, minutes: 15.3 }
    - { route: Tram, minutes: 16.6 }
    - { route: Tram, minutes: 13.4 }
    - { route: Tram, minutes: 12.5 }
    - { route: Tram, minutes: 12.5 }
    - { route: Bus, minutes: 15.6 }
    - { route: Bus, minutes: 19.8 }
    - { route: Bus, minutes: 16.5 }
    - { route: Bus, minutes: 24.8 }
    - { route: Bus, minutes: 11.6 }
    - { route: Bus, minutes: 15.1 }
    - { route: Bus, minutes: 15.7 }
    - { route: Bus, minutes: 10.6 }
encoding: { x: minutes, color: route }
axes:
  x: { label: Journey time (minutes) }
  y: { label: Trips }
```

## Source

````rmd
```chart
type: dotplot
title: Trams run to time, buses wander
data:
  inline:
    - { route: Tram, minutes: 14.8 }
    - { route: Tram, minutes: 13.2 }
    - { route: Tram, minutes: 15.1 }
    - { route: Tram, minutes: 15.3 }
    - { route: Tram, minutes: 16.6 }
    - { route: Tram, minutes: 13.4 }
    - { route: Tram, minutes: 12.5 }
    - { route: Tram, minutes: 12.5 }
    - { route: Bus, minutes: 15.6 }
    - { route: Bus, minutes: 19.8 }
    - { route: Bus, minutes: 16.5 }
    - { route: Bus, minutes: 24.8 }
    - { route: Bus, minutes: 11.6 }
    - { route: Bus, minutes: 15.1 }
    - { route: Bus, minutes: 15.7 }
    - { route: Bus, minutes: 10.6 }
encoding: { x: minutes, color: route }
axes:
  x: { label: Journey time (minutes) }
  y: { label: Trips }
```
````

## Fields that matter

- `type: dotplot` with `encoding.x` for the numeric field; each row is one observation and one dot
- `encoding.color` stacks one group on top of another within an interval
- The data are cut into 30 equal-width intervals from the smallest to the largest value, and a dot stands at the centre of its interval; the vertical axis counts observations
- Every panel of a faceted chart shares the same intervals
- `axes.x` and `axes.y` set `label` and, for the value axis, `format` and `unit`

## Interactions

- Hover or focus a column of dots to read its interval and the number of observations per group. Dark rings mark the column's dots under the pointer.
- Click a legend entry to hide a group when the chart has several.
- Open the Table view, Download PNG or CSV, or Expand the chart.
