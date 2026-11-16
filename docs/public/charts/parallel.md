---
title: Parallel coordinates chart
description: Parallel coordinates chart for comparing rows across several numeric measures, with one axis per measure and one line per row.
styles: [scientific]
---

Use parallel coordinates to compare many rows on several measures at once and to see which rows resemble each other.

```chart
type: parallel
title: Sports cars trade mileage for power and speed
subtitle: Nine cars on four measures, each axis from its own lowest to its highest value
data:
  inline:
    - {model: Mini, class: Small, mpg: 38, power: 85, weight: 1050, accel: 12.5}
    - {model: Zip, class: Small, mpg: 42, power: 75, weight: 980, accel: 13.8}
    - {model: Kei, class: Small, mpg: 35, power: 95, weight: 1120, accel: 11.9}
    - {model: Estate, class: Family, mpg: 28, power: 140, weight: 1550, accel: 9.5}
    - {model: Tourer, class: Family, mpg: 31, power: 125, weight: 1480, accel: 10.2}
    - {model: Sedan, class: Family, mpg: 26, power: 160, weight: 1700, accel: 8.8}
    - {model: Coupe, class: Sport, mpg: 19, power: 330, weight: 1450, accel: 4.8}
    - {model: Roadster, class: Sport, mpg: 22, power: 280, weight: 1380, accel: 5.4}
    - {model: GT, class: Sport, mpg: 17, power: 410, weight: 1500, accel: 3.9}
encoding:
  dimensions: [mpg, power, weight, accel]
  label: model
  color: class
```

## Source

````rmd
```chart
type: parallel
title: Sports cars trade mileage for power and speed
subtitle: Nine cars on four measures, each axis from its own lowest to its highest value
data:
  inline:
    - {model: Mini, class: Small, mpg: 38, power: 85, weight: 1050, accel: 12.5}
    - {model: Zip, class: Small, mpg: 42, power: 75, weight: 980, accel: 13.8}
    - {model: Kei, class: Small, mpg: 35, power: 95, weight: 1120, accel: 11.9}
    - {model: Estate, class: Family, mpg: 28, power: 140, weight: 1550, accel: 9.5}
    - {model: Tourer, class: Family, mpg: 31, power: 125, weight: 1480, accel: 10.2}
    - {model: Sedan, class: Family, mpg: 26, power: 160, weight: 1700, accel: 8.8}
    - {model: Coupe, class: Sport, mpg: 19, power: 330, weight: 1450, accel: 4.8}
    - {model: Roadster, class: Sport, mpg: 22, power: 280, weight: 1380, accel: 5.4}
    - {model: GT, class: Sport, mpg: 17, power: 410, weight: 1500, accel: 3.9}
encoding:
  dimensions: [mpg, power, weight, accel]
  label: model
  color: class
```
````

## Fields that matter

- `type: parallel` with `encoding.dimensions` listing two or more numeric columns, one axis each, in the order written
- Each axis runs from its own smallest to its own largest value, and its two ends are labelled with those values
- `encoding.color` colours the lines by group and `encoding.label` names each row in its readout
- A parallel coordinates chart takes at most eight axes; above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- A row with a missing value on any axis is left out and reported; a value that is not a number is an error
- Axis names and end labels that do not fit their space are left out and reported, so keep names short
- `encoding.facet` makes one chart per value, all on the same ranges unless `style.facet_scale` is `free`; `axes.y` sets `format`, `unit` and `digits` for the end labels

## Interactions

- Hover or focus the dot where a line meets the first axis to read the row's value on every axis.
- Click a legend entry to remove a group and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
