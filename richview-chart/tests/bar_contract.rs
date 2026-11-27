//! Bar charts: one bar per category from the baseline, heights in proportion to the values,
//! groups side by side, signs, orientation, missing values and the contract every type shares
//! (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Item, Rect};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"bar","title":"Share of mentions","encoding":{"x":"year","y":"share"},"axes":{"y":{"label":"Share (%)"}}})
}

fn rows() -> Vec<Value> {
    vec![
        json!({"year":"2023","share":3.0}),
        json!({"year":"2024","share":18.2}),
        json!({"year":"2025","share":42.8}),
        json!({"year":"2026","share":68.2}),
    ]
}

fn grouped_spec() -> Value {
    let mut s = spec();
    s["encoding"]["color"] = json!("company");
    s
}

fn grouped_rows() -> Vec<Value> {
    vec![
        json!({"year":"2025","company":"Anthropic","share":42.8}),
        json!({"year":"2025","company":"OpenAI","share":57.2}),
        json!({"year":"2026","company":"Anthropic","share":68.2}),
        json!({"year":"2026","company":"OpenAI","share":31.8}),
    ]
}

/// The painted bars, in drawing order.
fn bars(g: &richview_chart::geometry::Geometry) -> Vec<Rect> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &rows());
    contract(&grouped_spec(), &grouped_rows());
}

#[test]
fn bar_heights_are_in_proportion_to_the_values_and_stand_on_one_baseline() {
    let g = draw(&spec(), &rows());
    let bars = bars(&g);
    assert_eq!(bars.len(), 4);
    let baseline = bars[0].bottom();
    for bar in &bars {
        assert!((bar.bottom() - baseline).abs() < 0.01, "{bar:?} stands on {baseline}");
    }
    // A bar stops about a pixel short of its baseline (the surface gap), so heights follow the values
    // to within two pixels, measured from the tallest.
    let unit = bars[3].h / 68.2;
    for (bar, value) in bars.iter().zip([3.0, 18.2, 42.8, 68.2]) {
        assert!(
            (bar.h - unit * value).abs() < 2.0,
            "{value} is drawn {} px high, not {}",
            bar.h,
            unit * value
        );
    }
    let obs = observations(&g);
    assert_eq!(obs[0].readout.heading, "2023");
    assert_eq!(line(obs[0], "Share (%)"), Some("3"));
    assert_eq!(line(obs[1], "Share (%)"), Some("18.2"));
}

#[test]
fn a_negative_bar_hangs_below_the_baseline() {
    let rows = vec![json!({"year":"2023","share":10}), json!({"year":"2024","share":-5})];
    let g = draw(&spec(), &rows);
    let bars = bars(&g);
    assert_eq!(bars.len(), 2);
    assert!(
        (bars[1].y - bars[0].bottom()).abs() < 2.0,
        "the negative bar starts at the baseline (less the surface gap)"
    );
    assert!(
        bars[1].h > 0.0 && (bars[1].h / bars[0].h - 0.5).abs() < 0.01,
        "−5 is half of 10"
    );
    assert_eq!(line(observations(&g)[1], "Share (%)"), Some("−5"));
    contract(&spec(), &rows);
}

#[test]
fn groups_sit_side_by_side_in_one_band() {
    let g = draw(&grouped_spec(), &grouped_rows());
    assert_eq!(g.series.len(), 2);
    let obs = observations(&g);
    assert_eq!(obs.len(), 4);
    assert_eq!(line(obs[0], "Anthropic"), Some("42.8"));
    assert_eq!(line(obs[1], "OpenAI"), Some("57.2"));
    assert!(
        obs[0].x < obs[1].x && obs[1].x < obs[2].x,
        "2025's two bars come before 2026's"
    );
    assert!(
        obs[1].x - obs[0].x < obs[2].x - obs[1].x,
        "the bars of one year are closer than the years"
    );
}

#[test]
fn a_missing_value_is_a_disclosed_gap_and_bad_text_fails_the_chart() {
    let mut rows = rows();
    rows[1] = json!({"year":"2024","share":null});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "missing-observations").is_some(), "{:?}", g.checks);
    assert_eq!(bars(&g).len(), 3, "no bar is drawn for the missing value");
    assert_eq!(observations(&g).len(), 4, "its category stays reachable");
    rows[1] = json!({"year":"2024","share":"1,000"});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn two_rows_for_one_bar_slot_are_a_source_error() {
    let mut rows = rows();
    rows.push(json!({"year":"2023","share":9}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "ambiguous-bar-slot").is_some(), "{:?}", g.checks);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    contract(&spec(), &[json!({"year":"2023","share":5})]);
    contract(
        &spec(),
        &[json!({"year":"2023","share":-5}), json!({"year":"2024","share":-7})],
    );
    // A zero is no bar, but its category stays reachable.
    let g = draw(
        &spec(),
        &[json!({"year":"2023","share":4}), json!({"year":"2024","share":0})],
    );
    assert_eq!(observations(&g).len(), 2);
    assert!(observations(&g)[1].area.unwrap().h >= 1.0);
    let g = draw(
        &spec(),
        &[
            json!({"year":"2023","share":1e300}),
            json!({"year":"2024","share":-5e299}),
        ],
    );
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
}

#[test]
fn dense_charts_keep_every_category() {
    let rows: Vec<Value> = (0..60)
        .map(|i| json!({"year": format!("Category number {i}"), "share": (i % 7) as f64 - 3.0}))
        .collect();
    let g = draw_at(&spec(), &rows, 390.0);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 60);
    assert!(check(&g, "clipping").is_none() && check(&g, "label-collision").is_none());
}

#[test]
fn horizontal_bars_run_along_x_and_list_categories_top_to_bottom() {
    let mut s = spec();
    s["style"] = json!({"orientation":"horizontal"});
    let g = draw(&s, &rows());
    let bars = bars(&g);
    assert!(
        bars.iter().all(|b| (b.x - bars[0].x).abs() < 0.01),
        "bars start at one baseline"
    );
    assert!(bars[0].w < bars[1].w && bars[1].w < bars[2].w && bars[2].w < bars[3].w);
    let ys: Vec<f64> = observations(&g).iter().map(|o| o.area.unwrap().center()[1]).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    contract(&s, &rows());
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("bar", &spec(), &rows());
    gallery("bar-grouped", &grouped_spec(), &grouped_rows());
}

#[test]
fn the_runtime_keeps_the_axis_of_values_a_grid_step_apart() {
    let rows = vec![
        json!({"year":"2023","share":100.01}),
        json!({"year":"2024","share":1000}),
        json!({"year":"2025","share":100.04}),
        json!({"year":"2026","share":0.5}),
    ];
    runtime_matches_native(&spec(), &rows);
}
