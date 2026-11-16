//! The reader controls contract end to end (FEA-BR-01 to FEA-BR-06): a chart block with
//! `controls` resolves each reader state to a block and its rows, the engine lays them out and the
//! vector painter draws them. Distinct states draw distinct charts, and the resolver never rewrites
//! what the author wrote, so a host holding the authored block and its rows, as the browser
//! runtime does, resolves any state again.

use richview_chart::block::ChartType;
use richview_chart::layout::{ChartTheme, Options, layout};
use richview_chart::reader_controls::{Controls, State};
use richview_chart::vector;
use serde_json::{Value, json};

/// The state that picks each control's option by its authored value, in author order.
fn state(controls: &Controls, picks: &[&str]) -> State {
    controls
        .list
        .iter()
        .zip(picks)
        .map(|(control, pick)| {
            control
                .options
                .iter()
                .position(|choice| choice.value == *pick)
                .unwrap_or_else(|| panic!("{} has no option {pick}", control.id))
        })
        .collect()
}

/// The chart a reader sees in `picks`, drawn as SVG at 720 px from the authored block and rows.
fn draw(chart: &Value, rows: &[Value], picks: &[&str]) -> String {
    let controls = Controls::parse(chart, rows);
    let state = state(&controls, picks);
    let theme = ChartTheme::default();
    let base = Options {
        width: 720.0,
        ..Options::default()
    };
    let options = controls.options(chart, rows, &state, &theme, &base);
    let resolved = controls.chart(chart, rows, &state).expect("the state is a chart");
    vector::svg(&layout(&resolved.block, &resolved.rows, &theme, &options), &theme)
}

fn gapminder() -> (Value, Vec<Value>) {
    let chart = json!({
        "type": "scatter",
        "encoding": {"x": "income", "y": "life", "size": "pop", "color": "region"},
        "axes": {"x": {"label": "Income"}, "y": {"label": "Life expectancy", "unit": "years"}},
        "controls": [
            {"kind": "playback", "id": "year", "field": "year", "initial": "2000"},
            {"kind": "filter", "id": "region", "field": "region",
             "options": ["Europe", "Asia", "Americas"], "initial": "Europe"}
        ]
    });
    let rows = vec![
        json!({"year": "1960", "region": "Europe", "income": 8000, "life": 70, "pop": 400000000}),
        json!({"year": "1960", "region": "Asia", "income": 2000, "life": 55, "pop": 1700000000}),
        json!({"year": "2000", "region": "Europe", "income": 22000, "life": 78, "pop": 500000000}),
        json!({"year": "2000", "region": "Asia", "income": 9000, "life": 72, "pop": 3_700_000_000_i64}),
        json!({"year": "2000", "region": "Americas", "income": 16000, "life": 75, "pop": 850000000}),
    ];
    (chart, rows)
}

/// A reader first sees the authored initial state, here the frame 2000 of Europe; another frame
/// or another region draws another chart.
#[test]
fn gapminder_playback_and_filter_produce_distinct_frames_through_the_renderer() {
    let (chart, rows) = gapminder();
    let controls = Controls::parse(&chart, &rows);
    assert_eq!(controls.initial(), state(&controls, &["2000", "Europe"]));
    let initial = draw(&chart, &rows, &["2000", "Europe"]);
    let early_europe = draw(&chart, &rows, &["1960", "Europe"]);
    let late_asia = draw(&chart, &rows, &["2000", "Asia"]);
    assert!(initial.starts_with("<svg"), "the state draws as SVG");
    assert_ne!(initial, early_europe, "another frame draws another chart");
    assert_ne!(initial, late_asia, "another region draws another chart");
    assert_ne!(early_europe, late_asia, "distinct states draw distinctly");
}

/// Drawing one state twice gives the same SVG, byte for byte.
#[test]
fn same_control_state_renders_deterministically() {
    let (chart, rows) = gapminder();
    assert_eq!(
        draw(&chart, &rows, &["1960", "Asia"]),
        draw(&chart, &rows, &["1960", "Asia"])
    );
}

/// A metric swaps the measured field and the whole value-axis fragment, and keeps every row with
/// both fields, so a host swaps back from what it already holds.
#[test]
fn metric_swap_changes_svg_and_runtime_preserves_source() {
    let chart = json!({
        "type": "bar",
        "encoding": {"x": "country", "y": "emissions_abs"},
        "axes": {"y": {"label": "Emissions", "unit": "Mt"}},
        "controls": [{
            "kind": "metric",
            "id": "measure",
            "options": [
                {"value": "abs", "label": "Total", "field": "emissions_abs",
                 "axis": {"label": "Emissions", "unit": "Mt"}},
                {"value": "per_cap", "label": "Per person", "field": "emissions_per_cap",
                 "axis": {"label": "Emissions per capita", "unit": "t/person"}}
            ]
        }]
    });
    let rows = vec![
        json!({"country": "A", "emissions_abs": 10, "emissions_per_cap": 0.5}),
        json!({"country": "B", "emissions_abs": 20, "emissions_per_cap": 1.0}),
    ];
    assert_ne!(
        draw(&chart, &rows, &["abs"]),
        draw(&chart, &rows, &["per_cap"]),
        "a metric swap draws another chart"
    );
    let controls = Controls::parse(&chart, &rows);
    let per_cap = state(&controls, &["per_cap"]);
    let view = controls.view(&chart, &per_cap);
    assert_eq!(view["encoding"]["y"], "emissions_per_cap");
    assert_eq!(
        view["axes"]["y"],
        json!({"label": "Emissions per capita", "unit": "t/person"})
    );
    assert_eq!(
        chart["encoding"]["y"], "emissions_abs",
        "the authored block stays as written"
    );
    let resolved = controls.chart(&chart, &rows, &per_cap).expect("a bar chart");
    assert_eq!(resolved.rows, rows, "every row, with both fields");
}

/// A variant swaps in a whole partial chart, and the merged view keeps the controls with every
/// option, so a host can swap back.
#[test]
fn variant_swap_changes_svg_and_runtime_preserves_the_authored_variant_block() {
    let chart = json!({
        "type": "bar",
        "encoding": {"x": "country", "y": "emissions"},
        "axes": {"y": {"label": "Emissions", "unit": "Mt"}},
        "controls": [{
            "kind": "variant",
            "id": "view",
            "initial": "bars",
            "options": [
                {"value": "bars", "label": "Bars", "chart": {}},
                {"value": "share", "label": "Share", "chart": {"type": "pie"}}
            ]
        }]
    });
    let rows = vec![
        json!({"country": "A", "emissions": 10}),
        json!({"country": "B", "emissions": 20}),
        json!({"country": "C", "emissions": 30}),
    ];
    assert_ne!(
        draw(&chart, &rows, &["bars"]),
        draw(&chart, &rows, &["share"]),
        "a variant swap draws another chart"
    );
    let controls = Controls::parse(&chart, &rows);
    let view = controls.view(&chart, &state(&controls, &["share"]));
    assert_eq!(view["type"], "pie");
    let options = view["controls"][0]["options"]
        .as_array()
        .expect("the variant block stays");
    assert_eq!(options.len(), 2);
    assert_eq!(chart["type"], "bar", "the authored block stays as written");
}

/// A filter declared on the base chart applies to whichever variant the reader chose.
#[test]
fn variant_composes_with_filter_so_filtered_rows_apply_to_the_chosen_variant() {
    let chart = json!({
        "type": "bar",
        "encoding": {"x": "country", "y": "emissions"},
        "controls": [
            {"kind": "variant", "id": "view", "initial": "bars", "options": [
                {"value": "bars", "label": "Bars", "chart": {}},
                {"value": "share", "label": "Share", "chart": {"type": "pie"}}
            ]},
            {"kind": "filter", "id": "year", "field": "year", "options": ["2020", "2021"], "initial": "2021"}
        ]
    });
    let rows = vec![
        json!({"country": "A", "year": "2020", "emissions": 10}),
        json!({"country": "A", "year": "2021", "emissions": 12}),
        json!({"country": "B", "year": "2020", "emissions": 20}),
        json!({"country": "B", "year": "2021", "emissions": 22}),
    ];
    assert_ne!(
        draw(&chart, &rows, &["bars", "2021"]),
        draw(&chart, &rows, &["share", "2021"])
    );
    let controls = Controls::parse(&chart, &rows);
    for variant in ["bars", "share"] {
        let resolved = controls
            .chart(&chart, &rows, &state(&controls, &[variant, "2021"]))
            .expect("a chart");
        assert_eq!(resolved.kept, [1, 3], "{variant} draws the rows of 2021");
    }
    let share = controls
        .chart(&chart, &rows, &state(&controls, &["share", "2021"]))
        .expect("a pie");
    assert_eq!(share.block.kind, ChartType::Pie);
}

/// A line over its own x reveals the series up to the playback frame.
#[test]
fn time_series_playback_endpoint_shortens_the_view_window() {
    let chart = json!({
        "type": "line",
        "encoding": {"x": "month", "y": "co2"},
        "axes": {"y": {"label": "CO₂", "unit": "ppm"}},
        "controls": [{"kind": "playback", "id": "cursor", "field": "month"}]
    });
    let rows = vec![
        json!({"month": "2024-01-01", "co2": 422.80}),
        json!({"month": "2024-02-01", "co2": 424.55}),
        json!({"month": "2024-03-01", "co2": 425.38}),
        json!({"month": "2024-04-01", "co2": 426.19}),
    ];
    assert_ne!(
        draw(&chart, &rows, &["2024-02-01"]),
        draw(&chart, &rows, &["2024-04-01"]),
        "moving the playback cursor draws another chart"
    );
    let controls = Controls::parse(&chart, &rows);
    let kept = |frame: &str| {
        controls
            .chart(&chart, &rows, &state(&controls, &[frame]))
            .expect("a line chart")
            .kept
    };
    assert_eq!(kept("2024-02-01"), [0, 1], "the series up to the frame");
    assert_eq!(kept("2024-04-01"), [0, 1, 2, 3]);
}
