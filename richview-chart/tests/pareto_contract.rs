//! Pareto charts: sorting, the running share on its own axis, the readouts and the shared
//! contract (FEA-CH-01, FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Role};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"pareto","title":"Defects","encoding":{"x":"cause","y":"defects"}})
}

fn rows() -> Vec<Value> {
    vec![
        json!({"cause":"Scratches","defects":20}),
        json!({"cause":"Misalignment","defects":50}),
        json!({"cause":"Cracks","defects":10}),
        json!({"cause":"Discolouration","defects":20}),
    ]
}

/// The centres of the points of the cumulative line.
fn line_points(g: &Geometry) -> Vec<[f64; 2]> {
    g.items()
        .filter_map(|i| match i {
            Item::Point {
                center,
                series: Some(1),
                ..
            } => Some(*center),
            _ => None,
        })
        .collect()
}

fn texts(g: &Geometry, role: Role) -> Vec<String> {
    g.items()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == role => Some(t.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_pareto_meets_the_shared_contract() {
    contract(&spec(), &rows());
}

#[test]
fn bars_are_sorted_from_the_largest_down_with_ties_in_source_order() {
    let g = draw(&spec(), &rows());
    let heads: Vec<&str> = observations(&g).iter().map(|o| o.readout.heading.as_str()).collect();
    assert_eq!(heads, ["Misalignment", "Scratches", "Discolouration", "Cracks"]);
    let bars: Vec<_> = g
        .items()
        .filter_map(|i| match i {
            Item::Rect {
                rect,
                series: Some(0),
                role: Role::Mark,
                ..
            } => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(bars.len(), 4);
    assert!(bars.windows(2).all(|w| w[0].x < w[1].x && w[0].h >= w[1].h));
    // 50 against 20: the bar is two and a half times as tall.
    assert!((bars[0].h / bars[1].h - 2.5).abs() < 1e-6);
}

#[test]
fn the_cumulative_line_stands_at_its_true_shares_on_a_0_to_100_axis() {
    let g = draw(&spec(), &rows());
    let plot = g.panels[0].plot;
    let pts = line_points(&g);
    assert_eq!(pts.len(), 4);
    for (p, share) in pts.iter().zip([0.5, 0.7, 0.9, 1.0]) {
        let expected = plot.bottom() - share * plot.h;
        assert!((p[1] - expected).abs() < 1e-6, "{share}: {} vs {expected}", p[1]);
    }
    // 100 % is the top of the plot, and the line ends there above the last bar.
    assert!((pts[3][1] - plot.y).abs() < 1e-6);
    // One point per bar, centred on it.
    let obs = observations(&g);
    for (p, o) in pts.iter().zip(&obs) {
        assert!((p[0] - o.x).abs() < 1e-9);
    }
    let labels = texts(&g, Role::AxisLabel);
    assert!(
        labels.iter().any(|l| l == "100%") && labels.iter().any(|l| l == "60%"),
        "{labels:?}"
    );
    let titles = texts(&g, Role::AxisTitle);
    assert!(titles.iter().any(|t| t == "Cumulative share"), "{titles:?}");
    // The secondary axis draws labels, not gridlines of its own: one grid line per primary tick.
    let grid = g.items().filter(|i| i.role() == Role::Grid).count();
    let primary_ticks = texts(&g, Role::AxisLabel).iter().filter(|l| !l.ends_with('%')).count();
    assert!(
        grid <= primary_ticks,
        "{grid} grid lines for {primary_ticks} primary ticks"
    );
}

#[test]
fn readouts_give_the_value_the_share_and_the_cumulative_share() {
    let g = draw(&spec(), &rows());
    let obs = observations(&g);
    assert_eq!(line(obs[0], "defects"), Some("50"));
    assert_eq!(line(obs[0], "Share"), Some("50%"));
    assert_eq!(line(obs[0], "Cumulative share"), Some("50%"));
    assert_eq!(line(obs[2], "Share"), Some("20%"));
    assert_eq!(line(obs[2], "Cumulative share"), Some("90%"));
    assert_eq!(line(obs[3], "Cumulative share"), Some("100%"));
    let derived = |n: &str| obs[1].readout.lines.iter().find(|l| l.name == n).map(|l| l.derived);
    assert_eq!(derived("defects"), Some(false));
    assert_eq!(derived("Share"), Some(true));
    assert_eq!(derived("Cumulative share"), Some(true));
}

#[test]
fn the_value_axis_includes_zero() {
    let g = draw(
        &spec(),
        &[json!({"cause":"A","defects":1000}), json!({"cause":"B","defects":990})],
    );
    let obs = observations(&g);
    let plot = g.panels[0].plot;
    assert!((obs[0].area.unwrap().bottom() - plot.bottom()).abs() < 1e-6);
    let bar_h: Vec<f64> = g
        .items()
        .filter_map(|i| match i {
            Item::Rect {
                rect,
                series: Some(0),
                role: Role::Mark,
                ..
            } => Some(rect.h),
            _ => None,
        })
        .collect();
    assert!(
        bar_h[1] / bar_h[0] > 0.9,
        "bars start at zero, so 990 is nearly as tall as 1000"
    );
}

#[test]
fn a_negative_value_fails_the_panel_with_a_source_error() {
    let mut r = rows();
    r[2] = json!({"cause":"Cracks","defects":-4});
    let g = draw(&spec(), &r);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn missing_values_are_disclosed_and_text_fails_the_panel() {
    let mut r = rows();
    r[2] = json!({"cause":"Cracks","defects":null});
    let g = draw(&spec(), &r);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 3);
    // Shares are of what is shown: 50 + 20 + 20 = 90.
    assert_eq!(line(observations(&g)[0], "Share"), Some("55.6%"));
    r[2] = json!({"cause":"Cracks","defects":"many"});
    let g = draw(&spec(), &r);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    r[2] = json!({"cause":"Cracks","defects":"1e-999"});
    let g = draw(&spec(), &r);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&spec(), &[]);
    contract(&spec(), &[json!({"cause":"Only","defects":5})]);
    let g = draw(&spec(), &[json!({"cause":"Only","defects":5})]);
    assert_eq!(line(observations(&g)[0], "Cumulative share"), Some("100%"));
    // All zero: bars stay visible, there is no total to take a share of.
    let zeros = [json!({"cause":"A","defects":0}), json!({"cause":"B","defects":0})];
    contract(&spec(), &zeros);
    let g = draw(&spec(), &zeros);
    assert!(line_points(&g).is_empty());
    assert_eq!(line(observations(&g)[0], "Share"), None);
    let g = draw(
        &spec(),
        &[
            json!({"cause":"A","defects":1e300}),
            json!({"cause":"B","defects":5e299}),
        ],
    );
    assert!(nonfinite(&g).is_none());
    assert_eq!(line(observations(&g)[1], "Cumulative share"), Some("100%"));
    // A total that overflows fails the panel.
    let g = draw(
        &spec(),
        &[
            json!({"cause":"A","defects":1.7e308}),
            json!({"cause":"B","defects":1.7e308}),
        ],
    );
    assert!(g.failure.is_some());
    assert!(check(&g, "value-overflow").is_some());
}

#[test]
fn dense_pareto_charts_keep_every_category() {
    let r: Vec<Value> = (0..50)
        .map(|i| json!({"cause": format!("Cause number {i}"), "defects": 100 - i}))
        .collect();
    let g = draw_at(&spec(), &r, 390.0);
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 50);
    assert_eq!(line_points(&g).len(), 50);
}

#[test]
fn facets_sort_and_add_up_on_their_own() {
    let mut s = spec();
    s["encoding"]["facet"] = json!("line");
    let r = vec![
        json!({"cause":"A","defects":1,"line":"North"}),
        json!({"cause":"B","defects":3,"line":"North"}),
        json!({"cause":"A","defects":9,"line":"South"}),
        json!({"cause":"B","defects":1,"line":"South"}),
    ];
    contract(&s, &r);
    let g = draw(&s, &r);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(g.panels[0].observations[0].readout.heading, "B");
    assert_eq!(g.panels[1].observations[0].readout.heading, "A");
    assert_eq!(line(&g.panels[1].observations[0], "Cumulative share"), Some("90%"));
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("pareto", &spec(), &rows());
}

#[test]
fn the_runtime_ranks_values_closer_than_a_grid_step_like_the_exact_rows() {
    // 100.01 and 100.04 share a step of the 4,096-step grid over 0..1000.
    let rows = vec![
        json!({"cause":"Beta","defects":100.01}),
        json!({"cause":"Alpha","defects":100.04}),
        json!({"cause":"Gamma","defects":1000}),
        json!({"cause":"Delta","defects":5}),
    ];
    runtime_matches_native(&spec(), &rows);
}
