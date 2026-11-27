---
title: Waterfall chart
description: Waterfall chart for a total built from signed steps, with starts and totals marked by the author.
styles: [scientific]
---

Use a waterfall chart to show how a start value becomes an end value through a series of additions and subtractions.

```chart
type: waterfall
title: Profit is three quarters of revenue after a grant
subtitle: Thousands of dollars; each bar starts where the last one ended
data:
  inline:
    - step: Sales
      amount: 120
      kind: start
    - step: Costs
      amount: -45.5
    - step: Tax
      amount: -14.5
    - step: Grant
      amount: 30
    - step: Profit
      amount: 90
      kind: total
encoding: {x: step, y: amount, measure: kind}
axes:
  y: {label: Amount, unit: k}
```

## Source

````rmd
```chart
type: waterfall
title: Profit is three quarters of revenue after a grant
subtitle: Thousands of dollars; each bar starts where the last one ended
data:
  inline:
    - step: Sales
      amount: 120
      kind: start
    - step: Costs
      amount: -45.5
    - step: Tax
      amount: -14.5
    - step: Grant
      amount: 30
    - step: Profit
      amount: 90
      kind: total
encoding: {x: step, y: amount, measure: kind}
axes:
  y: {label: Amount, unit: k}
```
````

## Fields that matter

- `type: waterfall` with `encoding.x` for the step name and `encoding.y` for its signed amount
- `encoding.measure` names the field that marks a row: `start` (also `absolute`, `reset`) begins from that value, `total` (also `subtotal`) ends a run, and `delta` (also `relative`, `change`) or an empty cell is a change added to the running sum; any other value is an error
- A total that disagrees with the sum of the steps is reported, not replaced
- Each step prints its change beyond its bar; `auto_annotations: false` leaves the values to the readout
- `axes.y` sets `label`, `format`, `unit`, `min` and `max`
- `style.ratio` sets the width to height ratio, and `style.facet_scale: free` gives each facet its own value scale

## Interactions

- Hover or focus a bar to read the step's change and the running total it leads to. A dark outline marks the bar under the pointer.
- Open the Table view, Download PNG or CSV, or Expand the chart.
