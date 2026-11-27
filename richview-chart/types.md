# Chart types (FEA-CH-01, FEA-CH-75, FEA-CH-76)

The list RichView draws: every chart type the major libraries define (ggplot2, Plotly, ECharts,
Highcharts, Power BI), grouped by what they have in common in the geometry model. A variant that
only changes a setting of another type (spline, stacked, 100 %, donut, column vs bar, area range
as `band`) is an option of that type, not a type. Each type below gets the full contract of
FEA-CH-75; the status column says where it stands.

Names are the `type:` value. Channels are `encoding` keys; `x`, `y`, `color`, `size`, `label`,
`facet`, `lower`, `upper` exist today, the rest are new vocabulary that core's registry lists.

## Existing (nine)

line, area, bar, scatter (bubbles through `size`), pie, heatmap, histogram, boxplot, interval.

Covered by an option of an existing type, no new type:

| Library name | RichView |
| --- | --- |
| spline, areaspline, step (Highcharts, ECharts) | `line`/`area` with `style.curve` |
| column, bar (Highcharts), clustered/stacked/100 % column and bar (Power BI) | `bar` with `style.orientation`, `stack`, `stack: percent` |
| donut, doughnut (all) | `pie` with `style.inner` |
| bubble (Plotly, Highcharts, Power BI) | `scatter` with `encoding.size` |
| errorbar, crossbar, linerange, pointrange (ggplot2, Highcharts) | `interval` |
| columnrange (Highcharts) | `interval` drawn as bars (`style.mark: bar`) |
| filled / shape map (Power BI) | `map` |
| card, KPI, scorecard (Power BI), indicator number (Plotly) | the `metric` block |
| table, matrix (Power BI, Plotly) | `table` block and `pivot` |

## New types

Tier 1 reuses the category and value frame (axis, names, grid) and costs the least. Tier 2 are
free-form 2-D layouts. Tier 3 need their own layout algorithm. Tier 4 are the large elements and
3-D.

| Type | Seen in | Channels | Tier |
| --- | --- | --- | --- |
| `waterfall` | Plotly, Highcharts, Power BI | `x`, `y` (signed step), `color` | 1 |
| `lollipop` | Highcharts | `x`, `y`, `color` | done |
| `dumbbell` | Highcharts, ggplot2 segment | `x`, `lower`, `upper`, `color` | done |
| `funnel` | all five (Highcharts pyramid, Plotly funnelarea as a style) | `x`, `y` | done |
| `bullet` | Highcharts, Plotly indicator | `x`, `y`, `target`, `lower`, `upper` | done |
| `pareto` | Highcharts | `x`, `y` | done |
| `combo` | Power BI line and column | `x`, `y`, `line` (second measure) | done |
| `band` | ggplot2 ribbon, Highcharts arearange | `x`, `lower`, `upper`, `color` | done |
| `candlestick` | Plotly, ECharts, Highcharts | `x`, `open`, `high`, `low`, `close` | done |
| `ohlc` | Plotly, Highcharts | `x`, `open`, `high`, `low`, `close` | done |
| `violin` | ggplot2, Plotly | `y`, `x`, `color` | 2 |
| `density` | ggplot2, Highcharts bellcurve | `x`, `color` | done |
| `violin` | ggplot2, Plotly | `y`, `x`, `color` | done |
| `density` | ggplot2, Highcharts bellcurve | `x`, `color` | 2 |
| `ecdf` | ggplot2 | `x`, `color` | 2 |
| `density` | ggplot2, Highcharts bellcurve | `x`, `color` | 2 |
| `ecdf` | ggplot2 | `x`, `color` | done |
| `qq` | ggplot2 | `y`, `color` | done |
| `dotplot` | ggplot2 | `x`, `color` | 2 |
| `strip` (jitter, beeswarm) | ggplot2, Plotly box points | `y`, `x`, `color` | done |
| `dotplot` | ggplot2 | `x`, `color` | done |
| `strip` (jitter, beeswarm) | ggplot2, Plotly box points | `y`, `x`, `color` | 2 |
| `bin2d` | ggplot2, Plotly histogram2d | `x`, `y` | done |
| `hexbin` | ggplot2 | `x`, `y` | done |
| `contour` | ggplot2, Plotly, ECharts | `x`, `y`, `z` (grid) | done |
| `radar` | ECharts, Plotly scatterpolar | `x` (axis), `y`, `color` | done |
| `rose` (polar bar) | Plotly barpolar, ECharts | `x`, `y`, `color` | done |
| `gauge` | ECharts, Highcharts, Plotly, Power BI | `y`, bands in `style` | done |
| `parallel` | Plotly parcoords, ECharts | several `y` fields in `dimensions` | done |
| `splom` | Plotly | `dimensions`, `color` | done |
| `calendar` | ECharts | `x` (date), `color` | done |
| `streamgraph` | Highcharts, ECharts themeRiver | `x`, `y`, `color` | done |
| `ribbon` | Power BI ribbon chart | `x`, `y`, `color` | done |
| `waffle` (pictorial, item) | ECharts, Highcharts item | `x`, `y` | done |
| `treemap` | all five | `parent`, `label`, `value`, `color` | 2 |
| `sunburst` | Plotly, ECharts, Highcharts | `parent`, `label`, `value` | done |
| `icicle` | Plotly | `parent`, `label`, `value` | done |
| `packed-bubble` (circle packing) | Highcharts | `label`, `value`, `color` | 3 |
| `gantt` (timeline, xrange) | Highcharts, Plotly | `label`, `start`, `end`, `color` | done |
| `packed-bubble` (circle packing) | Highcharts | `label`, `value`, `color` | done |
| `gantt` (timeline, xrange) | Highcharts, Plotly | `label`, `start`, `end`, `color` | 2 |
| `wordcloud` | Highcharts | `label`, `value` | done |
| `venn` | Highcharts | `sets`, `value` | 3 |
| `sankey` | Plotly, ECharts, Highcharts | `source`, `target`, `value` |done |
| `alluvial` | Plotly parcats | `dimensions`, `value` |done |
| `chord` (dependency wheel) | ECharts, Highcharts | `source`, `target`, `value` |done |
| `venn` | Highcharts | `sets`, `value` | done |
| `sankey` | Plotly, ECharts, Highcharts | `source`, `target`, `value` | 3 |
| `alluvial` | Plotly parcats | `dimensions`, `value` | 3 |
| `chord` (dependency wheel) | ECharts, Highcharts | `source`, `target`, `value` | 3 |
| `tree` (dendrogram, organization chart) | ECharts, Highcharts | `parent`, `label` | done |
| `network` (force graph) | ECharts graph, Highcharts networkgraph | `source`, `target`, `value` |done |
| `vector` (field, wind barb) | Highcharts, ggplot2 spoke | `x`, `y`, `angle`, `value` | 3 |
| `network` (force graph) | ECharts graph, Highcharts networkgraph | `source`, `target`, `value` | 3 |
| `vector` (field, wind barb) | Highcharts, ggplot2 spoke | `x`, `y`, `angle`, `value` | done |
| `map` | Plotly geo, ECharts, Highcharts, Power BI | `region` or `lat`, `lon`; `value`; `color` | done (countries; regions below country are not shipped yet) |
| `flow` | NovaFlow, Graphviz dot, Mermaid flowchart | written as `nodes`, `edges`, `groups`; from data: `label` (step), `source`, `target`, `lane`, `y` (connection label) | done |
| `pivot` | Power BI matrix | `rows`, `columns`, `values` | done |
| `surface` | Plotly, Highcharts 3-D | `x`, `y`, `z` | done |
| `scatter3d` | Plotly, Highcharts 3-D | `x`, `y`, `z`, `color`, `size` | done |
| `bar3d` | Plotly, Highcharts 3-D | `x`, `y`, `color` | done |

That is 9 existing plus 47 new, 56 in all; map, flow and pivot are elements rather than plots.

## Order of work

1. Table-driven type registration in the engine; generic channels.
2. Tier 1, `waterfall` first to set the pattern, then the rest in parallel.
3. Tier 2 free-form layouts, `treemap` first.
4. Tier 3 and tier 4 one at a time.

## Decisions taken on assumption

- Variants that only change a setting stay options of the base type.
- 3-D types draw an orthographic projection into the same 2-D items, so vector output and checks
  stay the same; the reader drags to rotate.
- Boundary data for `map` comes from Natural Earth (public domain), simplified and shipped with
  RichView (ARC-11, ARC-68).
