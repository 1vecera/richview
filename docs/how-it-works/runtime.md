# Report reader

## Overview

The Report reader ([`richview-runtime`](../../crates/richview-runtime/src/lib.rs)) is the Rust program that runs in a browser as WebAssembly (Wasm) when someone opens an interactive report. It reads the figures and chart data that the [Report renderer](core.md) (`richview-core`) puts in the page, asks the [Chart engine](chart-engine.md) (`richview-chart`) for a drawing at the figure's width, and displays it through WebGL2, the browser's graphics interface. Readers can inspect a mark, change a chart control, open its Table or download its data. An offline HTML report file carries the reader code, while the [Cowork report server](server.md) (`richview-app`) can link to its own copy and the [Playground](site.md) (`richview-site`) runs it inside the preview frame. The Report reader never parses the original Rich Markdown or writes changes back to the Report folder. [Crate dependencies](../../crates/richview-runtime/Cargo.toml#L12) · [Runtime loader](../../crates/richview-core/src/render.rs#L1817)

## Module structure

- [`richview-runtime/src/lib.rs`](../../crates/richview-runtime/src/lib.rs) separates the chart calculations from the modules that need browser APIs.
  - [`payload.rs`](../../crates/richview-runtime/src/payload.rs) decodes a figure's embedded chart data and asks the Chart engine for its geometry.
  - [`mesh.rs`](../../crates/richview-runtime/src/mesh.rs) turns that geometry into a mesh, the vertices and drawing instructions sent to the graphics processor.
  - [`readout.rs`](../../crates/richview-runtime/src/readout.rs) chooses the lines and series cues for the value shown when a reader selects a mark.
  - [`tween.rs`](../../crates/richview-runtime/src/tween.rs) matches marks between successive layouts so they can move between their old and new positions.
  - [`data.rs`](../../crates/richview-runtime/src/data.rs) makes the chart's Table cells and comma-separated values (CSV) download from the display values in the payload.
  - [`web/mod.rs`](../../crates/richview-runtime/src/web/mod.rs) starts the reader and connects it to page events.
    - [`host.rs`](../../crates/richview-runtime/src/web/host.rs) owns each figure's view, canvas, selection, zoom and actions.
      - [`host/lazy.rs`](../../crates/richview-runtime/src/web/host/lazy.rs) schedules charts nearest the screen first and releases distant drawing surfaces.
      - [`host/controls.rs`](../../crates/richview-runtime/src/web/host/controls.rs) applies authored controls and synchronizes the figure with its expanded view.
      - [`host/focus.rs`](../../crates/richview-runtime/src/web/host/focus.rs) draws the outline that identifies the keyboard-selected mark.
      - [`host/slide.rs`](../../crates/richview-runtime/src/web/host/slide.rs) fits a chart to the space available on a slide.
    - [`gl.rs`](../../crates/richview-runtime/src/web/gl.rs) draws chart meshes on one shared WebGL2 canvas and copies the result to each figure.
    - [`atlas.rs`](../../crates/richview-runtime/src/web/atlas.rs) draws labels into a temporary image called a text atlas. The reader sizes each cell for both the layout bounds and the ink the browser measures, so a facet title in a fallback font has room for every letter. [Cell measurement](../../crates/richview-runtime/src/web/atlas.rs#L71)
    - [`tables.rs`](../../crates/richview-runtime/src/web/tables.rs) sorts and filters authored report tables, preserving the author's order when displayed values tie.
    - [`tabs.rs`](../../crates/richview-runtime/src/web/tabs.rs) shows the selected report tab and reveals the panel named in a link.
    - [`slides.rs`](../../crates/richview-runtime/src/web/slides.rs) handles slide navigation, the slide counter, full screen and the slide number in the address.
    - [`reading.rs`](../../crates/richview-runtime/src/web/reading.rs) tracks the current section and the share of the page read.
    - [`scrolly.rs`](../../crates/richview-runtime/src/web/scrolly.rs) changes the figure's state when a story step reaches the middle of the screen or gains keyboard focus.
    - [`entrance.rs`](../../crates/richview-runtime/src/web/entrance.rs) reveals eligible blocks once as they come into view.
    - [`embeds.rs`](../../crates/richview-runtime/src/web/embeds.rs) loads an external page in a sandboxed frame when the reader selects its load button.
    - [`preview.rs`](../../crates/richview-runtime/src/web/preview.rs) watches a local preview server for a new revision.

## Architecture

![The Chart engine combines embedded rows, reader controls and frame width to produce geometry. The Report reader draws the geometry on a shared WebGL2 canvas and copies the pixels to the figure. Inspection uses its observations, while the Table and CSV read the embedded rows.](diagrams/runtime.svg)

Each figure has two ways to answer a question about a value. Hovering over the September bar in the [orders example](../../examples/first-report/report.rmd#L18) selects an observation in the current geometry and shows its readout. Opening the Table reads the embedded rows directly, so the values remain available even when the chart draws only representative marks. For the picture itself, the Report reader draws into one shared canvas and copies the pixels to the visible canvas in the orders figure. [Pointer selection](../../richview-chart/src/hit.rs#L158) · [Table data](../../crates/richview-runtime/src/data.rs#L113) · [Canvas copy](../../crates/richview-runtime/src/web/gl.rs#L724)

## Pipeline

1. **Start the page.** The loader starts Wasm from the bytes in the HTML report file or from the runtime location the host supplied. The Wasm entry point `start` marks the document as started, activates its reading controls and registers print and motion-preference listeners. [render.rs:1817](../../crates/richview-core/src/render.rs#L1817) · [web/mod.rs:149](../../crates/richview-runtime/src/web/mod.rs#L149)

1. **Find the figures.** `host::start` finds each `figure[data-rv-chart]`. It chooses how to mount them, meaning when to attach their canvases and controls. [web/host.rs:447](../../crates/richview-runtime/src/web/host.rs#L447)

   - **Interactive report with viewport observers.** The reader queues figures and starts with the closest to the screen. A faraway figure can have a model while holding no drawn pixels. [host/lazy.rs:40](../../crates/richview-runtime/src/web/host/lazy.rs#L40)
   - **Other output or observers unavailable.** The reader mounts every figure immediately. Printing or calling `settled()` also drains an existing queue. [web/host.rs:455](../../crates/richview-runtime/src/web/host.rs#L455) · [host/lazy.rs:115](../../crates/richview-runtime/src/web/host/lazy.rs#L115)

1. **Read the data and attach a view.** `mount_as` finds the figure's frame and JSON script. `Chart::parse` decodes the block, position data and author-precision values, then initializes the controls. With a usable renderer, the frame gains a canvas, a keyboard focus target and a live region where a screen reader can announce selections. An unreadable payload or unavailable WebGL2 renderer leaves a chart notice. [web/host.rs:589](../../crates/richview-runtime/src/web/host.rs#L589) · [payload.rs:97](../../crates/richview-runtime/src/payload.rs#L97)

1. **Lay out the figure.** `layout_pass` measures the frame in CSS pixels and passes its width, hidden series and zoom to the Chart engine. `geometry_with_observations` combines positions with the text the author chose to expose. It reuses prepared rows for eligible changes of width or zoom. [web/host.rs:828](../../crates/richview-runtime/src/web/host.rs#L828) · [payload.rs:403](../../crates/richview-runtime/src/payload.rs#L403)

1. **Draw the frame.** `mesh::build` turns the geometry into drawing instructions. `atlas::build` supplies the label image and `draw` uploads the mesh and image as needed. The shared renderer draws the frame and `present` copies the pixels into the figure's visible canvas. [mesh.rs:1145](../../crates/richview-runtime/src/mesh.rs#L1145) · [web/atlas.rs:90](../../crates/richview-runtime/src/web/atlas.rs#L90) · [web/host.rs:1919](../../crates/richview-runtime/src/web/host.rs#L1919) · [web/gl.rs:724](../../crates/richview-runtime/src/web/gl.rs#L724)

1. **Respond to the reader.** Once mounted, the figure’s [event handlers](../../crates/richview-runtime/src/web/host.rs#L3984) select the action.

   - **Inspect a mark.** The pointer or keyboard selects an observation. `show_readout` turns its heading and lines into visible text and can announce the selection. [web/host.rs:3984](../../crates/richview-runtime/src/web/host.rs#L3984) · [web/host.rs:3035](../../crates/richview-runtime/src/web/host.rs#L3035)
   - **Change what is drawn.** A resize requests a new layout at the new width. A control change calls `apply`, which updates the chart state and its view. [web/host.rs:2333](../../crates/richview-runtime/src/web/host.rs#L2333) · [host/controls.rs:307](../../crates/richview-runtime/src/web/host/controls.rs#L307)
   - **Read rows or download.** The chart's Table initially uses the rows in the selected control state and can show all observations. CSV writes all embedded rows in column order. The PNG image download draws the current geometry at rest. [web/host.rs:5084](../../crates/richview-runtime/src/web/host.rs#L5084) · [data.rs:33](../../crates/richview-runtime/src/data.rs#L33) · [web/host.rs:5206](../../crates/richview-runtime/src/web/host.rs#L5206)
   - **Print the open report.** The browser's print event calls `print`, which mounts waiting charts, hides their readouts and applies the paper theme. Returning to the page restores the screen theme. The [Command-line program](cli.md)'s PDF export has its own [vector drawing path](chart-engine.md#pipeline). [web/host.rs:2161](../../crates/richview-runtime/src/web/host.rs#L2161) · [web/host.rs:2298](../../crates/richview-runtime/src/web/host.rs#L2298)

## What is stored where, and what is rebuilt

| Thing | Where it lives | Who writes it | When it changes |
| --- | --- | --- | --- |
| Chart definition, rows and theme | The figure's [JSON script](../../crates/richview-core/src/figure.rs#L895) in the HTML report file | The Report renderer | Regenerating the report changes the stored input. Reader actions use an in-memory copy. |
| Runtime program | Embedded bytes in the offline page or a [host-supplied runtime location](../../crates/richview-core/src/render.rs#L1817) | The [build](../../crates/richview-build/src/main.rs#L261) produces the program and the Report renderer embeds or links it | Rebuilding produces new runtime artifacts. An existing offline file retains its embedded copy. |
| Selected rows and reusable calculations | The figure's [`Chart`](../../crates/richview-runtime/src/payload.rs#L257) in Wasm memory | The Report reader | An authored control change replaces the selected rows and discards their cached layout data. |
| Selection, hidden series and zoom | Each figure's [`View`](../../crates/richview-runtime/src/web/host.rs#L653) in page memory | Pointer, keyboard and control handlers | Reloading the report creates fresh chart views. |
| Chart pixels, mesh and graphics buffers | The figure's canvas and its [drawing resources](../../crates/richview-runtime/src/web/host.rs#L1874) | The Report reader and WebGL2 | A distant idle figure releases them. Returning near it restores the surface from its model. |
| Label pixels | A temporary [text atlas](../../crates/richview-runtime/src/web/atlas.rs#L25) and a graphics texture | The Report reader | A layout or font change can require a new atlas. Its temporary canvas releases its pixels after upload. |
| Active tab or slide | The address fragment, such as `#tab=results` or `#slide=2` | The [tab](../../crates/richview-runtime/src/web/tabs.rs#L322) or [slide](../../crates/richview-runtime/src/web/slides.rs#L156) controller | Opening a matching address selects the named panel or numbered slide. |
| Downloaded chart data or image | A file saved by the browser | The reader invokes a [download action](../../crates/richview-runtime/src/web/host.rs#L4720) | Each download creates a file from the open report. Regenerating the report does not update existing downloads. |

A slide link restores a location, while reopening a chart restores the initial control selection in its payload. To preserve an image of the selected chart state, the reader can download the PNG. When a control state has a name, the filename includes it. [Slide address](../../crates/richview-runtime/src/web/slides.rs#L83) · [Initial state](../../crates/richview-runtime/src/payload.rs#L119) · [Image filename](../../crates/richview-runtime/src/web/host.rs#L5247)

## Configuration and limits

**Browser inputs.**

- The Report reader takes its settings from the page, the embedded chart definition and browser preferences.
- It has no reader-specific environment variables or command-line flags.
- The [chart reference](../RMD-SPEC.md#charts) defines the settings an author can supply. [Browser entry point](../../crates/richview-runtime/src/web/mod.rs#L149)

**JavaScript and WebGL2.**

- An interactive chart needs both.
- Its frame contains a notice for the unavailable chart, while the surrounding HTML text remains readable. [Figure frame](../../crates/richview-core/src/figure.rs#L895) · [Renderer failure](../../crates/richview-runtime/src/web/host.rs#L605)

**Drawing distance, 1 and 3 screen heights.**

- A chart becomes eligible to draw when it is within one viewport height of the screen.
- Beyond three screens an idle chart releases its drawing resources.
- Focus, playback, a transition or an open chart dialog can keep those resources in use. [Distance and release rules](../../crates/richview-runtime/src/web/host/lazy.rs)

**Mount schedule, 40 and 250 milliseconds.**

- Faraway charts get a 40 ms pause between scheduled mounts.
- A timer 250 ms after the scheduled delay lets a mount proceed if the browser suspends animation frames.
- A scroll can also bring up to three waiting on-screen figures forward in one event. [Mount scheduler](../../crates/richview-runtime/src/web/host/lazy.rs#L18)

**Screen density, 1–3 device pixels per CSS pixel.**

- A host can lower the upper bound with the `richview-max-dpr` page setting.
- Browser print uses at least 2 pixels per CSS pixel, and the PNG action draws at twice the chart's CSS dimensions. [Density setting](../../crates/richview-runtime/src/web/host.rs#L240) · [Print density](../../crates/richview-runtime/src/web/host.rs#L866) · [PNG sizing](../../crates/richview-runtime/src/web/host.rs#L5212)

**Chart Table, 1,000 rows at a time.**

- The first 1,000 matching rows appear immediately. “Show 1,000 more observations” adds the next batch until all rows in the selected view are visible.
- This display limit does not truncate the CSV. [Table display](../../crates/richview-runtime/src/web/host.rs#L5120) · [CSV writer](../../crates/richview-runtime/src/data.rs#L33)

**Precision while zooming.**

- On a continuous axis with a lossy position grid, the closest zoom keeps a grid interval at about 2 CSS pixels.
- An axis with named positions snaps to whole names and keeps at least two in view.
- The Table, CSV and readout use values at the author's precision. [Zoom clamp](../../crates/richview-runtime/src/payload.rs#L305) · [Display values](../../crates/richview-runtime/src/payload.rs#L471)

**Touch thresholds, 500 milliseconds and 8 pixels.**

- A short tap must finish under both thresholds.
- A selection drag needs more than 6 pixels of extent to define a zoom window. [Tap detection](../../crates/richview-runtime/src/web/host.rs#L4149) · [Drag selection](../../crates/richview-runtime/src/web/host.rs#L2479)

**Expanded charts on phones.**

- A portrait screen with a coarse pointer can turn the expanded chart if the screen is 360–600 CSS pixels wide and at most 1,024 pixels high, and turning gives the chart a better fit.
- Landscape screens up to 1,024 pixels wide and 600 high use the short, wide layout. [Screen conditions](../../richview-chart/src/interaction.rs#L17) · [Fit rules](../../crates/richview-runtime/src/web/host.rs#L1285)

**Chart size in an expanded view.** The reader starts from 96% of the window width, capped at 1,400 CSS pixels, then subtracts the border and padding. It can lay a tall chart out narrower, down to 240 CSS pixels. [Dialog width](../../crates/richview-runtime/src/web/host.rs#L1508) · [Minimum width](../../crates/richview-runtime/src/web/host.rs#L1296)

**Slide size, at least 120 CSS pixels of fitting room.** When the slide fits a chart to a box, the reader tries panel width-to-height ratios from 0.8 to 6. It can narrow the chart or let the slide grow to preserve legibility. [Slide fitting](../../crates/richview-runtime/src/web/host/slide.rs#L129)

**Playback and motion.** Playback advances every 800 ms unless the author sets a different duration. The reader stops on the last frame. Pressing Play there restarts at the first frame. With reduced motion, each press advances one frame. A chart and its expanded view share one playback at a time. Switching on the browser's reduced-motion preference pauses ongoing playback and settles active feedback. [Playback defaults](../../richview-chart/src/reader_controls.rs#L191) · [Play and restart](../../crates/richview-runtime/src/web/host/controls.rs#L190) · [Final frame](../../crates/richview-runtime/src/web/host/controls.rs#L253) · [Motion preference](../../crates/richview-runtime/src/web/host.rs#L2144)

**Block entrances.** The report setting `data-motion="off"` and the browser's reduced-motion preference skip entrance effects. The style sets the block effect and delay between entries. A group appearing together accumulates no more than four delay steps. [Block entrances](../../crates/richview-runtime/src/web/entrance.rs#L13)

**Preview refresh, 500 milliseconds.** Only a page marked with a local revision route runs the poll, with one request in flight. A changed revision or a server response reporting a render error reloads the page. An unreachable server leaves the last render on screen. [Local preview](../../crates/richview-runtime/src/web/preview.rs#L40)

**External embeds.** An external page loads after the reader selects its button. The browser creates the sandboxed frame with the address, height and restrictions supplied by the Report renderer. [Embed loader](../../crates/richview-runtime/src/web/embeds.rs#L21)

**Graphics recovery.** While graphics programs compile, one 32 ms timer at a time checks their status. A graphics context lost during that wait gets a replacement attempt after 1,000 ms if the browser has not restored it. [Compilation polling](../../crates/richview-runtime/src/web/host.rs#L370) · [Context recovery](../../crates/richview-runtime/src/web/host.rs#L295)

**Waiting for charts, 30 seconds.** A caller can use the browser's `window.__richview` object to inspect or wait for a report. [Reader hook](../../crates/richview-runtime/src/web/mod.rs#L75)

- `output` identifies the output mode. It defaults to `report`.

- `pending()` names work the reader is still waiting for.

- `settled()` mounts every chart and requests a frame at rest. It resolves when fonts have loaded and no work is pending. Its 30-second timer rejects if chart work is still pending. [Wait condition](../../crates/richview-runtime/src/web/mod.rs#L129)

- `charts()` returns the geometry and reading state of mounted chart views. [Chart models](../../crates/richview-runtime/src/web/host.rs#L2225)

For example, a caller inspecting an open report can ask for the final frame and read each figure's measured width and selected control state. The results follow document order even when the figures mounted in a different order. [Returned fields](../../crates/richview-runtime/src/web/host.rs#L2225)

```js
const reader = window.__richview;
await reader.settled();

const figures = reader.charts().map(({ figure, width, state }) => ({
  figure,
  width,
  state,
}));
console.table(figures);
```
