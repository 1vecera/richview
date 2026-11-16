---
title: Venn diagram
description: Venn diagram for how two or three groups overlap, with circle areas that follow the sizes of the sets.
styles: [scientific]
---

Use a Venn diagram when the question is how many items two or three groups share. Use a bar chart for more groups.

```chart
type: venn
title: Web and email readers overlap the most
subtitle: Readers by channel, a row for each set and each overlap, illustrative counts
data:
  inline:
    - {channel: Email, readers: 620}
    - {channel: App, readers: 480}
    - {channel: Web, readers: 540}
    - {channel: "Email & App", readers: 180}
    - {channel: "Email & Web", readers: 210}
    - {channel: "App & Web", readers: 150}
    - {channel: "Email & App & Web", readers: 70}
encoding: {sets: channel, y: readers}
axes:
  y: {label: Readers}
```

## Source

````rmd
```chart
type: venn
title: Web and email readers overlap the most
subtitle: Readers by channel, a row for each set and each overlap, illustrative counts
data:
  inline:
    - {channel: Email, readers: 620}
    - {channel: App, readers: 480}
    - {channel: Web, readers: 540}
    - {channel: "Email & App", readers: 180}
    - {channel: "Email & Web", readers: 210}
    - {channel: "App & Web", readers: 150}
    - {channel: "Email & App & Web", readers: 70}
encoding: {sets: channel, y: readers}
axes:
  y: {label: Readers}
```
````

## Fields that matter

- `type: venn` with `encoding.sets` for the sets a row counts and `encoding.y` for its size
- A set is a row with one name and gives that set's total; an overlap is a row with two or three names joined by `&` or a comma, such as `Email & App`, and gives the size of that overlap, including whatever else lies in it; a list of names is read the same way
- Every set needs a row of its own; two or three sets are drawn, and four or more fail with a reason
- Circle areas are proportional to the totals and each pair is placed at the distance at which its overlap has the size given, so areas are exact for two sets, and for three sets only when the three pair overlaps fit together; when they do not, a warning says so and a note names the overlaps as drawn off size; the overlap of three is wherever the geometry puts it, and a note says so
- An overlap cannot be larger than a set or a smaller overlap it lies in; an overlap of zero draws the circles apart
- The shares in a readout are computed by RichView and marked as derived; a name that does not fit inside its circle is left to the legend and reported
- `encoding.facet` makes one diagram per value on one scale; `axes.y.label` names the size

## Interactions

- Hover or focus a circle or an overlap to read its size and the share of each set.
- Click a legend entry to remove a set and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
