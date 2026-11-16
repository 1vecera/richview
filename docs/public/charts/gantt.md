---
title: Gantt chart
description: Gantt chart for a plan, with one bar per task from its start to its end on a time axis.
styles: [scientific]
---

Use a Gantt chart to show when each task of a plan runs and where tasks overlap.

```chart
type: gantt
title: The build overlaps the design and the launch lands on 1 July
subtitle: Planned tasks by phase, illustrative dates
data:
  inline:
    - {task: Research, phase: Plan, start: 2026-01-05, end: 2026-02-13}
    - {task: Wireframes, phase: Plan, start: 2026-02-02, end: 2026-03-06}
    - {task: Prototype, phase: Build, start: 2026-03-02, end: 2026-04-10}
    - {task: Build API, phase: Build, start: 2026-03-16, end: 2026-05-29}
    - {task: Build app, phase: Build, start: 2026-04-13, end: 2026-06-19}
    - {task: Docs, phase: Ship, start: 2026-05-18, end: 2026-06-12}
    - {task: Testing, phase: Ship, start: 2026-06-01, end: 2026-06-26}
    - {task: Launch, phase: Ship, start: 2026-07-01, end: 2026-07-01}
encoding: {label: task, start: start, end: end, color: phase}
```

## Source

````rmd
```chart
type: gantt
title: The build overlaps the design and the launch lands on 1 July
subtitle: Planned tasks by phase, illustrative dates
data:
  inline:
    - {task: Research, phase: Plan, start: 2026-01-05, end: 2026-02-13}
    - {task: Wireframes, phase: Plan, start: 2026-02-02, end: 2026-03-06}
    - {task: Prototype, phase: Build, start: 2026-03-02, end: 2026-04-10}
    - {task: Build API, phase: Build, start: 2026-03-16, end: 2026-05-29}
    - {task: Build app, phase: Build, start: 2026-04-13, end: 2026-06-19}
    - {task: Docs, phase: Ship, start: 2026-05-18, end: 2026-06-12}
    - {task: Testing, phase: Ship, start: 2026-06-01, end: 2026-06-26}
    - {task: Launch, phase: Ship, start: 2026-07-01, end: 2026-07-01}
encoding: {label: task, start: start, end: end, color: phase}
```
````

## Fields that matter

- `type: gantt` with `encoding.label` for the task, `encoding.start` and `encoding.end` for its bounds, and optional `encoding.color` for a group such as a phase
- Starts and ends are ISO dates or numbers, and both must be the same kind; the time axis follows the data
- Tasks keep the order of the source, from top to bottom; RichView does not sort them or move them
- A task that starts and ends at the same instant is a milestone, drawn as a diamond; a task that ends before it starts is an error
- A row without a label, a start or an end is left out and reported
- A panel takes at most 60 tasks; above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- The duration in a readout is computed by RichView and marked as derived
- `encoding.facet` makes one Gantt chart per value on a shared time axis; `axes.x.label` titles the time axis

## Interactions

- Hover or focus a bar to read the task, its start, end and duration.
- Click a legend entry to remove a group of tasks and back.
- Zoom the time axis to look at part of the plan.
- Open the Table view, Download PNG or CSV, or Expand the chart.
