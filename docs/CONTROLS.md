# Chart controls (playable charts and version selectors)

RichView charts carry a declarative `controls` block that lets a reader play an
ordered dimension, filter rows, swap the measured value, or switch to a named
variant of the chart — without leaving the report page or editing source.
Controls are *authored*, not drag-and-drop: the author lists the dimensions a
reader may change and their options, and the engine renders restrained chrome
above the chart and resolves each state deterministically.

This is chart-local version selection. Dashboard-wide cross-chart selection
belongs to a separate track.

## Where controls live

- **In `.rmd` chart source**: a `controls:` list beside `encoding`, `axes`,
  `annotations`.
- **In the engine `Spec`**: `controls: Option<ControlsSpec>` (richview-chart);
  typed with `deny_unknown_fields`, so a typo fails loudly with the field name.
- **On the rendered chart element** (`crates/richview-site`): a `data-rvc-controls`
  JSON envelope attribute plus two DOM CustomEvents on the chart root
  (`rv:control` is dispatched on every reader state change; `rv:control-set` is
  listened for as the imperative setter). Gallery and dashboard consumers read
  this envelope and dispatch/observe these events only; they never touch
  engine internals.

The engine never fabricates observations to make animation smoother. Each frame
is a resolution of the authored data at that control state. Missing values stay
missing. Membership that appears mid-playback enters at its real position;
membership that leaves fades out at its last observation.

## Dimension kinds

Each entry in `controls:` is one *dimension*. Four kinds, resolved by the
engine before layout:

### `playback` — ordered frames with play/pause

```yaml
controls:
  - kind: playback
    id: year                 # defaults to "time" or the first ordinal field
    field: year              # column in the chart's data
    label: "Year"            # shown beside the current value
    speed: 1200              # ms per frame; default 800
    loop: true               # once | true
```

A play/pause button, a frame scrubber, and a visible current-value label mount
above the chart. The current value is announced with `aria-live="polite"`.
Keyboard: ← / → scrub by one frame; Space toggles play.

Playback resolves to a *view window* over the primary axis for time-series
charts (line, area), so stacked series keep stable mark identity and tooltip
semantics as the window moves. For charts whose plot does not use the frame
column as an axis (bar/scatter/pie), playback resolves to a row filter
`frame_column == current_frame`.

`prefers-reduced-motion: reduce` is a **hard floor**: playback mounts, but
auto-play is disabled and transitions are stepped (not animated). The scrubber
still works; the current value still updates.

### `filter` — restrict rows by a categorical column

```yaml
controls:
  - kind: filter
    id: region
    field: region
    label: "Region"
    options: [Europe, Asia, Americas, Africa, Oceania]
    initial: Europe
    as: buttons              # buttons | dropdown
```

One-of semantics. Multi-select is deliberately deferred until a second author
asks for it; a single-choice button group reads cleanly and keeps the engine
free of combinatorial state.

### `metric` — swap value column with its axis label, unit, format, scale

```yaml
controls:
  - kind: metric
    id: measure
    label: "Measure"
    options:
      - value: emissions_abs
        label: "Absolute"
        field: emissions_mt
        axis: { label: "Emissions", unit: "Mt", format: ".0f" }
      - value: emissions_per_cap
        label: "Per capita"
        field: emissions_per_cap_t
        axis: { label: "Emissions per capita", unit: "t", format: ".2f" }
```

A metric option carries the *whole* axis fragment it needs — label, unit,
format, scale, decimals — so values on the chart and in tooltips never drift
from their labels. Changing the metric reruns the scale fit; it does not mutate
source data.

### `variant` — swap the whole chart

```yaml
controls:
  - kind: variant
    id: view
    label: "View"
    initial: bars
    options:
      - value: bars
        label: "Bars"
        chart:
          type: bar
          encoding: { x: country, y: emissions }
          axes: { y: { label: Emissions, suffix: Mt } }
      - value: share
        label: "Share"
        chart:
          type: pie
          encoding: { x: country, y: emissions }
```

Each option carries a complete chart fragment that **overlays the base chart**
when the reader selects it. Every top-level key in `chart:` replaces the
corresponding key on the base (`type`, `encoding`, `axes`, `data`, …); keys
absent from the option are preserved from the base. The outer `controls:` block
is **always preserved** so other dimensions (filter, metric, playback) keep
working after the swap.

Variants resolve at the `.rmd` layer *before* the engine runs, so the chosen
chart's encoding/axes drive the render; playback/filter/metric declared on the
base chart then compose with the swapped-in chart (the resolver runs variants
first, then the other kinds).

A `variant` dimension is a selector, so it follows the same presentation rules
as `filter` and `metric` below: primary selector = buttons, additional = dropdown
by default, overridable with `as: buttons | dropdown`.

## Chrome and composition

- Every `playback` dimension renders as its own chrome (prev/play/next + a
  scrubber + an `aria-live` current-value readout). Playback does not count
  towards the selector-presentation defaults below.
- The first non-playback selector dimension (first `filter`/`metric`/`variant`
  in author order) renders as a visible button group.
- Additional non-playback selector dimensions default to a native `<select>`
  dropdown, so the primary selector stays obvious at a glance.
- Author overrides per dimension with `as: buttons | dropdown` when cardinality
  or screen width makes a different presentation necessary.
- Controls mount above the chart, in the same chrome band as the existing
  `transforms` / `zero_toggle` toggles. They do not shift the chart body.

## JS envelope (consumer API)

The chart root element (`div.rvc`) carries a `data-rvc-controls` attribute
whose value is a JSON object. The attribute matches the existing
`data-rvc-*` convention used for `data-rvc-staging`, `data-rvc-enter` and
`data-rvc-diagnostics`, so consumers reading one dataset entry can read the
others the same way.

```ts
// Serialized on the chart root at render time. In JS, read as
// `rootEl.dataset.rvcControls`.
JSON.parse(rootEl.dataset.rvcControls) === {
  dimensions: [
    { kind: "playback", id: "year", label: "Year", field: "year", speed: 1200, loop: true },
    { kind: "metric", id: "measure", label: "Measure",
      options: [{ value: "emissions_abs", label: "Absolute", field: "emissions_mt", axis: {...} }, …],
      initial: "emissions_abs" },
  ],
}

// Observe reader state changes. The chart root dispatches a bubbling
// `rv:control` CustomEvent after every state change — click, playback
// step, programmatic set.
rootEl.addEventListener("rv:control", (ev) => {
  // ev.detail = { id, value }
})

// Imperative set from outside the chart: dispatch a bubbling
// `rv:control-set` CustomEvent on the chart root. Drives the reader
// state through the same path a click or scrubber step would take;
// the chart re-renders, button `aria-pressed` and the playback readout
// sync, and the chart then emits its own `rv:control` event.
rootEl.dispatchEvent(new CustomEvent("rv:control-set", {
  detail: { id: "year", value: "1990" },
}))
```

The envelope and events are stable and additive: new dimension kinds
must not change existing field names. Consumers that only read
`dimensions` can ignore any field they do not recognise. A DOM-only
contract (CustomEvents on the chart root) keeps the attach surface
stable without requiring consumer tracks to teach their JS about Rust
or wasm handles.

### Reduced motion

Playback respects `prefers-reduced-motion: reduce` as a hard floor.
When the reader's system prefers reduced motion, the play button
exists and is keyboard-reachable, but pressing it advances exactly
one frame and never starts an auto-advance interval. Prev and next
step buttons behave the same way under both motion preferences.

## Hard constraints

- Correct values, units, scales and source meaning across every frame and
  variant. The engine never invents observations to smooth animation.
- Shared chart semantics and numerical behaviour stay in the Rust engine;
  the native/WebAssembly boundary and legacy authoring compatibility are
  preserved.
- Reduced motion is a hard floor; keyboard, touch and assistive technology
  reach every control.
