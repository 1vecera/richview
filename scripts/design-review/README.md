# Design review capture

`capture.ts` records how reports look and move in the build of the worktree it runs in: every case in every theme the build exposes, at desktop and phone widths. It writes full-page screenshots, chart crops, motion filmstrips, frame pacing and layout diagnostics, plus a contact sheet and one composite image per case. With a baseline capture it puts the two builds side by side.

The chart-level parts (crops, glyph checks, filmstrips, reduced-motion and settled-state comparisons, mount continuity, taps, pacing) find charts through the markup of the removed SVG engine (`script[data-rvc-runtime]`, `.rvc`). Reports no longer carry it, so those parts find no charts until they are ported to the WebGL report runtime; the page-level captures and diagnostics still work.

## Requirements

The script compiles nothing. Build the worktree first; the capture reads `target/release/richview`, `dist/web` and stops with a message when one is missing.

```sh
npm ci --ignore-scripts
cargo run --release --locked -p richview-build
```

Chromium runs headless with a temporary profile. `/usr/bin/chromium` is used when present; set `RICHVIEW_CHROMIUM` to choose another executable, or run `npx playwright install chromium` for the bundled browser.

## Usage

```sh
node scripts/design-review/capture.ts --out tmp/review/current
node scripts/design-review/capture.ts --out tmp/review/quick --quick
node scripts/design-review/capture.ts --out tmp/review/after --baseline tmp/review/before --cases openai-vs-anthropic --widths 1440,390,320
```

| Option | Meaning |
| --- | --- |
| `--out DIR` | Output folder (required). Each run replaces the case folders, `reports/` and `sheets/` it writes. |
| `--cases a,b` | Case ids from `cases.json`. Default: all cases, or the `quick` list with `--quick`. |
| `--themes a,b` | Themes or aliases. Default: every theme the build exposes. |
| `--widths 1440,390` | Viewport widths. Widths below 600 px are 2x touch phones with the 390 × 844 aspect ratio; wider viewports are 1x desktops 900 px tall. |
| `--baseline DIR` | An earlier capture to compare with. Sheets, the contact sheet and pacing tables show baseline and current together. |
| `--port N` | Loopback port of the local file server. Default: a free port. |
| `--jobs N` | Theme and width captures run in parallel (default 3). Pacing always runs alone. |
| `--quick` | The `quick` cases, filmstrips and pacing for the first chart of each engine and type only, pacing at the first width only. |

On a 20-thread laptop a full run of 15 cases, four themes and two widths takes about 10 minutes and writes about 300 MB; `--quick` takes about 3 minutes. The exit code is 1 when any capture failed; failures are listed in the manifest and the contact sheet.

## Cases and outputs

`cases.json` lists the cases: the flagship report, the tutorial and sales examples, and chart-local studies from `benchmarks/chart-craft/reports` chosen for dense series, missing observations, signed bars, long labels, bubbles, small pie slices and dense heatmaps. Each case names an output kind:

- `report`: the folder of `source` is copied into `DIR/reports/<case>/` (without `research`, caches and earlier renders) and generated once per theme with `richview generate --style <theme>`, which writes only inside the copy's `Rendered/`. The release CLI produces the same offline report a reader receives.
- `page`: a page the build already serves from `dist/web`, such as `showcase/openai-vs-anthropic/index.html`; `{theme}` in its path is replaced by the theme.

A new output kind is a producer in `outputs.ts` that returns the URL to capture for a case and theme.

Themes come from the file names of `crates/richview-core/assets/*.rstyle.yaml`, one theme per file. A style file added there is captured without changes here.

## What is recorded

For each case, theme and width:

- **Settled page.** Real clock; the page is scrolled through once so content revealed on entering the viewport is shown, then the runtime's `settled()` (every chart drawn at rest, fonts loaded; its resolution time is recorded) and finite animations are awaited. `page.png` is the full page; `<chart>.png` crops each chart figure's box with a 12 px margin.
- **Chart glyphs.** Read from the geometry model (`window.__richview.charts()`): each text item's box is checked for leaving the chart and for covering another label, and the runtime's own clipping and collision checks are added. Labels the runtime dropped or thinned on purpose are counted per chart (`dropped_labels`), not reported.
- **Report text.** Text boxes of different blocks that overlap by more than a fifth of a line (lines of one paragraph may touch through tight leading), text cut off by a clipping container or the viewport (by more than a pixel sideways or a quarter line vertically, since a line box includes the font's full ascent and descent), and text out of view inside a scroll container (reported separately: a table that scrolls sideways on a phone). Visually hidden accessible text is ignored.
- **Overflow, shifts and errors.** Horizontal page overflow with the innermost elements that cause it, cumulative layout shift as the largest session window of unexpected shifts (the Web Vitals definition), console errors and uncaught exceptions, and the native diagnostics of generation.
- **Motion filmstrip.** Entrance frames at 0, 80, 160, 240, 360, 480, 640, 800, 1000 and 1400 ms, then a pointer sequence at 0, 50, 100, 150, 250 and 400 ms: a mouse hover on desktop widths, a tap on phone widths, on the largest slice or bar, the middle point or cell, or the observation of a line nearest to 62% of its length, taken from the model's observations. Frames are the chart canvas only; the readout text, which is DOM, is printed under each pointer frame. One PNG per chart (`<chart>-filmstrip.png`).
- **Reduced motion.** With `prefers-reduced-motion: reduce`, the page top and the first drawn canvas of each chart must equal its settled frame.
- **Settled state.** The canvas after the entrance has finished is compared with the reduced-motion settled frame, so an animation that ends somewhere other than the static presentation is found.
- **Pacing** (first theme, real clock). The chart is scrolled into view while the page loads; the entrance records whether the runtime still listed the chart as pending when it became visible and how long until it stopped; requestAnimationFrame intervals, long animation frames and dropped frames are recorded for 1400 ms from the chart's first visible frame (first screen: the frame its canvas first had a size) and for 500 ms after the pointer arrives, at 1x and 4x CPU throttling.

`index.html` is the contact sheet: a findings table, then per case a matrix with the page and each chart as rows and theme and width captures as columns, the listed findings and the pacing table. `sheets/<case>.png` shows every theme's full page side by side at each width, scaled to a quarter of its pixels and wrapped into strips up to 4000 px tall. `manifest.json` holds everything: build commit and binary hashes, device facts, themes, viewports, the timeline and, per case, each capture with image paths, diagnostics, motion results, timings and pacing. Paths are relative to the output folder.

## Method

**Frame capture.** The runtime emits no per-frame event, so filmstrips read the chart canvas in the page on every `requestAnimationFrame` and keep, for each time in the list, the first frame at or after it, with its actual time. The clock is real, so frames depend on machine speed; compare runs on one quiet machine. Time 0 is when the chart is scrolled to the middle of the viewport (a chart enters when half of it is visible, so this is within a frame of the entrance starting) or, for the pointer sequence, the first pointer input. A chart on the first screen has no entrance and its frames are identical. `settled()` finishes entrances at once, so only the settled page calls it; motion probes watch `pending()` instead. A frame can be one frame older than the input it follows. Each chart is filmed in a fresh page.

**Frames are canvas only.** Canvas pixels are read in the page, so a chart taller than the viewport still gets whole frames; it also gets a taller viewport of the same width so its pointer target is in view (`motion_viewport`).

**Pixel comparison.** Two frames are equal when no channel differs by more than two levels. An element that keeps a finished animation's fill stays on its own compositing layer and its edges rasterise slightly differently, so the settled-state check accepts up to 0.5% differing pixels and records the exact counts.

**Pacing.** Frame intervals are the differences between consecutive `requestAnimationFrame` timestamps. An interval spanning n refresh intervals counts as one delivered frame and n − 1 dropped frames; the dropped percentage is dropped / (delivered + dropped). The refresh interval is measured on an idle page. Long animation frames come from the `long-animation-frame` performance entries. CPU throttling uses the DevTools protocol (`Emulation.setCPUThrottlingRate`). The device facts record the CPU, Chromium version and the GPU feature status of headless Chromium, which rasterises in software; numbers describe that environment, so compare runs on the same machine while it is otherwise quiet. The load average is recorded before and after pacing.

**Limits.** Layout shift depends on load timing and varies slightly between identical builds; the contact sheet marks a CLS rise as worse only above 0.02. The pointer target is one representative mark per chart, not every interaction.
