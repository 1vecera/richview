//! Radar charts: spokes at equal angles from 12 o'clock, values at their true radius, polygons
//! that break at missing values, and the shared contract every type meets (FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Observation, Role};
use serde_json::{Value, json};
use std::f64::consts::TAU;

fn fixture() -> (Value, Vec<Value>) {
    let text = include_str!("fixtures/types/radar.json");
    let v: Value = serde_json::from_str(text).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

/// Angle (clockwise from 12 o'clock) and distance of an observation from the plot centre.
fn polar(g: &Geometry, o: &Observation) -> (f64, f64) {
    let c = g.panels[0].plot.center();
    let (dx, dy) = (o.at[0] - c[0], o.at[1] - c[1]);
    ((dx.atan2(-dy) + TAU) % TAU, dx.hypot(dy))
}

fn areas(g: &Geometry) -> Vec<&Item> {
    marks(g)
        .into_iter()
        .filter(|i| matches!(i, Item::Area { .. }))
        .collect()
}

#[test]
fn the_golden_fixture_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn spokes_sit_at_equal_angles_from_twelve_o_clock_clockwise() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(g.panels[0].targeting, richview_chart::geometry::Targeting::Radius);
    let obs = observations(&g);
    assert_eq!(obs.len(), 10, "one observation per group and spoke");
    for (i, name) in ["Speed", "Reliability", "Cost", "Documentation", "Community support"]
        .iter()
        .enumerate()
    {
        for o in obs.iter().filter(|o| o.readout.heading == *name) {
            let (angle, _) = polar(&g, o);
            let want = i as f64 * TAU / 5.0;
            let off = (angle - want).abs().min(TAU - (angle - want).abs());
            assert!(off < 1e-6, "{name} at {angle}, wanted {want}");
        }
    }
}

#[test]
fn radius_is_proportional_to_the_value_from_a_zero_centre() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    let r = |heading: &str, team: &str| {
        let o = obs
            .iter()
            .find(|o| o.readout.heading == heading && o.series == Some(if team == "Alpha" { 0 } else { 1 }))
            .unwrap();
        polar(&g, o).1
    };
    // Alpha scores 80 on Speed and 40 on Cost: twice the radius.
    assert!((r("Speed", "Alpha") / r("Cost", "Alpha") - 2.0).abs() < 1e-6);
    assert!((r("Speed", "Beta") / r("Speed", "Alpha") - 50.0 / 80.0).abs() < 1e-6);
    // Every point lies inside the outer ring.
    let ring = obs.iter().map(|o| polar(&g, o).1).fold(0.0, f64::max);
    assert!(ring <= g.panels[0].plot.w / 2.0);
}

#[test]
fn readouts_head_with_the_spoke_and_list_every_group() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let o = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "Reliability")
        .unwrap();
    assert_eq!(line(o, "Alpha"), Some("65"));
    assert_eq!(line(o, "Beta"), Some("90"));
    assert_eq!(o.readout.lines.len(), 2);
}

#[test]
fn each_group_has_a_closed_polygon_and_an_outline_and_rings_are_drawn() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(areas(&g).len(), 2);
    let outlines = marks(&g).into_iter().filter(|i| matches!(i, Item::Line { .. })).count();
    assert_eq!(outlines, 2);
    let rings = g.panels[0]
        .items
        .iter()
        .filter(|i| i.role() == Role::Grid && matches!(i, Item::Line { path, .. } if path.cmds.len() == 6))
        .count();
    assert!(rings >= 2, "rings at the value ticks: {rings}");
    let points = marks(&g)
        .into_iter()
        .filter(|i| matches!(i, Item::Point { .. }))
        .count();
    assert_eq!(points, 10, "five spokes or fewer than 13 draw a point each");
    // Axis names and tick labels are text on the panel.
    let texts: Vec<String> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| {
            if let Item::Text(t) = i {
                Some(t.text.clone())
            } else {
                None
            }
        })
        .collect();
    assert!(texts.iter().any(|t| t == "Speed"));
    assert!(texts.iter().any(|t| t == "0"));
}

#[test]
fn a_missing_value_breaks_the_polygon_and_is_disclosed() {
    let (spec, mut rows) = fixture();
    rows[2]["score"] = json!(null);
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "excluded-observations").is_some());
    assert_eq!(observations(&g).len(), 9);
    // Alpha is now a run of four values filled as a sector from the centre, not a closed ring.
    let outline = |series| {
        marks(&g)
            .into_iter()
            .find_map(|i| match i {
                Item::Line { path, .. } if i.series() == Some(series) => Some(path.cmds.len()),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(outline(0), 4, "Alpha's four values form one open run");
    assert_eq!(outline(1), 6, "Beta stays a closed ring");
    assert!(g.note.as_deref().unwrap().contains("missing"));
    contract(&spec, &rows);
    // A group with no row on a spoke is a gap too.
    let mut fewer = rows.clone();
    fewer.remove(7);
    let g = draw(&spec, &fewer);
    assert!(check(&g, "excluded-observations").is_some());
}

#[test]
fn fewer_than_three_spokes_fail_with_a_reason() {
    let (spec, _) = fixture();
    let rows = [
        json!({"skill": "A", "team": "T", "score": 1}),
        json!({"skill": "B", "team": "T", "score": 2}),
    ];
    let g = draw(&spec, &rows);
    assert!(g.failure.as_deref().unwrap().contains("at least 3"));
    assert!(check(&g, "source-error").is_some());
}

#[test]
fn text_that_is_not_a_number_fails_and_repeats_are_source_errors() {
    let (spec, mut rows) = fixture();
    rows[0]["score"] = json!("lots");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    let (_, mut rows) = fixture();
    rows.push(json!({"skill": "Speed", "team": "Alpha", "score": 3}));
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").unwrap().message.contains("repeats"));
    let (_, mut rows) = fixture();
    rows[0]["score"] = json!("1e-400");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").unwrap().message.contains("too small"));
}

#[test]
fn a_value_beyond_the_authored_scale_is_drawn_at_the_edge_and_reported() {
    let (mut spec, rows) = fixture();
    spec["axes"]["y"]["max"] = json!(60);
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    assert!(check(&g, "value-outside-scale").is_some());
    contract(&spec, &rows);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    // One group without a colour channel, and a negative value that pulls the centre below zero.
    let mut single = spec.clone();
    single["encoding"] = json!({"x": "skill", "y": "score"});
    let one: Vec<Value> = rows
        .iter()
        .take(5)
        .map(|r| json!({"skill": r["skill"], "score": r["score"]}))
        .collect();
    contract(&single, &one);
    let g = draw(&single, &one);
    assert_eq!(g.series.len(), 1);
    let mut negative = one.clone();
    negative[1]["score"] = json!(-30);
    contract(&single, &negative);
    assert!(draw(&single, &negative).note.as_deref().unwrap().contains("centre"));
    // Three spokes, all zero, and extreme values.
    let zeros: Vec<Value> = ["A", "B", "C"]
        .iter()
        .map(|s| json!({"skill": s, "score": 0}))
        .collect();
    contract(&single, &zeros);
    let huge: Vec<Value> = ["A", "B", "C"]
        .iter()
        .map(|s| json!({"skill": s, "score": 1e300}))
        .collect();
    assert!(nonfinite(&draw(&single, &huge)).is_none());
    // Many spokes: names that cannot all fit are dropped and reported, never clipped or overlapped.
    let dense: Vec<Value> = (0..40)
        .map(|i| json!({"skill": format!("Dimension number {i}"), "score": (i * 7) % 23}))
        .collect();
    for width in WIDTHS {
        let g = draw_at(&single, &dense, width);
        assert!(g.failure.is_none() && nonfinite(&g).is_none());
        assert!(check(&g, "clipping").is_none() && check(&g, "label-collision").is_none());
        assert_eq!(observations(&g).len(), 40);
        assert!(!g.dropped.is_empty() || width > 700.0);
    }
    contract(&single, &dense);
    // Long names wrap or drop and are never cut.
    let long: Vec<Value> = [
        "An extraordinarily long axis name that must wrap",
        "Short",
        "Another quite long axis name",
        "Mid",
    ]
    .iter()
    .map(|s| json!({"skill": s, "score": 5}))
    .collect();
    contract(&single, &long);
}

#[test]
fn facets_draw_one_radar_each() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["facet"] = json!("quarter");
    let rows: Vec<Value> = rows
        .iter()
        .flat_map(|r| {
            ["Q1", "Q2"].iter().map(move |q| {
                let mut r = r.clone();
                r["quarter"] = json!(q);
                r
            })
        })
        .collect();
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(g.panels[0].observations.len(), 10);
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("radar", &spec, &rows);
    let mut gappy = rows.clone();
    gappy[2]["score"] = json!(null);
    gallery("radar-gap", &spec, &gappy);
    let mut dense = spec.clone();
    dense["encoding"] = json!({"x": "skill", "y": "score"});
    let many: Vec<Value> = (0..24)
        .map(|i| json!({"skill": format!("Dimension {i}"), "score": (i * 7) % 23}))
        .collect();
    gallery("radar-dense", &dense, &many);
}

#[test]
fn the_runtime_keeps_the_axis_of_values_a_grid_step_apart() {
    let spec = json!({"type":"radar","title":"T","encoding":{"x":"skill","y":"score"}});
    let rows = vec![
        json!({"skill":"A","score":100.01}),
        json!({"skill":"B","score":100.04}),
        json!({"skill":"C","score":1000}),
        json!({"skill":"D","score":0.5}),
    ];
    runtime_matches_native(&spec, &rows);
}
