# Fixed chart review

Use the same observations, comments and interactions for every chart update. The [standard report](../benchmarks/chart-regression/report.rmd) covers temporal and categorical lines, close endings, missing data, small/zero bars, signed grouped bars, dense labeled scatter/bubbles, signed heatmaps, pie and area. The unchanged sourced flagship adds eighteen real charts; [Bar study 01](../benchmarks/chart-craft/reports/bar-01/report.rmd) exercises long names and signed horizontal bars.

Capture all five styles (`sleek`, `sleek-light`, `scientific`, `brutalist`, `hand-drawn`) at 1440, 768, 390 and 320 px. Every figure has a resting screenshot and a real keyboard-focused tooltip screenshot; full-page captures retain the same prose, review callout, source comments and table. The default matrix is 60 pages, 540 figures and 1,080 figure screenshots. Source, runtime and native binary hashes identify each capture. Before/after fixture hashes must match.

```sh
cargo run --release --locked -p richview-build
node scripts/verify-chart-review.mjs --out tmp/chart-before --phase before
# Apply the chart change, rebuild, then compare the unchanged cases.
node scripts/verify-chart-review.mjs --out tmp/chart-after --phase after --baseline tmp/chart-before
node scripts/design-review/figma-plates.ts tmp/chart-after
```

The after check rejects actual SVG glyph overlap/clipping, page overflow, browser errors, extra calendar rows, color bars, misaligned captions/axis titles/legends and overflowing tooltips. A phone keyboard cycle hides both categorical series and restores them, including from the all-hidden state. It runs after font loading and the production `settled()` barrier. Native glyph estimates and browser glyph rectangles remain separate evidence. A fixed matrix establishes those cases; arbitrary custom fonts, arbitrarily long source text and other datasets still need their own checks.

Hosted CI is advisory and manual-only; it supplies no per-PR chart-review evidence. The release gate records the required captures and checks in its receipt (DEL-21, ARC-50). Full generated reports and large capture archives stay in ignored `tmp/`; the [small current gallery](evidence/chart-review/README.md) is the review entry point in Git. Historical generated archives remain available at [the merged baseline](https://github.com/richview-universe/richview-v3/tree/3c07128b7139687b754f6e1b5848c2a46c31cfe5/docs/evidence).

## Editable sketches

[RichView chart review — fixed cases](https://www.figma.com/design/NlR94RSo5Pw6aszsoXk565) contains separate before/after pages in Daniel's team drafts. Each page has 40 matching plates: five styles, four viewports and two interaction states. Each plate contains the standard eight charts, flagship charts 3/13/17 and Bar study 01. Upload the generated SVG plates to the corresponding page with Figma MCP, then arrange one row per style. Fonts and vectors remain editable; no screenshot tracing or redrawn data is involved. Browser PNGs are the pixel evidence because Figma's text engine can differ from the browser.

## Production review reports

The builder stages the fixed standard and bar reports in all five styles using the normal report renderer and controls. The source remains synthetic and unchanged.

| Style | Standard report | Bar study 01 |
| --- | --- | --- |
| Sleek | [Open](https://richview.ai/showcase/chart-review/standard/Rendered/sleek.html) | [Open](https://richview.ai/showcase/chart-review/bar-study-01/Rendered/sleek.html) |
| Sleek light | [Open](https://richview.ai/showcase/chart-review/standard/Rendered/sleek-light.html) | [Open](https://richview.ai/showcase/chart-review/bar-study-01/Rendered/sleek-light.html) |
| Scientific | [Open](https://richview.ai/showcase/chart-review/standard/Rendered/scientific.html) | [Open](https://richview.ai/showcase/chart-review/bar-study-01/Rendered/scientific.html) |
| Brutalist | [Open](https://richview.ai/showcase/chart-review/standard/Rendered/brutalist.html) | [Open](https://richview.ai/showcase/chart-review/bar-study-01/Rendered/brutalist.html) |
| Hand-drawn | [Open](https://richview.ai/showcase/chart-review/standard/Rendered/hand-drawn.html) | [Open](https://richview.ai/showcase/chart-review/bar-study-01/Rendered/hand-drawn.html) |

The hand-drawn refinement follows the handwritten lettering and gray ink of the [No Mercy No Malice chart in “Hunger”](https://www.profgalloway.com/hunger-2/), while preserving exact endpoints and scale positions. Scott Galloway credits Katherine Dillon's earlier hand-drawn charts in [the tenth-anniversary post](https://www.profgmedia.com/p/no-mercy-no-malice-turns-10). Lettering, hatches and marker identities carry that treatment; curve geometry stays bounded by observations rather than being distorted for appearance.
