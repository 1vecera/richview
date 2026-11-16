---
title: Pie chart
description: Pie chart for part-to-whole shares across a handful of categories.
styles: [scientific]
---

Use a pie chart for the share each of a few categories contributes to a whole.

```chart
type: pie
title: Where the engineering hour goes
data:
  inline:
    - { activity: coding, hours: 3.1 }
    - { activity: review, hours: 1.4 }
    - { activity: meetings, hours: 1.8 }
    - { activity: ops, hours: 0.9 }
encoding: { x: activity, y: hours }
```

## Source

````rmd
```chart
type: pie
title: Where the engineering hour goes
data:
  inline:
    - { activity: coding, hours: 3.1 }
    - { activity: review, hours: 1.4 }
    - { activity: meetings, hours: 1.8 }
    - { activity: ops, hours: 0.9 }
encoding: { x: activity, y: hours }
```
````

## Fields that matter

- `type: pie` with `encoding.x` for the category name and `encoding.y` for the value
- Slices follow the order of the rows; past eight slices the palette repeats and `richview check` warns (`chart.series-differentiation`), so merge small categories first
- `style.ratio` defaults to 1:1
- Labels sit around the disk when names and percentages fit; otherwise a wrapped key is used
- `chart.decals` in a style adds hatch patterns for slices and key swatches

## Interactions

- Hover or focus a slice, or a key entry, to read its value; the slice moves outward and the others dim.
- Tiny and zero shares stay inspectable through the key.
- Open the Table view, Download PNG or CSV, or Expand the chart.
