---
title: Bar chart
description: Bar chart for comparing values across discrete categories.
styles: [scientific]
---

Use a bar chart to compare a value across discrete categories, such as years, regions or products.

```chart
type: bar
title: Fieldwork's share of company mentions
data:
  inline:
    - { year: "2023", share: 3.0 }
    - { year: "2024", share: 18.2 }
    - { year: "2025", share: 42.8 }
    - { year: "2026*", share: 68.2 }
encoding: { x: year, y: share }
axes: { y: { label: Share of mentions (%) } }
```

## Source

````rmd
```chart
type: bar
title: Fieldwork's share of company mentions
data:
  inline:
    - { year: "2023", share: 3.0 }
    - { year: "2024", share: 18.2 }
    - { year: "2025", share: 42.8 }
    - { year: "2026*", share: 68.2 }
encoding: { x: year, y: share }
axes: { y: { label: Share of mentions (%) } }
```
````

## Fields that matter

- `type: bar` with `encoding.x` for the category and `encoding.y` for the value
- `encoding.color` groups bars by a second category
- `stack: true` stacks the groups of `encoding.color` instead of placing them side by side
- `style.orientation` is `auto` (default), `vertical` or `horizontal`
- `axes.y.label`, `format` and `unit` describe the value axis
- `axes.y.min` and `axes.y.max` fix the ends of the value axis
- Each category and series slot takes one observation; repeats are rejected

Values appear automatically when complete text fits inside its own bar, or beside an ordinary bar without crossing ink. Large stacked parts use their raw part value; tiny parts stay unlabelled. The text matches the primary readout at the author’s precision. `auto_annotations: false` turns the values off.

## Interactions

- Point at a bar to read its value. A ring outlines the bar in its own colour and the other bars keep their colour. Use the arrow keys to select a mark and see the same ring and readout.
- Click a legend entry to toggle a series when the chart has several.
- Open the Table view, Download PNG or CSV, or Expand the chart.
