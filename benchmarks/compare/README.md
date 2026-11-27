# Library-comparison lab (ARC-64)

Renders every chart type of `examples/every-block/report.rmd` that Highcharts or ECharts also draws in RichView, Highcharts and ECharts on the same data, title and size, so a review agent can judge each type against INT-12's bar: charts withstand side-by-side comparison with Highcharts and ECharts. The references are benchmarks, never code or styling to copy.

The lab runs in no gate tier (ARC-64). Its verdicts, written by the review agent following [REVIEW-PROMPT.md](REVIEW-PROMPT.md), are FEA-CH-75's library-comparison evidence ("a local comparison against the libraries that define the type ... on the same data").

## Rules

- Highcharts is commercially licensed and runs only as a local reference (INT-53). It and ECharts are installed only under the evidence folder, never in a `package.json` or lockfile of this repository, and their code and pictures are never vendored or committed.
- Outputs go under `$RICHVIEW_EVIDENCE/arc64/<commit>/`, never into the repository.
- Each library is configured as its documentation's idiomatic chart for the type, with its default colours, fonts and axes. Highcharts credits are off (attribution, not craft) and animation is off in both; the stock charts drop the range selector, navigator and scrollbar, which a static 10-bar picture does not use. Where an encoding cannot be mapped faithfully, `comparison.json` records it instead of approximating.

## Install

```sh
export RICHVIEW_EVIDENCE=<an evidence folder outside the repository>
npm install --prefix "$RICHVIEW_EVIDENCE/reference" --save-exact --ignore-scripts highcharts@13.1.1 echarts@6.1.0 yaml@2.9.1
```

`yaml` only parses the report's chart blocks. Playwright 1.63 comes from the repository's `node_modules` (`npm ci --ignore-scripts`) with its Chromium installed. A kept RichView build provides the renderer: `$RICHVIEW_BIN` is a folder holding `richview` and `dist/web`, named by the commit it was built from (never build here; use the release agent's kept build).

## Run

```sh
node benchmarks/compare/run.ts --richview "$RICHVIEW_BIN"          # every type
node benchmarks/compare/run.ts --richview "$RICHVIEW_BIN" --types bar,line --styles sleek,air
node --test benchmarks/compare/test/*.test.ts                       # adapter unit tests
```

Options: `--evidence` (default `$RICHVIEW_EVIDENCE`), `--out` (default `<evidence>/arc64/<name of the build folder>`), `--types` (ids or types), `--styles` (default `sleek,air`: RichView is captured once per style; the first keeps the image key `richview`, the others are `richview-<style>`; the style is passed as `richview generate --style <name>`; `--style` takes one), `--keep` (keep the work folder with the generated reports and pages).

## What it does

1. `corpus.ts` reads the chart blocks and their CSV or inline rows; a repeated type gets `-2`.
2. `types.json` maps each RichView type to the Highcharts series type and module and the ECharts series type, with the documentation URL, or `null` and the reason.
3. `adapter.ts` turns block and rows into Highcharts options and an ECharts option.
4. `run.ts` renders a one-chart report per type and width with `richview generate --style <style>` once per style (Sleek is dark, Air is light, as are the references), draws the two libraries from static pages loading the local install, and captures all three in headless Chromium with reduced motion once settled at 720 x 440 and 390 x 440 (1x). RichView's figure gets `wide: true` and a `style.ratio` calibrated so the whole figure is as tall as the reference charts; its viewport is the width plus the page margins so the figure is exactly 720 or 390 px wide. The measured figure size is in the manifest.
5. `sheet.ts` composes one sheet per type (RichView Sleek, RichView Air, Highcharts, ECharts; two widths) and the contact sheet.

Types neither library defines are listed with the reason and not rendered.

## Output

```
$RICHVIEW_EVIDENCE/arc64/<commit>/
  index.html           contact sheet
  comparison.json      per type: libraries, series and modules, unmapped encodings, notes, errors, image paths; versions; counts
  sheets/<id>.png      one sheet per type
  captures/<id>/       the 1x captures per type (two widths of each column)
  REVIEW-PROMPT.md, verdicts.schema.json   copies for the review agent, which writes verdicts/<id>.json
```
