# S27-582 visual and interaction review

**Prepared for review.** The real Rust chart renderer and browser runtime are improved, with a runnable local reference benchmark and retained before/after builds. This does not establish complete Highcharts visual parity. No merge, release, deployment or license purchase is included.

Evidence on Lenovo: `/home/vecera/tmp/s27-582-evidence/`. Run the server described in [README.md](README.md), then browse `/evidence/index.html`, `/reference/index.html` and `/bench/index.html`. Each archived report is runnable through that loopback server. Original source, native outputs, patches, browser diagnostics, screenshots, videos and installation/provenance records remain inspectable locally.

## References actually reviewed

The corpus contains **125 distinct official gallery pictures**, verified by decoded pixels and byte hashes. The index records the official demo and downloaded image URLs, local paths, chart/state, dimensions and provenance. Ten additional captures from five live official examples supplement the corpus; they do not inflate its count. Highcharts **13.1.1**, its lockfile and terms are retained locally, outside production source.

The actual [time-series sample](https://www.highcharts.com/demo/highcharts/line-time-series) informed temporal spacing and readable observation readouts; [basic columns](https://www.highcharts.com/demo/highcharts/column-basic) informed category/axis hierarchy; [missing-data area](https://www.highcharts.com/demo/highcharts/area-missing) reinforced leaving absent observations as gaps; [heatmap](https://www.highcharts.com/demo/highcharts/heatmap) informed row alignment and restrained cell borders; [irregular time](https://www.highcharts.com/demo/highcharts/spline-irregular-time) informed calendar tick density. These are observations from rendered examples, not copied implementation. The [zoom](https://www.highcharts.com/docs/chart-concepts/zooming) and [tooltip](https://www.highcharts.com/docs/chart-concepts/tooltip) guidance informed the live-state review. The pinned local package executes only in the evaluation harness.

## Baseline and substantive rounds

Each dated directory contains 48 cases: eight reports × scientific/sleek/brutalist × widths 720/320. Manifests retain UTC capture times, source/build hashes, native estimates and settled browser glyph bounds separately. Every report uses its own folder and chart-local YAML data/loading definition; dense observations come from that report's `power.csv`.

| Artifact identifier | Observed gaps and implemented response | Actual browser glyph issues | Pointer readout present / contained |
| --- | --- | ---: | ---: |
| `2026-10-04-00-baseline` | Unmodified fetched default (`2cd1ee2`): clipped calendar context, crowded/full-width numeric ticks, disappearing long categories, mobile pie labels outside the SVG, and no pie pointer readout. | 110 | 42/48 · 42/48 |
| `2026-10-04-01-layout` | Adaptive plot margins and tick density; step-aware compact numbers; bounded category wrapping and reserved footer height. Re-capture exposed remaining calendar/pie problems rather than hiding them. | 45 | 42/48 · 42/48 |
| `2026-10-04-02-tooltips` | Renderer-owned bar/cell/circle/sector hit regions; empty-space rejection; pie category readout; exact source text and units; measured tooltip flipping/containment; mobile pie legend; shared static/browser calendar spacing. Three dense mobile axis collisions remained. | 3 | 48/48 · 48/48 |
| `2026-10-04-03-navigation` | Existing Rust overview enabled for report time series with ≥8 distinct timestamps; visible zoom buttons/reset state/help; overview keyboard access; touch pan/pinch; cancellation preservation; inspection bounded to visible observations; first calendar label moved clear of the y-axis. Final offline mobile review also exposed an intermediate-width scatter title collision; a dedicated axis-title row fixes it. | 0 | 48/48 · 48/48 |

The retained runtime archives and cumulative `product.patch` files identify the renderer used by each round. Archived builds were re-captured with one corrected harness protocol: bubble-size values are included and total widget dimensions are matched, including headings/controls/overview. RichView and Highcharts plot rectangles and SVG sizes are recorded separately because their chrome composition differs. The Highcharts reference keeps its own defaults and identity.

Resting/hover pairs provide the consistent before/after comparison. Earlier runtimes do not expose exact time-window telemetry, so their brush/zoom/pan recordings show RichView gestures against the full-range reference and are marked `interaction_window_matched: false`. Final-round zoom/pan/overview pairs use the actual Rust scene window. Brush screenshots capture an in-progress RichView gesture; they are not a claim that both libraries have identical transient gesture rendering.

## Live verification and export evidence

`verification/checks.json` and the desktop/touch recordings cover signed bar scales, grouped-bar row identity, source values/units, empty-space rejection, retained series colors after hiding, bubble-size mapping, visible-window keyboard inspection, zoom/reset buttons, cancelled brushing, ordinary wheel scrolling, modifier zoom, horizontal pan, keyboard overview, 720→320 resizing, contained tooltips and standalone SVG output. Intermediate scatter widths 354/390/520 are checked in all three canonical fonts. A real 390px report receives Chromium touch events for two-pointer pinch, one-finger pan and touch reset, under reduced motion. No visible desktop input is used.

`verification/native/checks.json` contains 48 native CLI checks. Native generation writes 24 complete offline reports, one per report/theme. There are 36 clean native cases and 12 cases with intentional missing-observation warnings; no native geometry errors. Browser glyph checks inspect settled canonical fonts rather than treating native estimates as actual glyph measurements. Missing values, signed scales and the 12,000 dense source observations retain their analytical meaning. The existing integration checks also verify fully offline HTML with network blocked, canonical themes, dense worker behavior and exported SVG markup.

Required local handoff checks: Rust formatting; workspace and Wasm Clippy with warnings denied; locked workspace tests; full release native/Wasm/package build; benchmark fixtures; editor, native, compatibility, browser, landing, showcase, reel and browser API verifiers; strict TypeScript declaration check. Logs are retained at the evidence root. Hosted CI state is recorded in the PR/handoff separately; no spending to unblock it is authorized.

## Candid remaining gaps

- Narrow categories abbreviate at three lines; the tooltip and table retain full names. Highcharts can expose more text through rotation. The current generic chart header also duplicates static legends and toggle controls, consuming space.
- Zoom/pan/overview improvements apply to the existing temporal engine. Generic numeric line/area, bar, scatter, pie and heatmap retain hover, keyboard inspection, hiding, table and export behavior; this change does not add generic continuous-domain navigation.
- Highcharts Stock grouping and RichView's per-pixel mean/range treatment differ. Dense paired readouts are useful comparative evidence, not proof that their aggregation policies are interchangeable. RichView's raw readout/table remains authoritative.
- Standalone SVG retains geometry and theme tokens, but relies on available fonts instead of embedding the full report's licensed font payload. Offline report HTML embeds its fonts/runtime; standalone SVG font fallback can alter appearance on another machine.
- The 48-case benchmark and focused gesture checks do not cover every possible annotation, data distribution, assistive technology or gallery type. They establish focused improvements in RichView's supported types, not all-product parity or a formal accessibility certification.
- “Escapism” and “Where to Render” remain unclarified wording from the coordinator conversation. No named feature requirements are inferred from them.

## Boundary and publication review

Changes stay in `richview-chart`, existing Rust browser bindings and this local benchmark. They add no report/editor/data schema and do not touch the S27-578 authoring worktree or S27-577 teaser session. The fresh task worktree starts from fetched remote default. Product behavior remains Rust; JavaScript is limited to the reference harness and verification tooling. Repository push/PR automation was inspected: task pushes do not deploy, the PR workflow runs checks only, and repository webhooks were empty. The repository is private; task-branch visibility and ordinary draft-PR notifications are the authorized preparation exception. No reviewers are requested and no merge is performed.
