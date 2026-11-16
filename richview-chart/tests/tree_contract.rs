//! Trees: layered node-link layout, leaves evenly spaced, parents centred over their children,
//! an optional value that is only read out, and the shared contract every type meets
//! (FEA-CH-01, FEA-CH-75).

mod common;

use common::*;
use richview_chart::geometry::{Cmd, Item};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let text = include_str!("fixtures/types/tree.json");
    let v: Value = serde_json::from_str(text).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn at(g: &richview_chart::geometry::Geometry, heading: &str) -> [f64; 2] {
    observations(g)
        .iter()
        .find(|o| o.readout.heading == heading)
        .unwrap()
        .at
}

#[test]
fn the_golden_hierarchy_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn depth_sets_the_column_and_leaves_are_evenly_spaced() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(observations(&g).len(), 12);
    let (eng, platform, core) = (
        at(&g, "Engineering"),
        at(&g, "Engineering › Platform"),
        at(&g, "Engineering › Platform › Core"),
    );
    assert!(
        eng[0] < platform[0] && platform[0] < core[0],
        "depth runs left to right"
    );
    assert_eq!(eng[0], at(&g, "Sales")[0], "one depth, one column");
    // Leaves in source order from the top, one row apart.
    let leaves = [
        "Engineering › Platform › Core",
        "Engineering › Platform › Tooling",
        "Engineering › Product",
        "Engineering › Data",
        "Sales › EMEA",
        "Sales › Americas",
        "Operations › People",
        "Operations › Finance",
    ];
    let ys: Vec<f64> = leaves.iter().map(|h| at(&g, h)[1]).collect();
    let step = ys[1] - ys[0];
    assert!(step > 0.0);
    for w in ys.windows(2) {
        assert!((w[1] - w[0] - step).abs() < 1e-9);
    }
}

#[test]
fn a_parent_is_centred_over_its_children() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let mid = |a: &str, b: &str| (at(&g, a)[1] + at(&g, b)[1]) / 2.0;
    assert!((at(&g, "Sales")[1] - mid("Sales › EMEA", "Sales › Americas")).abs() < 1e-9);
    assert!(
        (at(&g, "Engineering › Platform")[1]
            - mid("Engineering › Platform › Core", "Engineering › Platform › Tooling"))
        .abs()
            < 1e-9
    );
    assert!((at(&g, "Engineering")[1] - mid("Engineering › Platform", "Engineering › Data")).abs() < 1e-9);
}

#[test]
fn edges_are_cubic_curves_from_parent_to_child() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let curves: Vec<_> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Line { path, .. } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(curves.len(), 9, "one edge per non-top part");
    let from = at(&g, "Sales");
    let to = at(&g, "Sales › EMEA");
    let found = curves
        .iter()
        .any(|p| matches!(p.cmds.as_slice(), [Cmd::M { p: a }, Cmd::C { p: b, .. }] if *a == from && *b == to));
    assert!(found, "an elbow curve joins Sales to EMEA");
}

#[test]
fn readouts_give_the_path_the_given_value_and_the_children() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    let get = |h: &str| obs.iter().find(|o| o.readout.heading == h).unwrap();
    let core = get("Engineering › Platform › Core");
    assert_eq!(line(core, "headcount"), Some("8"));
    assert_eq!(line(core, "Children"), Some("0"));
    let sales = get("Sales");
    assert_eq!(
        line(sales, "headcount"),
        None,
        "a parent shows no value it was not given"
    );
    assert_eq!(line(sales, "Children"), Some("2"));
    assert!(sales.readout.lines.iter().any(|l| l.name == "Children" && l.derived));
}

#[test]
fn the_value_is_optional() {
    let (mut spec, rows) = fixture();
    spec["encoding"].as_object_mut().unwrap().remove("y");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(check(&g, "hierarchy-total").is_none());
    assert_eq!(observations(&g).len(), 12);
    assert!(observations(&g).iter().all(|o| o.readout.lines.len() == 1));
    contract(&spec, &rows);
    // A negative value is only a value here.
    let (spec, _) = fixture();
    let g = draw(&spec, &[json!({"unit":"A","headcount":-2})]);
    assert!(g.failure.is_none());
    assert_eq!(line(observations(&g)[0], "headcount"), Some("−2"));
}

#[test]
fn siblings_keep_source_order() {
    let (spec, mut rows) = fixture();
    rows.swap(7, 8);
    let g = draw(&spec, &rows);
    assert!(at(&g, "Sales › Americas")[1] < at(&g, "Sales › EMEA")[1]);
}

#[test]
fn colour_follows_the_top_level_part() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    let series = |h: &str| obs.iter().find(|o| o.readout.heading == h).unwrap().series;
    assert_eq!(series("Engineering"), series("Engineering › Platform › Core"));
    assert_ne!(series("Engineering"), series("Sales"));
}

#[test]
fn a_name_that_does_not_fit_is_reported_never_clipped() {
    let (spec, mut rows) = fixture();
    rows.push(
        json!({"unit":"A very long name that cannot fit beside its node at all","reports_to":"Sales","headcount":1}),
    );
    let g = draw_at(&spec, &rows, 320.0);
    assert!(check(&g, "clipping").is_none());
    assert!(check(&g, "label-collision").is_none());
    assert!(g.dropped.iter().any(|d| d.text.starts_with("A very long")));
}

#[test]
fn broken_hierarchies_fail_with_the_row_named() {
    let (spec, mut rows) = fixture();
    rows[1]["reports_to"] = json!("Nowhere");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").unwrap().message.contains("Nowhere"));
    let g = draw(
        &spec,
        &[
            json!({"unit":"A","reports_to":"B"}),
            json!({"unit":"B","reports_to":"A"}),
        ],
    );
    assert!(g.failure.is_some());
    let g = draw(&spec, &[json!({"unit":"A"}), json!({"unit":"A"})]);
    assert!(g.failure.is_some());
}

#[test]
fn facets_draw_one_tree_each() {
    let (mut spec, rows) = fixture();
    spec["encoding"]["facet"] = json!("year");
    let rows: Vec<Value> = [2024, 2025]
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
    assert_eq!(g.panels[0].observations.len(), 12);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, _) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &[json!({"unit":"Only"})]);
    contract(
        &spec,
        &[json!({"unit":"A","headcount":0}), json!({"unit":"B","headcount":3})],
    );
    let chain: Vec<Value> = (0..40)
        .map(|i| json!({"unit":format!("Level {i}"),"reports_to": if i == 0 { Value::Null } else { json!(format!("Level {}", i - 1)) }}))
        .collect();
    let g = draw_at(&spec, &chain, 390.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 40);
    let dense: Vec<Value> = (0..300)
        .map(|i| json!({"unit":format!("Part {i}"),"headcount":(i % 17 + 1)}))
        .collect();
    let g = draw_at(&spec, &dense, 390.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert_eq!(observations(&g).len(), 300);
    assert!(!g.dropped.is_empty(), "300 names do not fit and are reported");
    let g = draw(
        &spec,
        &[
            json!({"unit":"A","headcount":1e300}),
            json!({"unit":"B","headcount":1e300}),
        ],
    );
    assert!(nonfinite(&g).is_none());
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("tree", &spec, &rows);
}
