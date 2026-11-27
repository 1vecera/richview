//! Dot plots: one dot for every observation, stacked in equal-width intervals, counted on the
//! vertical axis, and the contract every type shares (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::Item;
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/dotplot.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn spec() -> Value {
    json!({"type":"dotplot","title":"T","encoding":{"x":"v"},"axes":{"x":{"label":"Value"}}})
}

fn values(xs: &[f64]) -> Vec<Value> {
    xs.iter().map(|v| json!({"v": v})).collect()
}

fn dots(g: &richview_chart::geometry::Geometry) -> Vec<([f64; 2], f64)> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Point { center, radius, .. } => Some((*center, *radius)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("dotplot", &spec, &rows);
}

#[test]
fn every_observation_is_one_dot_and_the_counts_add_up() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(dots(&g).len(), rows.len());
    let counted: usize = observations(&g)
        .iter()
        .flat_map(|o| &o.readout.lines)
        .map(|l| l.value.parse::<usize>().unwrap())
        .sum();
    assert_eq!(counted, rows.len());
    assert!(
        observations(&g)
            .iter()
            .all(|o| o.readout.lines.iter().all(|l| l.derived))
    );
}

#[test]
fn equal_values_stack_in_one_column_from_the_axis() {
    let rows = values(&[0.0, 0.0, 0.0, 10.0, 10.0]);
    let g = draw(&spec(), &rows);
    let obs = observations(&g);
    assert_eq!(obs.len(), 2);
    assert_eq!(line(obs[0], "Observations"), Some("3"));
    assert_eq!(line(obs[1], "Observations"), Some("2"));
    let mut column: Vec<([f64; 2], f64)> = dots(&g)
        .into_iter()
        .filter(|(c, _)| (c[0] - obs[0].x).abs() < 0.01)
        .collect();
    assert_eq!(column.len(), 3);
    column.sort_by(|a, b| b.0[1].total_cmp(&a.0[1]));
    for pair in column.windows(2) {
        // Higher dots are further up and never overlap the dot below.
        assert!(pair[0].0[1] - pair[1].0[1] >= 2.0 * pair[0].1 - 1e-6);
    }
    let baseline = g.panels[0].plot.bottom();
    assert!(column[0].0[1] < baseline && column[0].0[1] + column[0].1 <= baseline + 1e-6);
}

#[test]
fn groups_stack_on_each_other_in_series_order() {
    let mut s = spec();
    s["encoding"]["color"] = json!("g");
    let rows = vec![
        json!({"v": 0.0, "g": "A"}),
        json!({"v": 0.0, "g": "A"}),
        json!({"v": 0.0, "g": "B"}),
        json!({"v": 10.0, "g": "B"}),
    ];
    let g = draw(&s, &rows);
    assert_eq!(g.series.len(), 2);
    assert!(g.legend.is_some());
    let first = observations(&g)[0];
    assert_eq!(line(first, "A"), Some("2"));
    assert_eq!(line(first, "B"), Some("1"));
    contract(&s, &rows);
}

#[test]
fn one_value_is_one_dot() {
    let rows = values(&[4.0]);
    let g = draw(&spec(), &rows);
    assert_eq!(dots(&g).len(), 1);
    assert_eq!(line(observations(&g)[0], "Observations"), Some("1"));
    contract(&spec(), &rows);
    contract(&spec(), &values(&[3.0, 3.0, 3.0]));
}

#[test]
fn a_dense_sample_stays_inside_the_plot() {
    let rows = values(&(0..400).map(|i| (i % 7) as f64).collect::<Vec<_>>());
    contract(&spec(), &rows);
    let g = draw(&spec(), &rows);
    let plot = g.panels[0].plot;
    for (c, r) in dots(&g) {
        assert!(c[1] - r >= plot.y - 1.0 && c[1] + r <= plot.bottom() + 1.0);
    }
}

#[test]
fn missing_values_are_disclosed_and_bad_text_fails_the_panel() {
    let mut rows = values(&[1.0, 2.0, 3.0]);
    rows.push(json!({"v": null}));
    let g = draw(&spec(), &rows);
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(dots(&g).len(), 3);
    rows.push(json!({"v": "many"}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    stays_visible(&spec(), &[]);
    stays_visible(&spec(), &[json!({"v": null})]);
}

#[test]
fn extreme_ranges_stay_finite() {
    let g = draw(&spec(), &values(&[-1e300, 0.0, 1e300]));
    assert!(nonfinite(&g).is_none());
    let g = draw(&spec(), &values(&[1e-300, 2e-300, 3e-300]));
    assert!(nonfinite(&g).is_none());
}

#[test]
fn values_on_interval_edges_stay_in_the_exact_intervals_on_the_grid() {
    // Thirty intervals over 0 to 30 put every whole number on an edge; a grid step can push one
    // into the interval below, but the runtime keeps the exact rows' counts.
    let xs: Vec<f64> = (0..=30).map(f64::from).collect();
    runtime_matches_native(&spec(), &values(&xs));
}
