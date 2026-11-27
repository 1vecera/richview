# Complete pie labels with a useful disk

Narrow pies reserved a fixed two-column key inside the preferred ratio, leaving a tiny or negative disk radius as categories increased. Dense desktop labels overlapped and extended beyond the SVG. The renderer now retains radial labels when every complete label fits; otherwise, it wraps a category key and grows the chart to fit that key below a useful disk. Existing category order, values, palette and source readouts are preserved.

| Case | Exact source | Before | After |
| --- | --- | --- | --- |
| Seven categories, Scientific; 390 px viewport / 358 px SVG | [pie-05](../../../../benchmarks/chart-craft/reports/pie-05/report.rmd) | [Phone before](phone-before.png) | [Phone after](phone-after.png) |
| Eighteen long names, Sleek; 720 px viewport / 672 px SVG | [Dense report](dense-report.rmd) | [Dense before](dense-before.png) | [Dense after](dense-after.png) |

The phone disk grows from a 24.975 px radius to 100.24 px. The dense desktop example keeps a 120 px radius and replaces clipped radial labels with a two-column key containing every name and percentage. The chart is deliberately taller; categories are never combined or abbreviated.

One positive share accompanied by zero-valued categories must draw a complete disk. Zero shares keep their labels, source rows and keyboard slots while painting no wedge. The shared `pie_slice_path` helper and every motion attribute are preserved; its zero/full-sweep handling now keeps this geometry intact when entrance motion reconstructs the paths.

## Capture and verification

The paired captures compare main `3c07128b7139687b754f6e1b5848c2a46c31cfe5` with implementation `8c0419234d463cb841fa99e0c1440525cb725581`, after the sibling chart styles merged. They were refreshed on 11 October 2026 using fresh release native/Wasm builds, actual report HTML and the live browser runtime. Chromium `151.0.7922.173` ran headless with isolated contexts, packaged fonts, device scale factor 2 and reduced motion. Source, theme and viewport match within each pair. [Verification metadata](verification.json) records source, build and image hashes, dimensions and diagnostics.

Each before/after matrix has 195 captures: all five catalog pies, eighteen categories, long and unbroken names, one slice, unequal shares, one positive with zero shares, a zero total, and dense facets; Sleek, Scientific, Brutalist, Sleek Light and Hand-drawn at 320/390/720 px. All 225 after chart instances, including facet children, have clean live glyph diagnostics, no horizontal overflow, no browser errors and no active animations. The zero-total case retains its existing native `empty-region` warning.

Focused Rust tests cover actual chart widths 288/320/358/479/480/720/936 px, complete names and percentages, positive radii, hit sectors, source precision, radial preservation, zero shares, motion reconstruction and an undersized explicit height. Thirty-two static/shared-renderer/live/downloaded-SVG checks pass in Scientific and Hand-drawn, at 320/720 px, with both reduced and ordinary motion. Eighteen pointer/touch cases verify 96 positive sectors and all 102 keyboard rows. Twelve offline checks separately use actual native CLI reports with JavaScript disabled and enabled. Independent review accepts the current-main source and twelve fresh normal-motion cases, including long Unicode names and full/zero paths.

All required local checks pass: format, workspace tests, native and Wasm Clippy, fresh release build, generated fixtures, strict TypeScript declarations, native/editor/compatibility/browser/landing/local-Cloudflare/showcase/flagship/reel/browser-API/motion-runtime checks, and documentation verification. Local Node is `26.8.1` rather than CI's Node 22; Rust is `1.99.0`. Dependencies came from the earlier locked installation, with unchanged package manifests. No hosted CI, deployment or publication was performed for these captures.

To inspect the seven-category example with the repository's capture workflow after building:

```sh
node scripts/design-review/capture.ts --out tmp/pie-review --cases craft-pie-05 --themes scientific,hand-drawn --widths 720,390,320
```

For the dense example, copy `dense-report.rmd` into a scratch report folder and run `target/release/richview generate <folder>/report.rmd --style sleek`; the generated report is self-contained. The complete local matrix and interface checks are recorded under `tmp/pie-room/current-main/` in the implementation worktree.

## Bounds of the change

The ratio remains a preferred shape: complete labels can make the chart taller. An undersized internal `_renderHeight` emits `fixed-height` and grows rather than producing invalid geometry. Many categories still require a long key, and existing palette cycling remains visible. Zero-total empty-state behavior and annotation placement are unchanged. Static reports retain the existing responsive fixed-frame contract; live charts render at actual column width, and downloaded SVGs compose the figure heading above the plot. These checks compare the corresponding plot geometry and do not claim identical global dimensions. PDF export is outside this verification.
