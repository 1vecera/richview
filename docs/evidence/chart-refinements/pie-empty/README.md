# Explain a pie with no positive values

A pie whose eligible total is zero previously left a blank plot. It now shows `No positive values to show` in the existing chart text style. The phrase also describes missing, invalid and rejected negative observations without treating them as measured zeros. The product change is one label call in the existing return branch.

| Case | Before | After |
| --- | --- | --- |
| No title, two zero values; Sleek, 320 px viewport | [Blank phone plot](phone-before.png) | [Visible phone state](phone-after.png) |
| Two zero values, internal 60 px plot height; Brutalist, 272 px viewport / 240 px SVG | [Blank short plot](short-before.png) | [Visible short state](short-after.png) |

[Primary fixtures](fixtures.json) include zero values, missing values, negatives, invalid strings, mixed unavailable values, omitted and long titles, a populated control, a single positive slice and mixed empty/populated facets. [Supplementary fixtures](supplement-fixtures.json) cover one positive category with two zero shares, a 60 px internal height and a 4:1 preferred ratio.

The matched captures compare main `bb8b112c121eae1ec70d810df1b5e6a2070c9c4e` with product commit `a625297a29582b842efae1ae50ede13de29bdbde`. Both used fresh native/Wasm release builds. Headless Chromium `151.0.7922.173` used packaged fonts, device scale factor 2 and settled reduced motion. The [verification record](verification.json) contains exact source, build and image hashes.

The two matrices contain 195 matched reports across Sleek, Scientific, Brutalist, Sleek Light and Hand-drawn. Primary widths are 320/390/720 px; supplements add 272 px, which gives the renderer's 240 px minimum. Native diagnostics, figure sizes and sector geometry are identical in every pair. All 130 nonfaceted empty SVGs differ only by the new text node; all 50 positive-control SVGs and PNGs are byte-identical. Fifteen faceted reports preserve the populated panel. Actual glyph measurements cover 210 child or ordinary charts and introduce no new finding.

Seventy-two matched static/shared-renderer/live/downloaded-SVG panel checks pass in Scientific and Hand-drawn at 320/720 px with reduced and ordinary motion. They preserve 168 exact keyboard observations and 72 original Table/CSV exports. Empty plots have no sectors, marks or pointer/touch data hits. Native and live accessibility snapshots expose the message. Forty-eight separate offline native CLI views, with JavaScript disabled and enabled, verify 56 panels without external requests or browser errors. The global empty-data `No observations` output is byte-identical to main.

All required local checks pass: format, workspace tests, native and Wasm Clippy, release build, generated fixtures, editor/native/compatibility APIs, strict TypeScript declarations, browser/landing/local-Cloudflare/showcase/flagship/reel/browser-API/motion-runtime checks and documentation verification. Local Node is `26.8.1`, rather than CI's Node 22; Rust is the pinned `1.99.0`. The evidence uses local checks and does not claim a hosted CI result.

Fresh independent review accepts the source and actual result. Its separate 24 matched reports cover 28 actual chart SVGs: 16 gain the empty note and all 12 populated panels remain identical. Source diagnostics, dimensions and Home/End readouts are preserved. Eight views retain the expected negative-value error; four further minimum-width views confirm contained text, including a 60 px plot. The earlier review remains historical.

## Reproduce

Build both isolated checkouts using the pinned toolchain and locked dependencies. Set `pie_base` to the checkout of the baseline commit; run the commands below from the implementation checkout. These helpers start only local servers and headless browsers.

```sh
CARGO_BUILD_JOBS=3 cargo run --release --locked -p richview-build
mkdir -p tmp/pie-empty
cp docs/evidence/chart-refinements/pie-empty/fixtures.json docs/evidence/chart-refinements/pie-empty/supplement-fixtures.json docs/evidence/chart-refinements/pie-empty/verify-paths.ts docs/evidence/chart-refinements/pie-empty/verify-native-cli.ts tmp/pie-empty/
node docs/evidence/chart-refinements/pie-empty/capture.ts --worktree "$pie_base" --out tmp/pie-empty/before --sources tmp/pie-empty/fixtures.json --themes sleek,scientific,brutalist,sleek-light,hand-drawn
node docs/evidence/chart-refinements/pie-empty/capture.ts --worktree . --out tmp/pie-empty/after --sources tmp/pie-empty/fixtures.json --themes sleek,scientific,brutalist,sleek-light,hand-drawn
node docs/evidence/chart-refinements/pie-empty/capture.ts --worktree "$pie_base" --out tmp/pie-empty/supplement-before --sources tmp/pie-empty/supplement-fixtures.json --themes sleek,scientific,brutalist,sleek-light,hand-drawn --widths 272,320,390,720
node docs/evidence/chart-refinements/pie-empty/capture.ts --worktree . --out tmp/pie-empty/supplement-after --sources tmp/pie-empty/supplement-fixtures.json --themes sleek,scientific,brutalist,sleek-light,hand-drawn --widths 272,320,390,720
node tmp/pie-empty/verify-paths.ts --worktree "$pie_base" --out tmp/pie-empty/paths-before --phase before
node tmp/pie-empty/verify-paths.ts --worktree . --out tmp/pie-empty/paths-after --phase after
node tmp/pie-empty/verify-native-cli.ts
```

`capture.ts` writes each actual PNG and a source/build/geometry manifest. The two focused verifiers assert visible and exported text, preserved source controls, static/live accessibility and absence of invented marks. The native CLI intentionally returns an error for negative source values while still producing its inspectable report; that behavior is retained and checked.

## Bounds

Existing zero-total warnings and negative-value errors remain unchanged. This label does not expand validation, distinguish the cause of unavailable values, change dimensions, reorder categories or change a positive pie. Current main already includes the pie-room refinement: the one-positive/two-zero control preserves its complete disk and readable key. Negative-value source errors remain visible in the live diagnostics of both empty and populated rejected-value fixtures. Browser glyph diagnostics remain distinct from native source diagnostics. PDF export is outside this verification.
