# Chart motion

Chart motion explains structure and change: an entrance shows how the data is built, a transition keeps each mark identifiable while values, scales or windows change, and hover feedback confirms what the reader is pointing at. Nothing moves without one of these reasons. Motion policy (curves, durations, staggering, interruption) belongs to `richview-chart`; hosts schedule frames and apply the results. Reduced motion is a hard floor, and every export shows the settled state.

This document specifies the motion model. [Status](#status) lists where the current runtime differs; [Measuring](#measuring) records the reference behaviour that informed the values.

Motion belongs to the data. Blocks of the report (text, headings, metrics, callouts, tables, media) are at rest on the first paint; one that starts below the first screen fades up once, softly, as it comes into view (FEA-RP-35), and reduced motion, print and `enabled: false` leave every block at rest. The report style's `motion` section sets the character of a style's motion (`character`), the block entrance (`block-entrance`: `none`, `fade` or `fade-up`, with `distance`), and the `duration`, `easing` and `stagger` of arrivals; `enabled: false` renders everything settled. `delay` is retired, ignored with a `style.retired` warning. Chart transitions use the values below.

## Curves

| Name | Definition | Use |
| --- | --- | --- |
| `out` | `1 − (1 − t)³`; CSS `cubic-bezier(0.33, 1, 0.68, 1)` | Anything that appears or answers the pointer: hover, dimming, the value readout, report blocks |
| `move` | `cubic-bezier(0.4, 0, 0.2, 1)` | On-screen movement of marks and axes: data updates, series toggles, zoom, reset, story steps, and chart entrances |

`out` moves on the first frame and settles softly (2 / 50 / 98 % of the change at 0.7 / 21 / 73 % of the duration). `move` eases in briefly and is front-loaded (9 / 35 / 82 %). Data marks never use bounce, back or elastic curves: every intermediate frame is a valid chart, so no mark passes its final value. A chart's entrance uses the style's `easing` only when that curve starts at rest and never passes its end (`standard`, `snap`); any other style curve gives way to `move` for the marks, and the style keeps its duration and stagger.

## Entrances

A report chart mounts when it comes within one screen of the viewport, one chart per animation frame, from its static frame (see [Settled state and exports](#settled-state-and-exports)). A document that gets no animation frames, as a hidden one can, still mounts its charts one at a time: a chart whose frame has not come after 250 ms mounts from a timer. A chart that mounts below the screen waits in its entrance state and plays its entrance once, when half of it is in view, or half of the screen for a chart taller than that. Charts on the first screen, and charts that mount on or above the screen, keep their painted state: no chart replays, and scrolling back never meets an entrance. Repaints (hover, resizing, a source chart replacing its marks) continue an entrance where it is and never restart it. Axes, gridlines, titles and legends have no entrance: they are the frame the data enters. The chart surface never fades as a whole.

| Mark | Entrance |
| --- | --- |
| Line | A front sweeps from the left edge to the right in 700 ms; the line ends at the front. Isolated observations pop (scale 0.5 → 1 and opacity) in 120 ms `out` as the sweep reaches them. |
| Area, stream and flow | Shares the sweep: fills fade in over the 48 px behind the front, which runs 48 px past the right edge so the fill ends whole. |
| Column and bar | Grows from the zero baseline (negative values downwards or leftwards) in 500 ms. Bars start 40 ms apart in reading order; a longer row spreads its starts evenly over at most 320 ms. |
| Tiles and cells | Marks without a baseline (heatmap, treemap, waffle, bin2d, hexbin and calendar cells, boxes, funnel stages, Gantt bars) grow from their own centre in 500 ms, in a wave from the first tile at the plot's top left to the last, one stagger step per distinct column and row of tiles, under the same 320 ms cap. A mark inside a larger one (a treemap group's tiles, a box's median) grows with it, from its centre. A centred stack (the funnel) widens from its axis, so its stages stay joined. |
| Pie and donut | Fans out clockwise from 12 o'clock in 700 ms: all slice angles scale by the same progress, so proportions are correct in every frame. The radius does not change. |
| Scatter and bubble | Points settle (opacity 0 → 1, radius 0.5r → r) in 400 ms; a hollow point (no fill, or the background inside a ring) fades in place, so its ring never crosses its own hollow. |
| Labels and annotations | Data, slice and end labels and annotations fade in from the moment the last mark has covered 85 % of its travel until it lands, so the entrance ends with them. Labels never move independently of their marks. |

From 2000 marks a chart reveals as a whole: lines and aggregated mean/range bands keep the sweep; points, bars and tiles use one 300 ms fade.

## Hover and focus

Pointer emphasis applies under `(hover: hover) and (pointer: fine)`. Touch focuses with a tap; keyboard traversal shows the same emphasis. Emphasis changes transform, opacity and colour only; it never moves layout.

| Element | Behaviour |
| --- | --- |
| Crosshair and hover dot | Snap to the nearest observation on the next frame. The dot grows from scale 0 to 1 in 120 ms `out`: radius 4.5 px, a 1.5 px ring in the chart background colour and the treatment's dot shadow. |
| Targeted series | When one series is targeted (legend entry, end label, nearest-series hover), its stroke gains 1 px in 120 ms `out` and the other series dim to opacity 0.3 in 150 ms `out`, restoring in 250 ms `out`. Shared readouts that list every series dim nothing. |
| Value readout | Appears without delay (opacity 0 → 1, 100 ms `out`). Between observations it glides to the new anchor in 120 ms `out`, retargeting from its current position; its content changes immediately and values are never tweened. On exit it waits 300 ms, then fades over 150 ms. |
| Pie slice | Moves 6 px outward along its mid-angle in 120 ms `out`; its radius and area stay exact. Other slices dim to 0.3. |
| Column and bar | Brightens 12 % in 120 ms `out`. Bars of the same series stay unchanged; other series dim to 0.3. |
| Scatter point | Radius × 1.25 in 120 ms `out`, with the dot shadow. |

Shadows are one neutral token per treatment, used only on floating elements (the readout and the hover dot): the light treatments (sleek-light, scientific) use `0 2px 8px oklch(0 0 0 / 0.12)` on the readout and `0 1px 3px oklch(0 0 0 / 0.14)` on the dot; sleek dark uses surface colour and border instead; brutalist and hand-drawn use none. Lines, areas and bars never carry shadows or glows.

Touch and pointer focus follow the same rules. A tap (a touch that moves less than 8 px and lifts within 500 ms) focuses the observation under it and keeps its readout until a second tap on it, a tap outside the chart, Escape or scrolling the chart out of view; a finger held on the plot scrubs. Leaving the chart with a mouse or pen, or pressing Escape, releases the readout and any series emphasis. Lifting a finger never hides a tapped readout.

## Updates and zoom

| Change | Behaviour |
| --- | --- |
| Data update | Marks keyed by series and observation move to their new geometry in 450 ms `move`, with axes interpolating on the same timeline. Exiting marks fade in 200 ms `out`; entering marks then fade and grow in 300 ms `out`. |
| Series toggle | A hidden series fades out in 150 ms `out`, then the value domain re-fits in 400 ms `move`; showing a series reverses the order. |
| Zoom and reset | Brush, buttons, keys, double-click, Escape and reset animate window and value domain together in 450 ms `move`. The window interpolates in log span about the fixed point, so the focal point stays still. |
| Wheel, trackpad, pinch | Each event retargets a 120 ms `out` glide from the current window; dragging pans 1:1 with the pointer. |
| Story steps | Zoom about the focal point in 800 ms `move`, axes in sync; annotations follow when the zoom settles. |

The plot rectangle interpolates between the start and end layouts of a transition, so margins never jump mid-transition when tick labels change width. Ticks travel with the scale; entering and exiting ticks cross-fade over 150 ms. Every transition retargets from the rendered state when interrupted.

## Reduced motion

`prefers-reduced-motion: reduce` or a style with `motion.enabled: false` renders the settled state on the first frame: no sweeps, growth, pops, glides, slice offsets or zoom tweens. Colour and opacity emphasis remain and apply instantly. Story steps use cuts and 150 ms cross-fades. The runtime follows changes of the media query while the page is open.

## Settled state and exports

Each report chart is delivered with two static frames, drawn by the same layout code for the phone column (358 px, a 390 px screen less its gutters) and for the theme's desktop column (its report width less the gutters: 832 px in every builtin theme). A container query shows the frame nearest the figure's width, so the page without JavaScript shows legible chart text at both widths. The chart styles ship once per page; a downloaded or standalone SVG carries them inline. When the runtime mounts, it paints its first frame out of the layout and adopts the static frame when the two are the same picture: the same width and, for a time series, the same plot rectangle under the browser's measured text metrics. Otherwise the static frame fades out over the runtime frame in 150 ms `out` while the canvas eases to its height; the figure never jumps.

`toSVG()`, the Download SVG control and every export render the end state of the current interaction, never an intermediate frame or an entrance. The page exposes its readiness as `window.__richview`:

| Member | Meaning |
| --- | --- |
| `output` | The output the page renders for: `report` (interactive) unless the document or chart payloads name another; chartless PDF pages also report `pdf`. |
| `settled()` | Mounts every remaining chart in its settled state, finishes running entrances and resolves with `{charts: {mounted, painted}, fonts, output}` once fonts are loaded and every chart is mounted, painted and at rest. It rejects after 30 s, naming the pending work. |
| `pending()` | Labels of the work still pending (`fonts`, `mount rv-chart-N`, `failed mount rv-chart-N`, `paint rv-chart-N`, `entrance rv-chart-N`, `cross-fade`). A failed mount remains pending, so it cannot be reported as a successful export. |

Output modes other than the interactive report mount every chart in its settled state. `beforeprint` and print media synchronously mount and paint every remaining chart, including facet children, before the browser takes its print snapshot. Entrance styles apply to screens only, so print never shows an entrance frame. Capture tools wait for `settled()` rather than for a fixed delay. Two captures of the same report are identical.

## Performance

The transition budget at 720 px with three 24-point series is at most 4 ms of script per frame (p95) and no frame over 16.7 ms, including with 4× CPU throttling. Report charts mount one per animation frame and paint in the following frame, spreading layout and DOM work across frames. Most entrances use transform, clip and opacity; pie entrances update slice paths. The load target is no long animation frame over 50 ms at 1× CPU. Initial page and font layout still exceeds this target on the flagship; lazy chart mounting does not remove that work.

## Status

The runtime implements the curves, entrances, hover and focus timing, reduced motion and the settled state above. It differs from this model as follows; each item is a measured behaviour.

- Data updates re-render instantly. During entrance, a source chart retains its existing marks when a relayout emits identical SVG, so the animation starts once per mark. The temporary SVG comparison buffer is released at settlement. A real width change settles an active entrance immediately; subsequent data or geometry changes replace the source SVG.
- Zoom and re-fit transitions take 620 ms with `out`; reset re-fits the value axis every frame in steps and jumps at the end; wheel, key and pinch zoom jump. The left margin follows the widest tick label of each frame, so the plot edge jumps by up to 8 px during a transition.
- A targeted series does not gain stroke width, a hovered bar does not brighten and a hovered scatter point does not grow. No treatment draws the readout or dot shadow yet: the light treatments need a shadow token.
- Annotations fade in with the labels; the hand-drawn treatment's strokes do not draw on.
- Line markers and isolated observations are revealed by the sweep's front rather than popping as it reaches them.
- Time-series static frames are laid out with an estimated advance of 0.6 em for the chart face. A face with another advance (Space Mono in brutalist, 0.612 em) moves the runtime's plot by a few pixels, so those charts take the 150 ms cross-fade at mount instead of being adopted.

## Measuring

The sampler that measured the SVG chart engine against these references (`scripts/design-review/motion.mjs`) was removed with that engine; the WebGL report runtime has no motion measurement yet. The reference engines were local, nonproduction evaluation only.

Measured reference behaviour that informed these values (Highcharts 13.1.1 and ECharts 6.1.0 at their defaults):

| Behaviour | Highcharts | ECharts |
| --- | --- | --- |
| Entrance | 1000 ms, sine in-out; line clip sweep; bars grow together; pie fans out and grows in radius | 1000 ms, cubic in-out; line clip sweep linear; line symbols pop for 200 ms, delayed by x; pie fans out |
| Update, toggle, zoom | 500 ms, sine in-out | 500 ms, cubic in-out; wheel steps 100 ms, cubic out |
| Hover in / out | 150 ms / 500 ms; marker radius + 2 px; halo 10 px at 0.25; other series to 0.2 | 300 ms / 300 ms, cubic out, one frame after the event; symbol × 1.1; dimming to 0.1 only with `emphasis.focus` |
| Readout | appears instantly, moves in about 300 ms, hides after 500 ms over 150 ms | fades in over 200 ms, moves in 400 ms `cubic-bezier(0.23, 1, 0.32, 1)`, hides after 100 ms over 200 ms |
| Reduced motion | not honoured | not honoured |

Sources: Highcharts API reference, [`plotOptions.series.animation`](https://api.highcharts.com/highcharts/plotOptions.series.animation), [`chart.animation`](https://api.highcharts.com/highcharts/chart.animation), [`plotOptions.series.states.hover`](https://api.highcharts.com/highcharts/plotOptions.series.states.hover), [`plotOptions.series.states.inactive`](https://api.highcharts.com/highcharts/plotOptions.series.states.inactive), [`plotOptions.series.marker.states.hover`](https://api.highcharts.com/highcharts/plotOptions.series.marker.states.hover), [`tooltip.animation`](https://api.highcharts.com/highcharts/tooltip.animation) and [`tooltip.hideDelay`](https://api.highcharts.com/highcharts/tooltip.hideDelay). Apache ECharts 6.1.0 source (Apache-2.0), read to understand behaviour: `lib/model/globalDefault.js` (animation and state-transition defaults, `animationThreshold`), `lib/chart/line/LineSeries.js` and `LineView.js` (linear clip reveal, symbol timing), `lib/chart/pie/PieSeries.js` and `PieView.js` (expansion), `lib/chart/helper/Symbol.js` (emphasis scale), `lib/component/tooltip/TooltipModel.js` and `TooltipHTMLContent.js` (readout timing and curve), `lib/component/dataZoom/roams.js` (wheel glide). Further principles: Jeffrey Heer and George Robertson, "Animated Transitions in Statistical Data Graphics" (IEEE InfoVis 2007), on staging and valid intermediate frames; Mike Bostock, "Object Constancy" (2012); WCAG 2.2 Success Criterion 2.3.3, Animation from Interactions. RichView implements its own motion in Rust; no reference code is used.
