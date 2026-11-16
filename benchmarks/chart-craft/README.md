# Chart craft laboratory

Fifty chart-local reports cover line, area, bar, scatter/bubble, pie and heatmap, changing calendar spans, missing observations, signed bars, long labels, density and five calendar conventions. Each report owns its folder and inline YAML data/loading definition. Values are synthetic. [REVIEW.md](REVIEW.md) records the iterations, validation and remaining gaps of the October 2026 study.

The `reports/` folder is what remains of the laboratory: `scripts/design-review/cases.json` and `scripts/verify-chart-review.mjs` read it. The comparison pages, capture, verification and Figma transfer scripts drove the SVG chart engine against local Highcharts, ECharts, Plotly and ggplot2 references. They were removed with that engine and are in the Git history. A comparison with the WebGL report runtime needs new harnesses.

Four visual treatments are `sleek-light`, `sleek-dark`, `scientific` and `brutalist`. US calendar labels are the default; `axes.x.locale` supports `en-US`, `en-GB`, `cs-CZ`, `de-DE` and `ja-JP`, in UTC. This is a bounded convention set, not full internationalization.

Highcharts is local nonproduction evaluation only; ECharts uses Apache-2.0, Plotly MIT, ggplot2 MIT and R GPL. Reference libraries never became a production dependency, and no license purchase, public reference-package distribution or deployment is part of this study.
