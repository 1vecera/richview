---
title: Streamgraph
description: Streamgraph for how the parts of a changing total rise and fall, stacked around a moving centre line.
styles: [scientific]
---

Use a streamgraph to show how several groups share a total over time or along a number when the overall shape and the rise and fall of each part matter more than exact heights.

```chart
type: streamgraph
title: Pop swells in summer while rock fades
subtitle: Hours listened per month and genre
data:
  inline:
    - {month: 2025-01, genre: Rock, hours: 40}
    - {month: 2025-02, genre: Rock, hours: 49}
    - {month: 2025-03, genre: Rock, hours: 55}
    - {month: 2025-04, genre: Rock, hours: 58}
    - {month: 2025-05, genre: Rock, hours: 56}
    - {month: 2025-06, genre: Rock, hours: 51}
    - {month: 2025-07, genre: Rock, hours: 43}
    - {month: 2025-08, genre: Rock, hours: 34}
    - {month: 2025-09, genre: Rock, hours: 26}
    - {month: 2025-10, genre: Rock, hours: 22}
    - {month: 2025-11, genre: Rock, hours: 23}
    - {month: 2025-12, genre: Rock, hours: 27}
    - {month: 2025-01, genre: Jazz, hours: 36}
    - {month: 2025-02, genre: Jazz, hours: 31}
    - {month: 2025-03, genre: Jazz, hours: 25}
    - {month: 2025-04, genre: Jazz, hours: 20}
    - {month: 2025-05, genre: Jazz, hours: 16}
    - {month: 2025-06, genre: Jazz, hours: 14}
    - {month: 2025-07, genre: Jazz, hours: 15}
    - {month: 2025-08, genre: Jazz, hours: 19}
    - {month: 2025-09, genre: Jazz, hours: 25}
    - {month: 2025-10, genre: Jazz, hours: 31}
    - {month: 2025-11, genre: Jazz, hours: 36}
    - {month: 2025-12, genre: Jazz, hours: 38}
    - {month: 2025-01, genre: Pop, hours: 16}
    - {month: 2025-02, genre: Pop, hours: 12}
    - {month: 2025-03, genre: Pop, hours: 13}
    - {month: 2025-04, genre: Pop, hours: 19}
    - {month: 2025-05, genre: Pop, hours: 28}
    - {month: 2025-06, genre: Pop, hours: 38}
    - {month: 2025-07, genre: Pop, hours: 47}
    - {month: 2025-08, genre: Pop, hours: 51}
    - {month: 2025-09, genre: Pop, hours: 51}
    - {month: 2025-10, genre: Pop, hours: 47}
    - {month: 2025-11, genre: Pop, hours: 38}
    - {month: 2025-12, genre: Pop, hours: 29}
encoding: {x: month, y: hours, color: genre}
axes:
  y: {label: Hours listened}
```

## Source

````rmd
```chart
type: streamgraph
title: Pop swells in summer while rock fades
subtitle: Hours listened per month and genre
data:
  inline:
    - {month: 2025-01, genre: Rock, hours: 40}
    - {month: 2025-02, genre: Rock, hours: 49}
    - {month: 2025-03, genre: Rock, hours: 55}
    - {month: 2025-04, genre: Rock, hours: 58}
    - {month: 2025-05, genre: Rock, hours: 56}
    - {month: 2025-06, genre: Rock, hours: 51}
    - {month: 2025-07, genre: Rock, hours: 43}
    - {month: 2025-08, genre: Rock, hours: 34}
    - {month: 2025-09, genre: Rock, hours: 26}
    - {month: 2025-10, genre: Rock, hours: 22}
    - {month: 2025-11, genre: Rock, hours: 23}
    - {month: 2025-12, genre: Rock, hours: 27}
    - {month: 2025-01, genre: Jazz, hours: 36}
    - {month: 2025-02, genre: Jazz, hours: 31}
    - {month: 2025-03, genre: Jazz, hours: 25}
    - {month: 2025-04, genre: Jazz, hours: 20}
    - {month: 2025-05, genre: Jazz, hours: 16}
    - {month: 2025-06, genre: Jazz, hours: 14}
    - {month: 2025-07, genre: Jazz, hours: 15}
    - {month: 2025-08, genre: Jazz, hours: 19}
    - {month: 2025-09, genre: Jazz, hours: 25}
    - {month: 2025-10, genre: Jazz, hours: 31}
    - {month: 2025-11, genre: Jazz, hours: 36}
    - {month: 2025-12, genre: Jazz, hours: 38}
    - {month: 2025-01, genre: Pop, hours: 16}
    - {month: 2025-02, genre: Pop, hours: 12}
    - {month: 2025-03, genre: Pop, hours: 13}
    - {month: 2025-04, genre: Pop, hours: 19}
    - {month: 2025-05, genre: Pop, hours: 28}
    - {month: 2025-06, genre: Pop, hours: 38}
    - {month: 2025-07, genre: Pop, hours: 47}
    - {month: 2025-08, genre: Pop, hours: 51}
    - {month: 2025-09, genre: Pop, hours: 51}
    - {month: 2025-10, genre: Pop, hours: 47}
    - {month: 2025-11, genre: Pop, hours: 38}
    - {month: 2025-12, genre: Pop, hours: 29}
encoding: {x: month, y: hours, color: genre}
axes:
  y: {label: Hours listened}
```
````

## Fields that matter

- `type: streamgraph` with `encoding.x` for a date or a number, `encoding.y` for the value and `encoding.color` for one stream per group
- Streams stack in the order the groups first appear, which is the legend order, and the stack at each x is centred on zero; the thickness of a stream is its value, so there is a scale bar instead of a value axis
- A value cannot be negative, and a group may appear once per x
- A group with no row at an x another group has is drawn at zero width there and disclosed; RichView never joins across it
- A stream is named inside its widest stretch when the name fits whole, and the legend names every stream
- `style.curve` and `style.curvature` set how streams connect; `encoding.facet` makes one streamgraph per value

## Interactions

- Hover or focus the chart to read every group's value and the total at the nearest x. The stream under the pointer keeps its colour and the others fade, and a dot with a ring marks that stream's upper edge, as it does on a stacked area.
- Click a legend entry to remove a stream from the stack and back.
- Zoom and pan on the x axis with brush, wheel, pinch, keys or double-click; Escape or reset returns to the full range.
- Open the Table view, Download PNG or CSV, or Expand the chart.
