//! Chord diagrams: arcs sized by the flow into and out of a node, ribbons as wide as their flow,
//! names outside the ring, and the failures that name their cause (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::Level;
use richview_chart::block::ChartBlock;
use richview_chart::geometry::{Geometry, Observation};
use richview_chart::hit::{self, Target};
use richview_chart::layout::{ChartTheme, Options, layout};
use serde_json::{Value, json};
use std::f64::consts::TAU;

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/chord.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"chord","title":"T","encoding":{"source":"a","target":"b","y":"v"}})
}

fn row(a: &str, b: &str, v: Value) -> Value {
    json!({"a": a, "b": b, "v": v})
}

fn node<'a>(g: &'a Geometry, name: &str) -> &'a Observation {
    observations(g)
        .into_iter()
        .find(|o| o.wedge.is_some() && o.readout.heading == name)
        .unwrap_or_else(|| panic!("no node {name}"))
}

fn flow<'a>(g: &'a Geometry, from: &str, to: &str) -> &'a Observation {
    let heading = format!("{from} → {to}");
    observations(g)
        .into_iter()
        .find(|o| o.wedge.is_none() && o.readout.heading == heading)
        .unwrap_or_else(|| panic!("no flow {heading}"))
}

fn angle(o: &Observation) -> f64 {
    let w = o.wedge.unwrap();
    w.end - w.start
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn an_arc_for_each_node_in_first_seen_order_around_the_circle() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(observations(&g).len(), 5 + 13);
    let names = ["Europe", "Asia", "Americas", "Africa", "Oceania"];
    let wedges: Vec<_> = names.iter().map(|n| node(&g, n).wedge.unwrap()).collect();
    for w in wedges.windows(2) {
        assert!(w[0].end < w[1].start, "arcs run clockwise with a gap between them");
    }
    assert!(wedges[0].start >= 0.0 && wedges[4].end < TAU);
    assert!(
        wedges
            .iter()
            .all(|w| w.center == wedges[0].center && w.inner > 0.0 && w.radius > w.inner)
    );
}

#[test]
fn an_arc_is_as_long_as_the_flow_into_and_out_of_its_node() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    // Europe sends 115 and receives 95, Asia 120 and 100, Africa 27 and 30.
    assert!((angle(node(&g, "Europe")) / angle(node(&g, "Africa")) - 210.0 / 57.0).abs() < 1e-6);
    assert!((angle(node(&g, "Asia")) / angle(node(&g, "Oceania")) - 220.0 / 51.0).abs() < 1e-6);
}

#[test]
fn readouts_give_the_value_the_share_and_the_sides_of_a_node() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let europe = node(&g, "Europe");
    assert_eq!(line(europe, "Out"), Some("115"));
    assert_eq!(line(europe, "In"), Some("95"));
    assert_eq!(line(europe, "Share of the circle"), Some("28.2%"));
    assert!(europe.readout.lines.iter().all(|l| l.derived));
    let f = flow(&g, "Europe", "Asia");
    assert_eq!(line(f, "bn USD"), Some("40"));
    assert_eq!(line(f, "Share of the whole"), Some("10.7%"));
    assert!(!f.readout.lines[0].derived && f.readout.lines[1].derived);
    assert!(f.row.is_some() && europe.row.is_none());
}

#[test]
fn a_ribbon_takes_the_colour_of_its_source_and_both_directions_are_drawn() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(g.series.len(), 5);
    assert_eq!(flow(&g, "Europe", "Asia").series, node(&g, "Europe").series);
    assert_eq!(flow(&g, "Asia", "Europe").series, node(&g, "Asia").series);
    assert_ne!(flow(&g, "Europe", "Asia").series, flow(&g, "Asia", "Europe").series);
}

#[test]
fn arcs_and_ribbons_are_found_by_pointer() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    for (i, o) in g.panels[0]
        .observations
        .iter()
        .enumerate()
        .filter(|(_, o)| o.wedge.is_some())
    {
        assert_eq!(
            hit::at(&g, o.at),
            Some(Target {
                panel: 0,
                observation: i
            }),
            "{}",
            o.readout.heading
        );
    }
    for o in observations(&g).into_iter().filter(|o| o.wedge.is_none()) {
        assert!(hit::at(&g, o.at).is_some(), "{}", o.readout.heading);
    }
}

#[test]
fn a_link_to_the_same_node_is_a_loop_on_its_arc() {
    let spec = plain();
    let rows = [row("A", "A", json!(3)), row("A", "B", json!(2))];
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(observations(&g).len(), 2 + 2);
    assert!(check(&g, "excluded-observations").is_none());
    // A's arc takes both ends of the loop: 3 + 3 + 2 of 10.
    assert!((angle(node(&g, "A")) / angle(node(&g, "B")) - 8.0 / 2.0).abs() < 1e-6);
}

#[test]
fn two_directions_between_the_same_nodes_are_two_ribbons() {
    let spec = plain();
    let rows = [row("A", "B", json!(2)), row("B", "A", json!(5))];
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(observations(&g).len(), 2 + 2);
}

#[test]
fn hiding_a_node_removes_its_arc_and_ribbons() {
    let (spec, rows) = fixture();
    let g = layout(
        &ChartBlock::from_value(&spec).unwrap(),
        &rows,
        &ChartTheme::default(),
        &Options {
            width: 640.0,
            hidden: vec!["Africa".into()],
            ..Options::default()
        },
    );
    assert!(g.failure.is_none());
    assert_eq!(observations(&g).len(), 4 + 9);
    assert!(observations(&g).iter().all(|o| !o.readout.heading.contains("Africa")));
}

#[test]
fn a_name_that_does_not_fit_is_reported_and_the_key_names_it() {
    let (spec, mut rows) = fixture();
    rows.push(json!({"from": "A region whose name cannot fit beside its arc at all", "to": "Asia", "bn": 5}));
    let g = draw_at(&spec, &rows, 320.0);
    assert!(check(&g, "clipping").is_none());
    assert!(check(&g, "label-collision").is_none());
    assert!(g.dropped.iter().any(|d| d.text.starts_with("A region")));
    assert!(g.legend.is_some(), "the key stands in for the names that were left out");
}

#[test]
fn rows_without_a_usable_link_are_left_out_and_bad_values_fail() {
    let spec = plain();
    let g = draw(
        &spec,
        &[
            row("A", "B", json!(2)),
            row("A", "C", json!(-1)),
            row("A", "D", json!("NA")),
            json!({"b": "E", "v": 3}),
        ],
    );
    assert!(g.failure.is_none());
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 3);
    assert_eq!(observations(&g).len(), 2 + 1);
    let g = draw(&spec, &[row("A", "B", json!("some"))]);
    assert!(g.failure.is_some() && check(&g, "non-numeric").is_some());
    let g = draw(&spec, &[row("A", "B", json!(1)), row("A", "B", json!(1))]);
    assert!(g.failure.is_some() && check(&g, "source-error").is_some());
    let g = draw(&spec, &[row("A", "B", json!(1e308)), row("B", "C", json!(1e308))]);
    assert!(g.failure.is_some() && check(&g, "value-overflow").is_some());
}

#[test]
fn facets_draw_one_diagram_each() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["facet"] = json!("year");
    let rows: Vec<Value> = ["2025", "2026"]
        .iter()
        .flat_map(|y| {
            rows.iter().cloned().map(move |mut r| {
                r["year"] = json!(y);
                r
            })
        })
        .collect();
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(g.panels[0].observations.len(), 18);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let spec = plain();
    stays_visible(&spec, &[]);
    stays_visible(&spec, &[row("A", "B", json!(0))]);
    contract(&spec, &[row("A", "B", json!(1))]);
    let g = draw(&spec, &[row("A", "B", json!(1e300)), row("B", "C", json!(1e-300))]);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let mesh: Vec<Value> = (0..30)
        .flat_map(|i| {
            (0..30)
                .filter(move |j| *j != i)
                .map(move |j| row(&format!("Node {i}"), &format!("Node {j}"), json!(1 + (i + j) % 5)))
        })
        .collect();
    let g = draw_at(&spec, &mesh, 390.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 30 + 870);
    assert!(!g.dropped.is_empty(), "30 names around a small ring do not all fit");
    assert!(
        check(&g, "clipping").is_none() && check(&g, "label-collision").is_none(),
        "{:?} {:?}",
        check(&g, "clipping"),
        check(&g, "label-collision")
    );
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("chord", &spec, &rows);
}

#[test]
fn the_runtime_keeps_the_links_the_exact_rows_keep() {
    let rows: Vec<Value> = vec![
        json!({"a": "A", "b": "B", "v": 1000}),
        json!({"a": "B", "b": "C", "v": 0.05}),
        json!({"a": "C", "b": "A", "v": 100.04}),
        json!({"a": "C", "b": "B", "v": 0}),
    ];
    runtime_matches_native(&plain(), &rows);
}

#[test]
fn a_chord_over_a_limit_is_not_drawn_and_names_the_limit_and_at_it_draws() {
    let rows: Vec<Value> = (0..300)
        .map(|i| row(&format!("n{i}"), &format!("n{}", i + 1), json!(1)))
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "nodes");
    assert_eq!(c.details["count"], 301);
    assert_eq!(c.details["limit"], 300);
    assert!(observations(&g).is_empty());
    let rows: Vec<Value> = (0..46)
        .flat_map(|i| (0..46).map(move |j| row(&format!("a{i}"), &format!("b{j}"), json!(1))))
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "links");
    assert_eq!(c.details["count"], 2116);
    assert_eq!(c.details["limit"], 2000);
    assert!(observations(&g).is_empty());
    let rows: Vec<Value> = (0..299)
        .map(|i| row(&format!("n{i}"), &format!("n{}", i + 1), json!(1)))
        .collect();
    let g = draw(&plain(), &rows);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(check(&g, "limit-exceeded").is_none());
    assert!(nonfinite(&g).is_none());
}
