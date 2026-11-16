---
title: OHLC chart
description: OHLC chart for prices that open, peak, bottom and close in each period, drawn as a range line with a tick for the open and one for the close.
styles: [scientific]
---

Use an OHLC chart when each period has an open, a high, a low and a close and you want a lighter mark than a candlestick, such as daily prices over many periods.

```chart
type: ohlc
title: Prices climbed from 101 to 105 despite two dips
digits: 4
subtitle: Daily open, high, low and close in dollars
data:
  inline:
    - {day: '2026-03-02', open: 101.2, high: 103.8, low: 100.4, close: 103.1}
    - {day: '2026-03-03', open: 103.1, high: 104.6, low: 102.2, close: 102.6}
    - {day: '2026-03-04', open: 102.6, high: 102.9, low: 99.8, close: 100.3}
    - {day: '2026-03-05', open: 100.3, high: 101.7, low: 99.5, close: 101.4}
    - {day: '2026-03-06', open: 101.4, high: 104.2, low: 101.1, close: 103.9}
    - {day: '2026-03-09', open: 103.9, high: 106.0, low: 103.5, close: 105.2}
    - {day: '2026-03-10', open: 105.2, high: 105.8, low: 102.9, close: 103.4}
    - {day: '2026-03-11', open: 103.4, high: 104.1, low: 102.0, close: 103.8}
    - {day: '2026-03-12', open: 103.8, high: 107.1, low: 103.6, close: 106.5}
    - {day: '2026-03-13', open: 106.5, high: 107.4, low: 105.1, close: 105.3}
encoding: {x: day, open: open, high: high, low: low, close: close}
axes:
  y: {label: Price, unit: '$'}
```

## Source

````rmd
```chart
type: ohlc
title: Prices climbed from 101 to 105 despite two dips
digits: 4
subtitle: Daily open, high, low and close in dollars
data:
  inline:
    - {day: '2026-03-02', open: 101.2, high: 103.8, low: 100.4, close: 103.1}
    - {day: '2026-03-03', open: 103.1, high: 104.6, low: 102.2, close: 102.6}
    - {day: '2026-03-04', open: 102.6, high: 102.9, low: 99.8, close: 100.3}
    - {day: '2026-03-05', open: 100.3, high: 101.7, low: 99.5, close: 101.4}
    - {day: '2026-03-06', open: 101.4, high: 104.2, low: 101.1, close: 103.9}
    - {day: '2026-03-09', open: 103.9, high: 106.0, low: 103.5, close: 105.2}
    - {day: '2026-03-10', open: 105.2, high: 105.8, low: 102.9, close: 103.4}
    - {day: '2026-03-11', open: 103.4, high: 104.1, low: 102.0, close: 103.8}
    - {day: '2026-03-12', open: 103.8, high: 107.1, low: 103.6, close: 106.5}
    - {day: '2026-03-13', open: 106.5, high: 107.4, low: 105.1, close: 105.3}
encoding: {x: day, open: open, high: high, low: low, close: close}
axes:
  y: {label: Price, unit: '$'}
```
````

## Fields that matter

- `type: ohlc` with `encoding.x` for the period, kept in source order, and `encoding.open`, `encoding.high`, `encoding.low` and `encoding.close` for the four prices
- `encoding.facet` splits one panel per value, all on one price scale
- A row whose high is below its open or close, or whose low is above them, fails the chart and names the row; rows missing a price are left out and reported
- `axes.y` sets `label`, `unit` and `min` / `max`; the price axis does not have to include zero
- `digits` sets the precision of axis labels and readouts, so prices with decimals need it
- The last close is printed right of the plot at its own height, as a price tag; `auto_annotations: false` leaves it out and gives the plot that room
- `style.ratio` sets the width to height ratio (16:9 by default)

## Interactions

- The tick on the left of a bar marks the open and the tick on the right the close; the colour says whether the close was at or above the open (Up) or below it (Down).
- Hover or focus a bar to read Open, High, Low, Close and the change from open to close.
- Dense series thin the period names instead of overlapping them.
- Open the Table view, Download PNG or CSV, or Expand the chart.
