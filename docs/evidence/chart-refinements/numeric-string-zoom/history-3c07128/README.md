This archived evidence belongs to the reviewed source commit `699d048` and evidence commit `5951fcc`, based on `3c07128`. Current-main results are linked from the [current evidence](../README.md).

# Numeric strings survive numeric zoom

The source X values `[0, "4.000", "6.000", 10]` draw four observations. On main `3c07128`, pressing `+` selects `[2.5, 7.5]` but removes both interior strings and reports “No observations.” The change retains those two observations in scatter, line and area views, using the renderer’s existing finite-number parser. All-string numeric sources gain the same zoom behavior.

| Scientific, 390 px viewport | Sleek, 320 px viewport |
| --- | --- |
| Before: ![Scientific zoom loses both observations](before-scientific-390.png) | Before: ![Sleek zoom loses both observations](before-sleek-320.png) |
| After: ![Scientific zoom retains both observations](after-scientific-390.png) | After: ![Sleek zoom retains both observations](after-sleek-320.png) |

These are actual settled Chromium figure captures at device scale 2, with unchanged report styling and source. [validation.json](validation.json) records the build, source and image identities, checks and coverage.

After the repository build, copy the [source fixture](source.rmd) into a dedicated report folder and serve it locally:

```sh
mkdir -p tmp/numeric-string-repro
cp docs/evidence/chart-refinements/numeric-string-zoom/source.rmd tmp/numeric-string-repro/report.rmd
target/release/richview serve tmp/numeric-string-repro/report.rmd --style scientific --port 0
```

Use a 390 px viewport, focus the chart stage and press `+`. The selected observations have X values `4.000` and `6.000`; Home and Right inspect those original rows. The downloaded SVG contains the selected marks. Table and CSV retain all four original rows, including the string literals. Escape restores the original chart. Use `--style sleek` and a 320 px viewport for the dark pair.

Categorical line/area sources cannot enter numeric zoom. Hiding their nominal series may make the visible source numeric; restoring it resets numeric selection and reveals all categories. Calendar strings keep calendar behavior, while a numeric selection containing only year strings keeps its established numeric scale. Updating a chart cancels an obsolete numeric gesture. Readouts use the original source rows with their existing formatting.

Validation includes exact native SVG parity, matched live views across five themes and three widths, normal and reduced motion, source Table/CSV and SVG exports, inclusive boundaries, missing Y, calendar and category controls, facets, visibility changes, and real pointer drag/update transitions. The required local Rust and host checks and independent adversarial review passed. Crossing-only line/area windows and extreme-coordinate overflow remain separate existing limitations; no interpolation, clipping-neighbor policy or date coercion is added.
