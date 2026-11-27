# Adding a chart type

The pattern `waterfall` sets (FEA-CH-75). Read `src/layout/waterfall.rs`, `src/layout/kit.rs` and
`tests/waterfall_contract.rs` first; copy their shape.

1. **Painter** `src/layout/<type>.rs`: `pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted`.
   - Read rows with `rows::num` and `rows::text`. Missing values are disclosed (`excluded-observations`),
     text that is not a number fails the panel with a `non-numeric` check, a value too small to represent
     goes through `ctx.underflow` (FEA-CH-70). A chart never disappears: failures draw a reason.
   - Charts with a calendar or numeric x axis use `kit::Plan::xy`, `kit::read_x`, `kit::xy_frame`, `kit::xy_panel`
     (see `band.rs`); they get zoom and nearest-x targeting.
   - Category-and-value charts use `kit::plan`, `kit::frame`, `kit::panel`, `kit::finish`. Free-form charts
     (no value axis) build their own `Panel` and still use `failed_panel`, `grid_frames` and `panel_title`.
   - Series and colours: `ctx.set_series(&keys)`, then `ctx.series_color(slot)` and `ctx.series[slot].hatch`
     so decals work (FEA-CH-57). Never hard-code a colour; take `ctx.style.*`.
   - Every mark is an `Item` (Rect, Line, Area, Point, Text) with `Role::Mark`; every thing a reader can
     inspect is an `Observation` with a readout (heading, named values, `derived: true` for values RichView
     computed). Hit areas: `area` (Rect) or `wedge`. Never invent a value in a readout (FEA-CH-42).
   - Numbers are formatted with `block.label_format` (axes, labels) and `block.readout_format` (readouts) so
     they keep the author's precision (FEA-CH-37).
   - Set `ctx.summary` (one sentence for the figure's accessible name) and, for computed values, `ctx.note`.
   - Text goes through `ctx.text` / `ctx.text_mid`, which measure with the real font; drop what does not fit
     with `ctx.drop_label`, never clip it.
2. **Table row** in `src/layout/types.rs`: name, default ratio, entrance, what more width gives the type
   (`Wider::ShowsMore` for an x scale of time, ordered categories or values, else `Wider::Enlarges`; only
   the first turns on a phone held upright, FEA-CH-68), painter. Add `mod <type>;` in `src/layout/mod.rs`.
3. **Vocabulary**: add the name to `CHART_TYPES` in `crates/richview-core/src/authoring.rs` (once the schema
   registry is on dev: its `CHART_TYPES` row and any new `encoding.<channel>` field instead). Core's parser
   restricts `lower`/`upper` to listed types; extend that list when your type uses them.
4. **Fixture** `tests/fixtures/types/<type>.json`: `{"spec": {...chart block without data...}, "rows": [...]}`.
   `tests/every_type.rs` runs the shared contract on it at 320, 390 and 1440 px.
5. **Contract tests** `tests/<type>_contract.rs`: `mod common;`, then assertions on the geometry for what is
   only this type's: values at the right coordinates, the readouts, each check firing, orientation, facets,
   edge fixtures (empty, single value, negative, missing, extremes, dense). Include `gallery(...)` in a test
   so `RICHVIEW_GALLERY_OUT=<dir>` writes SVGs; rasterise with `rsvg-convert` and look at them.
6. **Example** `examples/<type>/report.rmd`: a short report with one chart of the type; the browser smoke
   suite mounts every example.
7. **types.md**: change the type's tier cell to `done`.
8. **Docs page** (FEA-SF-43): `docs/public/charts/<type>.md`, same shape as `line.md`, from the example.

Check with `cargo test -p richview-chart --locked` and `cargo test -p richview-core --locked --test integration -- chart_types::`
in your own `CARGO_TARGET_DIR` (the shared one is for the gate), then `cargo fmt --all` and
`cargo clippy -p richview-chart -p richview-core --all-targets --locked -- -D warnings`.

## Landing without breaking dev

Rebases conflict in `layout/types.rs` (one `use super::<module>;` line and one row per type), `layout/mod.rs`,
`authoring.rs` and `types.md`: keep every side's lines once, and check for duplicate lines. Never push while `git status` says a rebase is in progress or any `<<<<<<<` remains.
The shared target can hand you another checkout's stale artifacts: `git ls-files -z '*.rs' '*Cargo.toml' | xargs
-0 touch` before a manual build. After the rebase run `cargo test -p richview-chart --locked` and `cargo test -p
richview-core --locked --test integration -- chart_types::` (the fast gate runs only library tests), then the gate, then push.
