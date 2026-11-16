//! Line charts: one path per series through the author's values, one readout per x, nearest-x
//! targeting, missing values as gaps, and the contract every type shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Cmd, Item, Targeting};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"line","title":"Monthly mentions","encoding":{"x":"month","y":"mentions","color":"company"}})
}

fn rows() -> Vec<Value> {
    vec![
        json!({"month":"2026-04","company":"Anthropic","mentions":1120}),
        json!({"month":"2026-05","company":"Anthropic","mentions":1340}),
        json!({"month":"2026-06","company":"Anthropic","mentions":1580}),
        json!({"month":"2026-04","company":"OpenAI","mentions":2210}),
        json!({"month":"2026-05","company":"OpenAI","mentions":2180}),
        json!({"month":"2026-06","company":"OpenAI","mentions":2060}),
    ]
}

fn single_spec() -> Value {
    json!({"type":"line","title":"Mentions","encoding":{"x":"month","y":"mentions"}})
}

/// The first and last anchor of each series' line, in drawing order.
fn ends(g: &richview_chart::geometry::Geometry) -> Vec<([f64; 2], [f64; 2])> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Line { path, .. } => {
                let first = path.cmds.iter().find_map(|c| match c {
                    Cmd::M { p } => Some(*p),
                    _ => None,
                })?;
                let last = path.cmds.last().and_then(|c| match c {
                    Cmd::M { p } | Cmd::L { p } => Some(*p),
                    Cmd::C { p, .. } => Some(*p),
                    _ => None,
                })?;
                Some((first, last))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &rows());
    contract(&single_spec(), &rows()[..3]);
}

#[test]
fn each_x_reads_every_series_at_the_authors_values_and_targets_the_nearest_x() {
    let g = draw(&spec(), &rows());
    assert_eq!(g.series.len(), 2);
    assert_eq!(g.panels[0].targeting, Targeting::NearestX);
    let obs = observations(&g);
    assert_eq!(obs.len(), 3, "one readout per x, not one per row");
    assert_eq!(obs[0].readout.heading, "Apr 2026");
    assert_eq!(line(obs[0], "Anthropic"), Some("1,120"));
    assert_eq!(line(obs[0], "OpenAI"), Some("2,210"));
    assert_eq!(line(obs[2], "OpenAI"), Some("2,060"));
    assert!(obs[0].x < obs[1].x && obs[1].x < obs[2].x, "x runs left to right");
    // The month axis is calendar-exact: May sits where its date puts it, not midway.
    assert!(
        (obs[1].x - obs[0].x) / (obs[2].x - obs[1].x) > 0.9,
        "Apr to May is 30 days and May to Jun 31"
    );
}

#[test]
fn every_series_path_starts_and_ends_at_its_first_and_last_observation() {
    let g = draw(&spec(), &rows());
    let paths = ends(&g);
    assert_eq!(paths.len(), 2, "one path per series");
    let obs = observations(&g);
    for (first, last) in &paths {
        assert!((first[0] - obs[0].x).abs() < 0.01, "{first:?} starts at the first x");
        assert!((last[0] - obs[2].x).abs() < 0.01, "{last:?} ends at the last x");
    }
    // OpenAI's values are larger, so its line sits above Anthropic's at every x (smaller y is higher).
    assert!(paths[1].0[1] < paths[0].0[1] && paths[1].1[1] < paths[0].1[1]);
}

#[test]
fn a_larger_value_is_drawn_higher() {
    let g = draw(&single_spec(), &rows()[..3]);
    let (first, last) = ends(&g)[0];
    assert!(last[1] < first[1], "1,580 is above 1,120: {first:?} to {last:?}");
}

#[test]
fn missing_values_are_disclosed_and_bad_text_fails_the_chart() {
    let mut rows = rows();
    rows[1] = json!({"month":"2026-05","company":"Anthropic","mentions":null});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_none(), "a missing value leaves a gap, not a failed chart");
    assert!(check(&g, "missing-observations").is_some(), "{:?}", g.checks);
    rows[1] = json!({"month":"2026-05","company":"Anthropic","mentions":"1,340"});
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn a_repeated_x_in_one_series_is_a_source_error() {
    let mut rows = rows();
    rows.push(json!({"month":"2026-04","company":"Anthropic","mentions":9}));
    let g = draw(&spec(), &rows);
    assert!(g.failure.is_some());
    assert_eq!(
        g.checks.iter().map(|c| c.kind.as_str()).collect::<Vec<_>>(),
        ["source-error"]
    );
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&single_spec(), &[]);
    contract(&single_spec(), &[json!({"month":"2026-04","mentions":5})]);
    contract(
        &single_spec(),
        &[
            json!({"month":"2026-04","mentions":-5}),
            json!({"month":"2026-05","mentions":-7}),
            json!({"month":"2026-06","mentions":0}),
        ],
    );
    let g = draw(
        &single_spec(),
        &[
            json!({"month":"2026-04","mentions":1e300}),
            json!({"month":"2026-05","mentions":-5e299}),
        ],
    );
    assert!(nonfinite(&g).is_none());
}

#[test]
fn dense_series_keep_every_observation_reachable() {
    let rows: Vec<Value> = (0..400)
        .map(|i| json!({"month": format!("2025-{:02}-{:02}", i / 28 % 12 + 1, i % 28 + 1), "mentions": ((i * 37) % 101) as f64, "company": format!("S{}", i / 200)}))
        .collect();
    // Not every generated date is distinct per series; keep the first row of each (series, date).
    let mut seen = std::collections::HashSet::new();
    let rows: Vec<Value> = rows
        .into_iter()
        .filter(|r| {
            seen.insert((
                r["company"].as_str().unwrap().to_string(),
                r["month"].as_str().unwrap().to_string(),
            ))
        })
        .collect();
    let g = draw_at(&spec(), &rows, 390.0);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(nonfinite(&g).is_none());
    let order = richview_chart::hit::order(&g);
    assert_eq!(order.len(), observations(&g).len());
}

#[test]
fn step_and_linear_curves_draw_the_same_observations() {
    for curve in ["monotone", "linear", "step", "step_before", "step_after"] {
        let mut s = spec();
        s["style"] = json!({"curve": curve});
        let g = draw(&s, &rows());
        assert_eq!(observations(&g).len(), 3, "{curve}");
        contract(&s, &rows());
    }
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("line", &spec(), &rows());
}

#[test]
fn the_runtime_keeps_values_a_grid_step_apart() {
    let rows = vec![
        json!({"month":"2026-04","mentions":100.01}),
        json!({"month":"2026-05","mentions":1000}),
        json!({"month":"2026-06","mentions":100.04}),
    ];
    runtime_matches_native(&single_spec(), &rows);
}
