---
title: Surface chart
description: Surface chart for a measure over two numeric inputs, seen from an angle and coloured by height.
styles: [scientific]
---

Use a surface chart to show how one measure varies over two numeric inputs and where it peaks.

```chart
type: surface
title: Yield peaks at moderate water and 80 kg of nitrogen
subtitle: Crop yield by water and nitrogen, illustrative figures
data:
  inline:
    - {water: 100, nitrogen: 0, yield: 2.2}
    - {water: 100, nitrogen: 20, yield: 2.5}
    - {water: 100, nitrogen: 40, yield: 2.8}
    - {water: 100, nitrogen: 60, yield: 3.1}
    - {water: 100, nitrogen: 80, yield: 3.3}
    - {water: 100, nitrogen: 100, yield: 3.1}
    - {water: 100, nitrogen: 120, yield: 2.8}
    - {water: 150, nitrogen: 0, yield: 2.5}
    - {water: 150, nitrogen: 20, yield: 3.1}
    - {water: 150, nitrogen: 40, yield: 3.9}
    - {water: 150, nitrogen: 60, yield: 4.7}
    - {water: 150, nitrogen: 80, yield: 5.0}
    - {water: 150, nitrogen: 100, yield: 4.7}
    - {water: 150, nitrogen: 120, yield: 3.9}
    - {water: 200, nitrogen: 0, yield: 2.9}
    - {water: 200, nitrogen: 20, yield: 3.9}
    - {water: 200, nitrogen: 40, yield: 5.2}
    - {water: 200, nitrogen: 60, yield: 6.5}
    - {water: 200, nitrogen: 80, yield: 7.0}
    - {water: 200, nitrogen: 100, yield: 6.5}
    - {water: 200, nitrogen: 120, yield: 5.2}
    - {water: 250, nitrogen: 0, yield: 3.0}
    - {water: 250, nitrogen: 20, yield: 4.2}
    - {water: 250, nitrogen: 40, yield: 5.8}
    - {water: 250, nitrogen: 60, yield: 7.4}
    - {water: 250, nitrogen: 80, yield: 8.0}
    - {water: 250, nitrogen: 100, yield: 7.4}
    - {water: 250, nitrogen: 120, yield: 5.8}
    - {water: 300, nitrogen: 0, yield: 2.9}
    - {water: 300, nitrogen: 20, yield: 3.9}
    - {water: 300, nitrogen: 40, yield: 5.2}
    - {water: 300, nitrogen: 60, yield: 6.5}
    - {water: 300, nitrogen: 80, yield: 7.0}
    - {water: 300, nitrogen: 100, yield: 6.5}
    - {water: 300, nitrogen: 120, yield: 5.2}
    - {water: 350, nitrogen: 0, yield: 2.5}
    - {water: 350, nitrogen: 20, yield: 3.1}
    - {water: 350, nitrogen: 40, yield: 3.9}
    - {water: 350, nitrogen: 60, yield: 4.7}
    - {water: 350, nitrogen: 80, yield: 5.0}
    - {water: 350, nitrogen: 100, yield: 4.7}
    - {water: 350, nitrogen: 120, yield: 3.9}
    - {water: 400, nitrogen: 0, yield: 2.2}
    - {water: 400, nitrogen: 20, yield: 2.5}
    - {water: 400, nitrogen: 40, yield: 2.8}
    - {water: 400, nitrogen: 60, yield: 3.1}
    - {water: 400, nitrogen: 80, yield: 3.3}
    - {water: 400, nitrogen: 100, yield: 3.1}
    - {water: 400, nitrogen: 120, yield: 2.8}
encoding: {x: water, y: nitrogen, z: yield}
axes:
  x: {label: Water (mm)}
  y: {label: Nitrogen (kg/ha)}
  color: {label: Yield (t/ha)}
```

## Source

````rmd
```chart
type: surface
title: Yield peaks at moderate water and 80 kg of nitrogen
subtitle: Crop yield by water and nitrogen, illustrative figures
data:
  inline:
    - {water: 100, nitrogen: 0, yield: 2.2}
    - {water: 100, nitrogen: 20, yield: 2.5}
    - {water: 100, nitrogen: 40, yield: 2.8}
    - {water: 100, nitrogen: 60, yield: 3.1}
    - {water: 100, nitrogen: 80, yield: 3.3}
    - {water: 100, nitrogen: 100, yield: 3.1}
    - {water: 100, nitrogen: 120, yield: 2.8}
    - {water: 150, nitrogen: 0, yield: 2.5}
    - {water: 150, nitrogen: 20, yield: 3.1}
    - {water: 150, nitrogen: 40, yield: 3.9}
    - {water: 150, nitrogen: 60, yield: 4.7}
    - {water: 150, nitrogen: 80, yield: 5.0}
    - {water: 150, nitrogen: 100, yield: 4.7}
    - {water: 150, nitrogen: 120, yield: 3.9}
    - {water: 200, nitrogen: 0, yield: 2.9}
    - {water: 200, nitrogen: 20, yield: 3.9}
    - {water: 200, nitrogen: 40, yield: 5.2}
    - {water: 200, nitrogen: 60, yield: 6.5}
    - {water: 200, nitrogen: 80, yield: 7.0}
    - {water: 200, nitrogen: 100, yield: 6.5}
    - {water: 200, nitrogen: 120, yield: 5.2}
    - {water: 250, nitrogen: 0, yield: 3.0}
    - {water: 250, nitrogen: 20, yield: 4.2}
    - {water: 250, nitrogen: 40, yield: 5.8}
    - {water: 250, nitrogen: 60, yield: 7.4}
    - {water: 250, nitrogen: 80, yield: 8.0}
    - {water: 250, nitrogen: 100, yield: 7.4}
    - {water: 250, nitrogen: 120, yield: 5.8}
    - {water: 300, nitrogen: 0, yield: 2.9}
    - {water: 300, nitrogen: 20, yield: 3.9}
    - {water: 300, nitrogen: 40, yield: 5.2}
    - {water: 300, nitrogen: 60, yield: 6.5}
    - {water: 300, nitrogen: 80, yield: 7.0}
    - {water: 300, nitrogen: 100, yield: 6.5}
    - {water: 300, nitrogen: 120, yield: 5.2}
    - {water: 350, nitrogen: 0, yield: 2.5}
    - {water: 350, nitrogen: 20, yield: 3.1}
    - {water: 350, nitrogen: 40, yield: 3.9}
    - {water: 350, nitrogen: 60, yield: 4.7}
    - {water: 350, nitrogen: 80, yield: 5.0}
    - {water: 350, nitrogen: 100, yield: 4.7}
    - {water: 350, nitrogen: 120, yield: 3.9}
    - {water: 400, nitrogen: 0, yield: 2.2}
    - {water: 400, nitrogen: 20, yield: 2.5}
    - {water: 400, nitrogen: 40, yield: 2.8}
    - {water: 400, nitrogen: 60, yield: 3.1}
    - {water: 400, nitrogen: 80, yield: 3.3}
    - {water: 400, nitrogen: 100, yield: 3.1}
    - {water: 400, nitrogen: 120, yield: 2.8}
encoding: {x: water, y: nitrogen, z: yield}
axes:
  x: {label: Water (mm)}
  y: {label: Nitrogen (kg/ha)}
  color: {label: Yield (t/ha)}
```
````

## Fields that matter

- `type: surface` with numeric `encoding.x`, `encoding.y` and `encoding.z`
- Rows that fill a complete grid are drawn as given; any other rows are interpolated onto a 24 by 24 grid by inverse-distance weighting, and the readout marks the values as derived
- A surface takes a complete grid of at most 100 nodes along each axis and at most 4,000 scattered rows; above either limit the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- Each cell is coloured by the mean z of its corners on the style's ramp, and a key under the chart decodes the colour
- `style.azimuth` (default 35) turns the view round the vertical axis and `style.elevation` (default 28) tilts it, both in degrees; they are the view the chart opens at, and a reader turns it from there
- `axes.x`, `axes.y` and `axes.color` name and format the three axes
- A row without an x, a y or a z is left out and reported; fewer than three rows, or rows that share one x or one y, say what is missing

## Interactions

- Hover or focus a cell to read its x range, y range and mean z; a dark outline traces the cell under the pointer.
- Open the Table view, Download CSV, or Expand the chart.

## Turn the view

Drag the chart at the top of this page to turn it. The author's `style.azimuth` and `style.elevation` set the view it opens at, and a reader turns it from there.

- Drag with a mouse, a pen or a finger. Dragging right brings the near side to the right and dragging down looks from higher; the elevation stops at 5 and 85 degrees, and the azimuth goes round.
- Hold Shift and press an arrow key to turn it five degrees from the keyboard. Arrow keys alone still step through the cells.
- Double-click the chart, press R or choose Reset view to return to the author's view.
- Hover, focus and the readout work at every angle: the pointer names the cell drawn on top under it.
- The box keeps one size at every angle, so it does not grow or shrink while it turns, and its labels stay inside the chart; it moves a little to keep them there as they change sides. It is the largest size that fits from every angle, so a chart drawn at one angle is a little smaller than one fitted to that angle alone.
- Print, PDF, the command line's image export and a page without script always show the author's view. Download PNG saves the picture on the screen.
