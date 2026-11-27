//! 3-D scatterplots: depth-sorted dots with the nearer ones larger, size and colour channels,
//! the views the author can ask for, and the failures that name their cause (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Geometry, Item, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/scatter3d.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"scatter3d","title":"T","encoding":{"x":"a","y":"b","z":"c"}})
}

fn point(a: f64, b: f64, c: f64) -> Value {
    json!({"a": a, "b": b, "c": c})
}

/// Radii of the dots in the order they are drawn.
fn radii(g: &Geometry) -> Vec<f64> {
    marks(g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Point { radius, .. } => Some(*radius),
            _ => None,
        })
        .collect()
}

fn cloud(n: usize) -> Vec<Value> {
    (0..n)
        .map(|k| point((k * 7 % 31) as f64, (k * 11 % 29) as f64, (k * 13 % 23) as f64))
        .collect()
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
    gallery("scatter3d", &spec, &rows);
}

#[test]
fn one_dot_for_each_row_with_its_three_coordinates_and_group_in_the_readout() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(radii(&g).len(), 36);
    let o = observations(&g);
    assert_eq!(o.len(), 36);
    assert!(g.legend.is_some());
    assert_eq!(g.series.len(), 3);
    let first = o.iter().find(|o| o.row == Some(0)).unwrap();
    assert_eq!(first.readout.heading, "Adelie");
    assert_eq!(line(first, "Species"), None);
    assert!(line(first, "Bill length (mm)").is_some());
    assert!(line(first, "Bill depth (mm)").is_some());
    assert!(line(first, "Flipper (mm)").is_some());
    assert!(
        first.readout.lines.iter().all(|l| !l.derived),
        "every value is as given"
    );
}

#[test]
fn nearer_dots_are_drawn_later_and_larger() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let r = radii(&g);
    assert!(r.windows(2).all(|w| w[0] <= w[1] + 1e-9), "{r:?}");
    assert!(r[35] > r[0] + 0.3, "depth shows in the size: {} to {}", r[0], r[35]);
    let series: std::collections::BTreeSet<usize> = marks(&g)
        .into_iter()
        .filter_map(|i| match i {
            Item::Point { series, .. } => *series,
            _ => None,
        })
        .collect();
    assert_eq!(series.len(), 3);
}

#[test]
fn a_size_channel_sets_the_radius_by_area_and_gets_a_key() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["size"] = json!("body_mass");
    let g = draw(&spec, &rows);
    let r = radii(&g);
    let top = r.iter().cloned().fold(0.0, f64::max);
    assert!(
        (top - 16.0).abs() < 1e-9,
        "the largest value gets the bubble maximum, got {top}"
    );
    // Radius squared follows the value: compare two dots through their readouts.
    let o = observations(&g);
    let mass = |k: usize| -> f64 { line(o[k], "Body mass").unwrap().replace(',', "").parse().unwrap() };
    let (a, b) = (0, 1);
    let ra = o[a].radius;
    let rb = o[b].radius;
    if ra > 4.1 && rb > 4.1 && ra < 16.0 && rb < 16.0 {
        assert!(((ra / rb).powi(2) - mass(a) / mass(b)).abs() < 0.05);
    }
    assert!(
        g.figure_items
            .iter()
            .any(|i| matches!(i, Item::Point { role: Role::Key, .. }))
    );
    assert!(g.height > g.panels[0].frame.h, "the key makes the figure taller");
    contract(&spec, &rows);
}

#[test]
fn a_few_points_stand_on_a_line_to_the_floor_and_many_do_not() {
    let leaders = |g: &Geometry| g.items().filter(|i| i.role() == Role::Leader).count();
    assert_eq!(leaders(&draw(&plain(), &cloud(30))), 30);
    assert_eq!(leaders(&draw(&plain(), &cloud(200))), 0);
}

#[test]
fn the_view_follows_the_azimuth_and_the_elevation() {
    let (spec, rows) = fixture();
    let centres = |spec: &Value| -> Vec<[f64; 2]> { observations(&draw(spec, &rows)).iter().map(|o| o.at).collect() };
    let mut same = spec.clone();
    same["style"] = json!({"azimuth": 35, "elevation": 28});
    assert_eq!(centres(&spec), centres(&same));
    let mut turned = spec.clone();
    turned["style"] = json!({"azimuth": 200, "elevation": 60});
    assert_ne!(centres(&spec), centres(&turned));
}

#[test]
fn every_view_draws_clean() {
    let (spec, rows) = fixture();
    for azimuth in [0, 60, 135, 200, 290] {
        for elevation in [5, 28, 70, 85] {
            let mut s = spec.clone();
            s["style"] = json!({"azimuth": azimuth, "elevation": elevation});
            contract(&s, &rows);
        }
    }
}

#[test]
fn more_points_than_the_limit_are_not_drawn_and_name_the_limit() {
    let g = draw(&plain(), &cloud(5001));
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["count"], 5001);
    assert_eq!(c.details["limit"], 5000);
    assert_eq!(c.details["what"], "points");
    assert!(observations(&g).is_empty());
}

#[test]
fn points_at_the_limit_all_draw() {
    let g = draw(&plain(), &cloud(5000));
    assert!(g.failure.is_none());
    assert!(check(&g, "limit-exceeded").is_none());
    assert_eq!(observations(&g).len(), 5000);
}

#[test]
fn rows_without_a_coordinate_or_with_a_negative_size_are_disclosed() {
    let (mut spec, mut rows) = fixture();
    spec["encoding"]["size"] = json!("body_mass");
    rows[0]["flipper"] = Value::Null;
    rows[1]["body_mass"] = json!(-5);
    rows[2]["species"] = json!("");
    let g = draw(&spec, &rows);
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 3);
    assert_eq!(observations(&g).len(), 33);
}

#[test]
fn bad_input_says_why() {
    let g = draw(&plain(), &[point(1.0, 1.0, 1.0), json!({"a": 1, "b": "x", "c": 2})]);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    let spec = json!({"type":"scatter3d","title":"T","encoding":{"x":"a","y":"b"}});
    let g = draw(&spec, &[point(1.0, 1.0, 1.0)]);
    assert!(g.failure.as_deref().unwrap().contains("encoding.z"));
    let g = draw(&plain(), &[json!({"a": 1})]);
    assert!(g.failure.as_deref().unwrap().contains("No row has an x, a y and a z"));
    stays_visible(&plain(), &[]);
}

#[test]
fn single_flat_negative_and_coincident_points_draw() {
    contract(&plain(), &[point(1.0, 2.0, 3.0)]);
    contract(
        &plain(),
        &[point(1.0, 1.0, 1.0), point(1.0, 1.0, 1.0), point(1.0, 1.0, 1.0)],
    );
    contract(
        &plain(),
        &[point(-5.0, -2.0, -9.0), point(5.0, 2.0, 9.0), point(0.0, 0.0, 0.0)],
    );
    contract(&plain(), &cloud(300));
}

#[test]
fn each_dot_announces_its_own_row_whatever_the_painting_order() {
    let (spec, rows) = fixture();
    for g in [draw(&spec, &rows), draw(&spec, &quantized(&spec, &rows))] {
        let o = observations(&g);
        let rows_seen: Vec<_> = o.iter().filter_map(|o| o.row).collect();
        assert!(
            rows_seen.windows(2).all(|w| w[0] < w[1]),
            "observations run in source order"
        );
        let dots: Vec<_> = marks(&g)
            .into_iter()
            .filter_map(|i| match i {
                Item::Point {
                    center, obs: Some(k), ..
                } => Some((*center, *k)),
                _ => None,
            })
            .collect();
        assert_eq!(dots.len(), 36);
        for (center, k) in dots {
            let own = o[k];
            assert_eq!(own.at, center, "dot {k} is read at its own place");
            assert_eq!(own.readout.heading, rows[own.row.unwrap()]["species"].as_str().unwrap());
        }
    }
}

#[test]
fn the_runtime_paints_the_order_and_axes_the_exact_rows_have() {
    // Dots at nearly one depth, and extremes that put a tick edge a grid step away.
    let mut rows = cloud(40);
    rows.push(point(100.01, 3.0, 4.0));
    rows.push(point(100.04, 3.0, 4.0));
    rows.push(point(1000.0, 29.0, 23.0));
    runtime_matches_native(&plain(), &rows);
}

#[test]
fn a_label_shared_with_an_axis_is_named_once_at_readout_precision() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["label"] = json!("bill_length");
    let g = draw(&spec, &rows);
    let o = observations(&g).into_iter().find(|o| o.row == Some(0)).unwrap();
    assert_eq!(o.readout.heading, "Bill length (mm) 38.3");
    assert_eq!(line(o, "Bill length (mm)"), None);
    assert_eq!(line(o, "Bill depth (mm)"), Some("18.8"));
}

#[test]
fn a_label_shared_with_the_z_axis_is_named_once() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["label"] = json!("flipper");
    let g = draw(&spec, &rows);
    let o = observations(&g).into_iter().find(|o| o.row == Some(0)).unwrap();
    assert_eq!(o.readout.heading, "Flipper (mm) 189");
    assert_eq!(line(o, "Flipper (mm)"), None);
    assert_eq!(line(o, "Bill length (mm)"), Some("38.3"));
}
