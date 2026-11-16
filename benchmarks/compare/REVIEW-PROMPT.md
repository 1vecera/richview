# Review prompt: RichView against Highcharts and ECharts

You are the review agent of FEA-CH-75's library-comparison evidence (ARC-64). Judge every chart type in `comparison.json` against Daniel's INT-12 bar: RichView's charts withstand side-by-side comparison with Highcharts and ECharts. The references are benchmarks for quality, never code or styling to copy; do not recommend copying a reference's look.

## Inputs

All paths are relative to the run folder (`$RICHVIEW_EVIDENCE/arc64/<commit>/`).

- `comparison.json`: per type, the libraries that draw it, the encodings that could not be mapped, notes on how a reference was configured, the capture paths and the versions.
- `sheets/<id>.png`: one sheet per type; columns RichView Sleek (dark), RichView Air (light), Highcharts (light), ECharts (light); row one is 720 x 440, row two 390 x 440. `comparison.json` lists the RichView styles under `richview.styles` and each type's `richviewStyles` (image keys `richview` for the first style, `richview-<style>` for the others).
- `captures/<id>/<column>-<width>.png` (`richview`, `richview-air`, `highcharts`, `echarts`): the same pictures at 1x, for detail. Open them when a label or mark is too small on the sheet.
- `index.html`: the contact sheet, for orientation only.

## What to judge

For each type with at least one library, look at both widths of all columns, for every RichView style the type was captured in. Read the type's `unmapped` and `notes` first: a missing mapping or a documented idiom (for example a reference drawing a different encoding, ECharts drawing a waterfall from stacked bars) limits what the comparison proves, so say so under `notComparable` and do not count it against RichView.

Judge these six criteria, each against each reference that draws the type, from the pictures alone:

- **craft**: overall finish. Geometry, spacing, alignment, stroke weights, corner and edge handling, hierarchy between title, subtitle, plot and chrome, restraint (the right number of lines, marks and colours).
- **legibility**: text size and contrast, axis and tick readability, whether the claim and the data can be read at a glance, no clipped or overlapping text or marks.
- **labels**: which values and names are shown, how they are placed and thinned, direct labels against legends, units and axis titles, leader lines, no collisions.
- **colour**: palette quality and distinction between series, encoding sense (sequential, diverging, categorical), contrast on the page background, colour-blind safety. The references use their default light pages. Judge each RichView style on its own and against a reference of the same lightness: Air (light) against Highcharts and ECharts for colour, contrast and the palette's distinction on white; Sleek (dark) against the same references for its colour decisions only, since a dark page cannot be compared with a light one on lightness. Never mark a colour or contrast down against a reference only because one is dark and the other light.
- **density**: use of the space, data-ink balance, plot area against chrome, marks per area, whether dense data stays readable.
- **phone**: the 390 px capture alone: text size, label thinning, tap-sized marks, whether anything is clipped, wasted or cramped.

Ignore RichView's toolbar (Table, Download, CSV, Expand), which the references lack, except where it crowds the chart. Ignore tooltips and motion; the pictures are settled and static. Interactivity and motion are judged elsewhere (INT-59, INT-61, FEA-CH-39).

## Verdict scale

- `withstands`: placed next to the reference, a critical reader would not prefer the reference on this criterion.
- `close`: a reader could prefer the reference in a detail, but nothing a reader would call a flaw.
- `falls-short`: a reader would prefer the reference, or RichView shows a flaw (clipping, collision, unreadable text, misleading encoding, empty or broken drawing).

Judge each RichView style separately (Sleek and Air). Give a verdict per criterion, with `against` naming the verdict for each reference when they differ. Where the styles differ on a criterion, the criterion `verdict` is the lower one and `byStyle` gives each (`{ "sleek": ..., "air": ... }`); name the style in every reason that is specific to one. The type's `byStyle` gives each style's overall verdict. The type's overall `verdict` is the lowest criterion verdict of any style, unless you state in `summary` why a higher one is fair. When RichView is the only column that drew correctly (a reference failed to render), say so and judge against the intent of the type.

Be skeptical. A pleasant picture is not enough: check every label for clipping, every legend entry against the marks, and the phone capture for text that is too small. Do not give `withstands` for a criterion you could not judge from the pictures; say what is missing in `notComparable`.

## Output

Write one file per type, `verdicts/<id>.json`, that validates against `verdicts.schema.json` (a copy sits beside `comparison.json`). Every reason names the width, the RichView style when it matters, and the reference it compares with, for example "720: Highcharts thins the x labels to five, RichView shows all twelve at a larger size". `fixes` lists concrete changes (name the style when a fix applies to one) to RichView's output, never a request to imitate a reference.

For types where `libraries` is empty, write no verdict; `comparison.json` already records that neither reference defines the type.

When all types are done, write `verdicts/summary.json`: `{ "counts": { "withstands": n, "close": n, "falls-short": n }, "fallsShort": [ids], "close": [ids] }`. Do not edit `comparison.json` or the sheets.
