---
title: Calendar chart
description: Calendar chart for one value per day, in week columns grouped by month.
styles: [scientific]
---

Use a calendar chart to show a daily value over weeks and months, such as activity, sales or readings, where weekly rhythm and gaps matter.

```chart
type: calendar
title: Commits come in bursts and stop on Sundays
subtitle: Commits per day, 10 February to 20 March 2025
data:
  inline:
    - {day: 2025-02-10, commits: 3}
    - {day: 2025-02-11, commits: 8}
    - {day: 2025-02-12, commits: 1}
    - {day: 2025-02-13, commits: 6}
    - {day: 2025-02-14, commits: 11}
    - {day: 2025-02-15, commits: 4}
    - {day: 2025-02-17, commits: 2}
    - {day: 2025-02-18, commits: 7}
    - {day: 2025-02-19, commits: 0}
    - {day: 2025-02-20, commits: 5}
    - {day: 2025-02-21, commits: 10}
    - {day: 2025-02-22, commits: 3}
    - {day: 2025-02-24, commits: 1}
    - {day: 2025-02-25, commits: 6}
    - {day: 2025-02-26, commits: 11}
    - {day: 2025-02-27, commits: 4}
    - {day: 2025-02-28, commits: 9}
    - {day: 2025-03-01, commits: 2}
    - {day: 2025-03-03, commits: 0}
    - {day: 2025-03-04, commits: 5}
    - {day: 2025-03-05, commits: 10}
    - {day: 2025-03-06, commits: 3}
    - {day: 2025-03-07, commits: 8}
    - {day: 2025-03-08, commits: 1}
    - {day: 2025-03-10, commits: 11}
    - {day: 2025-03-11, commits: 4}
    - {day: 2025-03-12, commits: 9}
    - {day: 2025-03-13, commits: 2}
    - {day: 2025-03-14, commits: 7}
    - {day: 2025-03-15, commits: 0}
    - {day: 2025-03-17, commits: 10}
    - {day: 2025-03-18, commits: 3}
    - {day: 2025-03-19, commits: 8}
    - {day: 2025-03-20, commits: 1}
encoding: {x: day, y: commits}
axes:
  y: {label: Commits}
```

## Source

````rmd
```chart
type: calendar
title: Commits come in bursts and stop on Sundays
subtitle: Commits per day, 10 February to 20 March 2025
data:
  inline:
    - {day: 2025-02-10, commits: 3}
    - {day: 2025-02-11, commits: 8}
    - {day: 2025-02-12, commits: 1}
    - {day: 2025-02-13, commits: 6}
    - {day: 2025-02-14, commits: 11}
    - {day: 2025-02-15, commits: 4}
    - {day: 2025-02-17, commits: 2}
    - {day: 2025-02-18, commits: 7}
    - {day: 2025-02-19, commits: 0}
    - {day: 2025-02-20, commits: 5}
    - {day: 2025-02-21, commits: 10}
    - {day: 2025-02-22, commits: 3}
    - {day: 2025-02-24, commits: 1}
    - {day: 2025-02-25, commits: 6}
    - {day: 2025-02-26, commits: 11}
    - {day: 2025-02-27, commits: 4}
    - {day: 2025-02-28, commits: 9}
    - {day: 2025-03-01, commits: 2}
    - {day: 2025-03-03, commits: 0}
    - {day: 2025-03-04, commits: 5}
    - {day: 2025-03-05, commits: 10}
    - {day: 2025-03-06, commits: 3}
    - {day: 2025-03-07, commits: 8}
    - {day: 2025-03-08, commits: 1}
    - {day: 2025-03-10, commits: 11}
    - {day: 2025-03-11, commits: 4}
    - {day: 2025-03-12, commits: 9}
    - {day: 2025-03-13, commits: 2}
    - {day: 2025-03-14, commits: 7}
    - {day: 2025-03-15, commits: 0}
    - {day: 2025-03-17, commits: 10}
    - {day: 2025-03-18, commits: 3}
    - {day: 2025-03-19, commits: 8}
    - {day: 2025-03-20, commits: 1}
encoding: {x: day, y: commits}
axes:
  y: {label: Commits}
```
````

## Fields that matter

- `type: calendar` with `encoding.x` for the date and `encoding.y` for the value coloured on the style's ramp
- Dates need a day (`2025-03-14`); a date may appear once per panel, and a calendar spans at most twelve years
- Days fall in week columns with Monday at the top; there is one strip for each calendar year, covering whole months from the first month of the data to the last, with each month labelled and outlined
- A day without a row is an empty outline, never a zero, and a key under the chart decodes the colour and names the empty outline
- Negative values take the style's second arm of the ramp
- `encoding.facet` makes one calendar per value, all on one colour scale; `axes.y` sets `label`, `format` and `unit`

## Interactions

- Hover or focus a day to read its date and value.
- Open the Table view, Download PNG or CSV, or Expand the chart.
