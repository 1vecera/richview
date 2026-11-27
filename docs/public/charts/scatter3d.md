---
title: 3-D scatterplot
description: 3-D scatterplot for three numeric fields in a box seen from an angle, with groups and sizes.
styles: [scientific]
---

Use a 3-D scatterplot to show how groups of points separate along three numeric fields.

```chart
type: scatter3d
title: Gentoo penguins stand apart in bill depth and flipper length
subtitle: Three measurements of 36 penguins, illustrative figures
data:
  inline:
    - {species: Adelie, bill_length: 38.3, bill_depth: 18.8, flipper: 189}
    - {species: Adelie, bill_length: 36.9, bill_depth: 18.1, flipper: 196}
    - {species: Adelie, bill_length: 40.9, bill_depth: 18.5, flipper: 192}
    - {species: Adelie, bill_length: 35.5, bill_depth: 19.1, flipper: 193}
    - {species: Adelie, bill_length: 35.4, bill_depth: 16.7, flipper: 186}
    - {species: Adelie, bill_length: 39.4, bill_depth: 18.3, flipper: 193}
    - {species: Adelie, bill_length: 39.4, bill_depth: 18.7, flipper: 187}
    - {species: Adelie, bill_length: 39.9, bill_depth: 19.4, flipper: 187}
    - {species: Adelie, bill_length: 38.1, bill_depth: 18.2, flipper: 193}
    - {species: Adelie, bill_length: 37.9, bill_depth: 17.4, flipper: 187}
    - {species: Adelie, bill_length: 37.2, bill_depth: 18.5, flipper: 192}
    - {species: Adelie, bill_length: 38.9, bill_depth: 19.5, flipper: 180}
    - {species: Chinstrap, bill_length: 48.6, bill_depth: 17.7, flipper: 198}
    - {species: Chinstrap, bill_length: 45.9, bill_depth: 19.1, flipper: 199}
    - {species: Chinstrap, bill_length: 51.7, bill_depth: 18.7, flipper: 197}
    - {species: Chinstrap, bill_length: 50.0, bill_depth: 17.8, flipper: 194}
    - {species: Chinstrap, bill_length: 46.9, bill_depth: 17.9, flipper: 202}
    - {species: Chinstrap, bill_length: 45.9, bill_depth: 18.6, flipper: 203}
    - {species: Chinstrap, bill_length: 45.0, bill_depth: 16.1, flipper: 198}
    - {species: Chinstrap, bill_length: 46.6, bill_depth: 19.3, flipper: 202}
    - {species: Chinstrap, bill_length: 49.3, bill_depth: 18.8, flipper: 204}
    - {species: Chinstrap, bill_length: 49.8, bill_depth: 18.9, flipper: 188}
    - {species: Chinstrap, bill_length: 50.7, bill_depth: 18.9, flipper: 186}
    - {species: Chinstrap, bill_length: 50.5, bill_depth: 16.8, flipper: 195}
    - {species: Gentoo, bill_length: 44.9, bill_depth: 16.4, flipper: 220}
    - {species: Gentoo, bill_length: 48.1, bill_depth: 15.6, flipper: 218}
    - {species: Gentoo, bill_length: 46.2, bill_depth: 14.6, flipper: 222}
    - {species: Gentoo, bill_length: 45.7, bill_depth: 15.9, flipper: 224}
    - {species: Gentoo, bill_length: 44.7, bill_depth: 14.9, flipper: 216}
    - {species: Gentoo, bill_length: 50.3, bill_depth: 14.1, flipper: 223}
    - {species: Gentoo, bill_length: 45.9, bill_depth: 15.6, flipper: 223}
    - {species: Gentoo, bill_length: 48.2, bill_depth: 15.1, flipper: 218}
    - {species: Gentoo, bill_length: 47.1, bill_depth: 15.2, flipper: 220}
    - {species: Gentoo, bill_length: 49.0, bill_depth: 15.5, flipper: 227}
    - {species: Gentoo, bill_length: 46.6, bill_depth: 14.7, flipper: 217}
    - {species: Gentoo, bill_length: 46.8, bill_depth: 15.3, flipper: 226}
encoding: {x: bill_length, y: bill_depth, z: flipper, color: species}
axes:
  x: {label: Bill length (mm)}
  y: {label: Bill depth (mm)}
  color: {label: Flipper (mm)}
```

## Source

````rmd
```chart
type: scatter3d
title: Gentoo penguins stand apart in bill depth and flipper length
subtitle: Three measurements of 36 penguins, illustrative figures
data:
  inline:
    - {species: Adelie, bill_length: 38.3, bill_depth: 18.8, flipper: 189}
    - {species: Adelie, bill_length: 36.9, bill_depth: 18.1, flipper: 196}
    - {species: Adelie, bill_length: 40.9, bill_depth: 18.5, flipper: 192}
    - {species: Adelie, bill_length: 35.5, bill_depth: 19.1, flipper: 193}
    - {species: Adelie, bill_length: 35.4, bill_depth: 16.7, flipper: 186}
    - {species: Adelie, bill_length: 39.4, bill_depth: 18.3, flipper: 193}
    - {species: Adelie, bill_length: 39.4, bill_depth: 18.7, flipper: 187}
    - {species: Adelie, bill_length: 39.9, bill_depth: 19.4, flipper: 187}
    - {species: Adelie, bill_length: 38.1, bill_depth: 18.2, flipper: 193}
    - {species: Adelie, bill_length: 37.9, bill_depth: 17.4, flipper: 187}
    - {species: Adelie, bill_length: 37.2, bill_depth: 18.5, flipper: 192}
    - {species: Adelie, bill_length: 38.9, bill_depth: 19.5, flipper: 180}
    - {species: Chinstrap, bill_length: 48.6, bill_depth: 17.7, flipper: 198}
    - {species: Chinstrap, bill_length: 45.9, bill_depth: 19.1, flipper: 199}
    - {species: Chinstrap, bill_length: 51.7, bill_depth: 18.7, flipper: 197}
    - {species: Chinstrap, bill_length: 50.0, bill_depth: 17.8, flipper: 194}
    - {species: Chinstrap, bill_length: 46.9, bill_depth: 17.9, flipper: 202}
    - {species: Chinstrap, bill_length: 45.9, bill_depth: 18.6, flipper: 203}
    - {species: Chinstrap, bill_length: 45.0, bill_depth: 16.1, flipper: 198}
    - {species: Chinstrap, bill_length: 46.6, bill_depth: 19.3, flipper: 202}
    - {species: Chinstrap, bill_length: 49.3, bill_depth: 18.8, flipper: 204}
    - {species: Chinstrap, bill_length: 49.8, bill_depth: 18.9, flipper: 188}
    - {species: Chinstrap, bill_length: 50.7, bill_depth: 18.9, flipper: 186}
    - {species: Chinstrap, bill_length: 50.5, bill_depth: 16.8, flipper: 195}
    - {species: Gentoo, bill_length: 44.9, bill_depth: 16.4, flipper: 220}
    - {species: Gentoo, bill_length: 48.1, bill_depth: 15.6, flipper: 218}
    - {species: Gentoo, bill_length: 46.2, bill_depth: 14.6, flipper: 222}
    - {species: Gentoo, bill_length: 45.7, bill_depth: 15.9, flipper: 224}
    - {species: Gentoo, bill_length: 44.7, bill_depth: 14.9, flipper: 216}
    - {species: Gentoo, bill_length: 50.3, bill_depth: 14.1, flipper: 223}
    - {species: Gentoo, bill_length: 45.9, bill_depth: 15.6, flipper: 223}
    - {species: Gentoo, bill_length: 48.2, bill_depth: 15.1, flipper: 218}
    - {species: Gentoo, bill_length: 47.1, bill_depth: 15.2, flipper: 220}
    - {species: Gentoo, bill_length: 49.0, bill_depth: 15.5, flipper: 227}
    - {species: Gentoo, bill_length: 46.6, bill_depth: 14.7, flipper: 217}
    - {species: Gentoo, bill_length: 46.8, bill_depth: 15.3, flipper: 226}
encoding: {x: bill_length, y: bill_depth, z: flipper, color: species}
axes:
  x: {label: Bill length (mm)}
  y: {label: Bill depth (mm)}
  color: {label: Flipper (mm)}
```
````

## Fields that matter

- `type: scatter3d` with numeric `encoding.x`, `encoding.y` and `encoding.z`, an optional `encoding.color` that splits the points into groups, an optional `encoding.size` and an optional `encoding.label` for the readout heading
- Dots are drawn far to near; without `encoding.size` a nearer dot is larger and more opaque, with it the radius shows the size by area and only the opacity follows depth
- Up to 80 points stand on a faint line to the floor so their position reads
- A chart takes at most 5,000 points; above that it is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- `style.azimuth` (default 35) turns the view round the vertical axis and `style.elevation` (default 28) tilts it, both in degrees; they are the view the chart opens at, and a reader turns it from there
- `axes.x`, `axes.y` and `axes.color` name and format the three axes
- A row without a coordinate or a group, or with a negative size, is left out and reported

## Interactions

- Hover or focus a dot to read its three coordinates, its group and its size. The dot grows and takes a ring in its own colour, as a scatter point does.
- Click a legend entry to remove a group and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.

## Turn the view

Drag the chart at the top of this page to turn it. The author's `style.azimuth` and `style.elevation` set the view it opens at, and a reader turns it from there.

- Drag with a mouse, a pen or a finger. Dragging right brings the near side to the right and dragging down looks from higher; the elevation stops at 5 and 85 degrees, and the azimuth goes round.
- Hold Shift and press an arrow key to turn it five degrees from the keyboard. Arrow keys alone still step through the dots.
- Double-click the chart, press R or choose Reset view to return to the author's view.
- Hover, focus and the readout work at every angle: the pointer names the dot drawn on top under it.
- The box keeps one size at every angle, so it does not grow or shrink while it turns, and its labels stay inside the chart; it moves a little to keep them there as they change sides. It is the largest size that fits from every angle, so a chart drawn at one angle is a little smaller than one fitted to that angle alone.
- Print, PDF, the command line's image export and a page without script always show the author's view. Download PNG saves the picture on the screen.
