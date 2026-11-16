# Current chart comparison

The [fixed review guide](../../CHART-REVIEW.md) defines the unchanged data, comments, four viewports and real tooltip states. Before and after cover 60 pages and 540 figures each in five styles. Actual glyph overlap/clipping findings fell from 115 to 0; browser errors are zero. Full captures stay in ignored tmp/chart-before/ and tmp/chart-after/; Hosted CI is manual-only and supplied none of this evidence. [Build hashes and checks](verification.json) identify this release.

These phone views use 390 px throughout. Each scatter pair includes a real keyboard tooltip.

| Style | Line before | Line after | Scatter tooltip before | Scatter tooltip after |
| --- | --- | --- | --- | --- |
| sleek | [Before](sleek-line-before.png) | [After](sleek-line-after.png) | [Before](sleek-scatter-before.png) | [After](sleek-scatter-after.png) |
| sleek-light | [Before](sleek-light-line-before.png) | [After](sleek-light-line-after.png) | [Before](sleek-light-scatter-before.png) | [After](sleek-light-scatter-after.png) |
| scientific | [Before](scientific-line-before.png) | [After](scientific-line-after.png) | [Before](scientific-scatter-before.png) | [After](scientific-scatter-after.png) |
| brutalist | [Before](brutalist-line-before.png) | [After](brutalist-line-after.png) | [Before](brutalist-scatter-before.png) | [After](brutalist-scatter-after.png) |
| hand-drawn | [Before](hand-drawn-line-before.png) | [After](hand-drawn-line-after.png) | [Before](hand-drawn-scatter-before.png) | [After](hand-drawn-scatter-after.png) |

Bar study 01: [before](hand-drawn-bars-before.png), [after](hand-drawn-bars-after.png). Brutalist heading gap and fixed prose: [before](brutalist-header-before.png), [after](brutalist-header-after.png).

[Editable before/after Figma plates](https://www.figma.com/design/NlR94RSo5Pw6aszsoXk565?node-id=9-24848) contain 40 matched plates per page with the same twelve chart cases, widths and interaction states. Twenty heatmap tooltip figures were refreshed from the final authored-unit capture; the other final chart graphics reproduce the imported cases byte-for-byte; the neutral plate caption wraps on phones. [Hand-drawn case](figma-hand-drawn-case.png) and [whole plate](figma-hand-drawn-plate.png) show the imported result. The [updated heatmap tooltip](figma-heatmap-tooltip.png) retains the authored measure label. Browser PNGs remain the pixel evidence because Figma can render fonts differently.

The hand-drawn flagship PDF smoke check retains four known bottom-whitespace warnings (pages 11, 16, 17 and 20); the same pages were flagged in the merged baseline. Other smoke checks passed. Arbitrary custom fonts, long names and other datasets need their own checks. Historical generated archives remain available at [the merged baseline](https://github.com/richview-universe/richview-v3/tree/3c07128b7139687b754f6e1b5848c2a46c31cfe5/docs/evidence).

The additional pie note check uses the same source at 320 and 390 px: [before](pie-note-before.png), [after](pie-note-after.png). Notes now avoid the complete disk, key swatches and all category labels; crowded notes are explicitly omitted rather than overlapped. The final composition also passes 585 extra chart cases with real keyboard tooltips.

The earlier `9b300f0` shared-host performance capture passes 19 of 25 fixed timing/size/memory limits and 6 of 7 integrity checks. Over-budget endpoints: native.dense, cli.dense, browser.theme.first-all-charts, browser.theme.latest-all-charts, browser.small-edit, browser.dense-edit. The browser integrity check also fails because its harness asserts these timing limits after completing its behavioral assertions without browser errors. Exact measurements and matching runtime/input signatures remain in [the verification record](verification.json); limits were not widened. This machine was shared, with four performance cores selected for the timing commands, so the capture is not a quiet-machine acceptance pass. The final composition has fresh correctness and screenshot checks; this earlier timing capture does not establish the final Wasm performance.

Baseline comparison uses the unchanged dense source and exact baseline/current CLI binaries in 50 alternating runs each: median 540.5 → 541.5 ms. The baseline also exceeded the CLI, theme-switch, small-edit and dense-edit limits. The observed dense worker source edit was 1,148 → 2,011.5 ms (one event per build); sequential shared-host browser captures cannot establish the cause. Browser latency remains a limitation, not a claimed improvement.
