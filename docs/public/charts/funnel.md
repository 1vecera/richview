---
title: Funnel chart
description: Funnel chart for stages in order, each as wide as its value, with the share kept between stages.
styles: [scientific]
---

Use a funnel chart for stages that lose part of the quantity at each step, such as a signup flow.

```chart
type: funnel
title: Four in ten visitors sign up and one in twenty subscribes
subtitle: Users at each stage, from the first visit to a paid subscription
data:
  inline:
    - stage: Visited
      users: 12000
    - stage: Signed up
      users: 4800
    - stage: Activated
      users: 2900
    - stage: Subscribed
      users: 640
encoding: {x: stage, y: users}
axes:
  y: {label: Users}
```

## Source

````rmd
```chart
type: funnel
title: Four in ten visitors sign up and one in twenty subscribes
subtitle: Users at each stage, from the first visit to a paid subscription
data:
  inline:
    - stage: Visited
      users: 12000
    - stage: Signed up
      users: 4800
    - stage: Activated
      users: 2900
    - stage: Subscribed
      users: 640
encoding: {x: stage, y: users}
axes:
  y: {label: Users}
```
````

## Fields that matter

- `type: funnel` with `encoding.x` for the stage name and `encoding.y` for its value
- Stages run from top to bottom in source order
- There is no value axis; names stand at the left and values at the right of each band; `auto_annotations: false` leaves the values to the readout and gives their room to the stages
- A negative value, or text that is not a number, fails the chart instead of being drawn
- `encoding.facet` makes one funnel per value; `style.ratio` sets the width to height ratio and `style.facet_scale: free` gives each facet its own width scale

## Interactions

- Hover or focus a stage to read its value and its share; the pointer picks the band it is inside.
- Open the Table view, Download PNG or CSV, or Expand the chart.
