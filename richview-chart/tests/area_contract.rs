//! Area charts: filled bands under each series from the baseline, overlaid unless `stack: true`
//! (FEA-CH-05), a total in the stacked readout, nearest-x targeting, gaps for missing values and the
//! contract every type shares (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Cmd, Geometry, Item, Targeting};
use serde_json::{Value, json};

fn spec() -> Value {
    json!({"type":"area","title":"Daily active users","encoding":{"x":"day","y":"users","color":"tier"}})
}

fn stacked_spec() -> Value {
    let mut s = spec();
    s["stack"] = json!(true);
    s
}

fn rows() -> Vec<Value> {
    ["Mon", "Tue", "Wed", "Thu"]
        .iter()
        .enumerate()
        .flat_map(|(i, day)| {
            [
                json!({"day":day,"tier":"free","users":820 + i * 30}),
                json!({"day":day,"tier":"paid","users":240 + i * 20}),
            ]
        })
        .collect()
}

fn single_spec() -> Value {
    json!({"type":"area","title":"Users","encoding":{"x":"day","y":"users"}})
}

/// The height above the baseline at the first x of each filled band, in drawing order.
fn first_tops(g: &Geometry) -> Vec<f64> {
    let baseline = g.panels[0]
        .items
        .iter()
        .find_map(|i| match i {
            Item::Line {
                role: richview_chart::geometry::Role::Baseline,
                path,
                ..
            } => path.bounds().map(|b| b.y),
            _ => None,
        })
        .expect("a baseline");
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Area { path, .. } => path.cmds.iter().find_map(|c| match c {
                Cmd::M { p } => Some(baseline - p[1]),
                _ => None,
            }),
            _ => None,
        })
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    contract(&spec(), &rows());
    contract(&stacked_spec(), &rows());
    contract(
        &single_spec(),
        &[json!({"day":"Mon","users":5}), json!({"day":"Tue","users":8})],
    );
}

#[test]
fn bands_overlay_from_the_baseline_unless_stacked() {
    let g = draw(&spec(), &rows());
    assert_eq!(g.panels[0].targeting, Targeting::NearestX);
    let tops = first_tops(&g);
    assert_eq!(tops.len(), 2);
    // Both bands stand on the baseline: free is 820 and paid 240 high at Mon, on one scale.
    assert!((tops[0] / tops[1] - 820.0 / 240.0).abs() < 0.01, "{tops:?}");
    let obs = observations(&g);
    assert_eq!(obs.len(), 4, "one readout per x");
    assert_eq!(line(obs[0], "free"), Some("820"));
    assert_eq!(line(obs[0], "paid"), Some("240"));
    assert_eq!(line(obs[0], "Total"), None, "an overlay has no total");
}

#[test]
fn a_stacked_band_sits_on_the_one_below_and_the_readout_carries_the_total() {
    let g = draw(&stacked_spec(), &rows());
    let tops = first_tops(&g);
    assert_eq!(tops.len(), 2);
    // Free is 820 high, and paid's top is 820 + 240 high on the same scale.
    assert!((tops[1] / tops[0] - 1060.0 / 820.0).abs() < 0.01, "{tops:?}");
    let obs = observations(&g);
    assert_eq!(line(obs[0], "free"), Some("820"));
    assert_eq!(line(obs[0], "paid"), Some("240"));
    assert_eq!(line(obs[0], "Total"), Some("1,060"));
    assert_eq!(line(obs[3], "Total"), Some("1,210"));
}

#[test]
fn a_larger_value_is_drawn_higher_along_the_axis() {
    let g = draw(
        &single_spec(),
        &[json!({"day":"Mon","users":5}), json!({"day":"Tue","users":8})],
    );
    let obs = observations(&g);
    assert!(
        obs[1].at[1] < obs[0].at[1],
        "8 is above 5: {:?} {:?}",
        obs[0].at,
        obs[1].at
    );
    assert!(obs[0].x < obs[1].x);
}

#[test]
fn a_missing_value_is_disclosed_and_bad_text_fails_the_chart() {
    let mut rows = vec![
        json!({"day":"Mon","users":5}),
        json!({"day":"Tue","users":null}),
        json!({"day":"Wed","users":3}),
    ];
    let g = draw(&single_spec(), &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "missing-observations").is_some(), "{:?}", g.checks);
    rows[1] = json!({"day":"Tue","users":"1,000"});
    let g = draw(&single_spec(), &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    stays_visible(&single_spec(), &[]);
    contract(&single_spec(), &[json!({"day":"Mon","users":5})]);
    contract(
        &single_spec(),
        &[
            json!({"day":"Mon","users":-5}),
            json!({"day":"Tue","users":-7}),
            json!({"day":"Wed","users":0}),
        ],
    );
    // A stack keeps negative values below zero and positive above.
    contract(
        &stacked_spec(),
        &[
            json!({"day":"Mon","tier":"a","users":5}),
            json!({"day":"Mon","tier":"b","users":-3}),
            json!({"day":"Tue","tier":"a","users":2}),
            json!({"day":"Tue","tier":"b","users":-1}),
        ],
    );
    let g = draw(
        &single_spec(),
        &[json!({"day":"Mon","users":1e300}), json!({"day":"Tue","users":-5e299})],
    );
    assert!(nonfinite(&g).is_none());
}

#[test]
fn dense_series_keep_every_observation_reachable() {
    let rows: Vec<Value> = (0..300)
        .map(|i| json!({"day": format!("2025-{:02}-{:02}", i / 28 % 12 + 1, i % 28 + 1), "users": ((i * 37) % 101) as f64}))
        .collect();
    let g = draw_at(&single_spec(), &rows, 390.0);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(nonfinite(&g).is_none());
    assert_eq!(richview_chart::hit::order(&g).len(), observations(&g).len());
}

#[test]
fn every_curve_draws_the_same_observations() {
    for curve in ["monotone", "linear", "step", "step_before", "step_after"] {
        let mut s = spec();
        s["style"] = json!({"curve": curve});
        assert_eq!(observations(&draw(&s, &rows())).len(), 4, "{curve}");
        contract(&s, &rows());
    }
}

#[test]
fn writes_the_gallery_when_asked() {
    gallery("area", &spec(), &rows());
    gallery("area-stacked", &stacked_spec(), &rows());
}

#[test]
fn the_runtime_keeps_values_a_grid_step_apart() {
    let rows = vec![
        json!({"day":"Mon","users":100.01}),
        json!({"day":"Tue","users":1000}),
        json!({"day":"Wed","users":100.04}),
    ];
    runtime_matches_native(&single_spec(), &rows);
}
