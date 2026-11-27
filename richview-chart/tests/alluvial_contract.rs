//! Alluvial diagrams: one column of bars for each dimension, ribbons between neighbouring
//! categories coloured by the first dimension, rows added up and marked as derived, and the
//! failures that name their cause (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::Level;
use richview_chart::geometry::{Geometry, Item, Observation, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/alluvial.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"alluvial","title":"T","encoding":{"dimensions":["a","b"]}})
}

fn node<'a>(g: &'a Geometry, heading: &str) -> &'a Observation {
    observations(g)
        .into_iter()
        .find(|o| o.row.is_none() && o.readout.heading == heading)
        .unwrap_or_else(|| panic!("no node {heading}"))
}

/// The ribbon between two categories; `plan` picks the one that belongs to that first category.
fn ribbon<'a>(g: &'a Geometry, heading: &str, plan: Option<&str>) -> &'a Observation {
    observations(g)
        .into_iter()
        .find(|o| o.readout.heading == heading && (plan.is_none() || line(o, "Plan") == plan))
        .unwrap_or_else(|| panic!("no ribbon {heading}"))
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn a_column_for_each_dimension_with_categories_in_the_order_the_rows_name_them() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let at = |h: &str| node(&g, h).at;
    assert!(at("plan: Free")[0] < at("channel: Search")[0] && at("channel: Search")[0] < at("outcome: Stayed")[0]);
    assert_eq!(at("plan: Free")[0], at("plan: Pro")[0]);
    assert!(at("plan: Free")[1] < at("plan: Basic")[1] && at("plan: Basic")[1] < at("plan: Pro")[1]);
    assert!(at("channel: Search")[1] < at("channel: Ads")[1] && at("channel: Ads")[1] < at("channel: Referral")[1]);
    assert!(at("outcome: Stayed")[1] < at("outcome: Left")[1]);
    // Eight bars, nine ribbons into the channels and eighteen out of them.
    assert_eq!(observations(&g).len(), 8 + 9 + 18);
}

#[test]
fn the_dimensions_are_titled_over_their_columns() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let titles: Vec<&str> = g.panels[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text(t) if t.role == Role::AxisLabel => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(titles, ["plan", "channel", "outcome"]);
}

#[test]
fn readouts_add_up_the_rows_and_say_so() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let free = node(&g, "plan: Free");
    assert_eq!(line(free, "Users"), Some("600"));
    assert_eq!(line(free, "Share of the whole"), Some("57.4%"));
    assert!(free.readout.lines.iter().all(|l| l.derived));
    let search = node(&g, "channel: Search");
    assert_eq!(line(search, "In"), Some("490"));
    assert_eq!(line(search, "Out"), Some("490"));
    let first = ribbon(&g, "Free → Search", None);
    assert_eq!(line(first, "Users"), Some("300"));
    assert_eq!(line(first, "Share of source"), Some("50%"));
    assert_eq!(line(first, "Share of target"), Some("61.2%"));
    assert!(first.readout.lines[0].derived, "rows were added up");
    let later = ribbon(&g, "Search → Stayed", Some("Free"));
    assert_eq!(line(later, "Users"), Some("120"));
    assert_eq!(line(later, "Plan"), Some("Free"));
    assert_eq!(line(ribbon(&g, "Search → Stayed", Some("Pro")), "Users"), Some("50"));
}

#[test]
fn a_value_field_without_an_authored_axis_title_has_a_readable_name() {
    let spec = json!({"type":"alluvial","title":"T","encoding":{"dimensions":["a","b","c"],"y":"v"}});
    let g = draw(&spec, &[json!({"a":"x","b":"y","c":"z","v":7})]);
    let link = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "x → y")
        .unwrap();
    assert_eq!(line(link, "V"), Some("7"));
    let later = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "y → z")
        .unwrap();
    assert_eq!(line(later, "A"), Some("x"));
}

#[test]
fn ribbons_take_the_colour_of_the_first_dimension() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(g.series.len(), 3);
    let free = ribbon(&g, "Search → Stayed", Some("Free")).series;
    assert_eq!(free, ribbon(&g, "Free → Ads", None).series);
    assert_ne!(free, ribbon(&g, "Search → Stayed", Some("Pro")).series);
    assert_eq!(
        node(&g, "channel: Ads").series,
        None,
        "bars after the first column are neutral"
    );
}

#[test]
fn without_a_value_every_row_counts_one() {
    let spec = plain();
    let rows = [
        json!({"a":"x","b":"p"}),
        json!({"a":"x","b":"p"}),
        json!({"a":"y","b":"p"}),
    ];
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    let o = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "x → p")
        .unwrap();
    assert_eq!(line(o, "Rows"), Some("2"));
    contract(&spec, &rows);
}

#[test]
fn rows_without_a_category_or_a_value_are_left_out_and_counted() {
    let (spec, mut rows) = fixture();
    rows.push(json!({"plan": "Free", "channel": null, "outcome": "Left", "users": 5}));
    rows.push(json!({"plan": "Free", "channel": "Ads", "outcome": "Left", "users": -5}));
    rows.push(json!({"plan": "Free", "channel": "Ads", "outcome": "Left", "users": null}));
    rows.push(json!({"plan": "Free", "channel": "Ads", "outcome": "Left", "users": 0}));
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none());
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 3);
    assert!(g.note.as_deref().unwrap_or("").contains("zero"));
    assert_eq!(line(node(&g, "plan: Free"), "Users"), Some("600"));
}

#[test]
fn bad_input_fails_the_chart_with_the_cause_named() {
    let g = draw(
        &json!({"type":"alluvial","title":"T","encoding":{"dimensions":["a"]}}),
        &[json!({"a":"x"})],
    );
    assert!(g.failure.is_some());
    assert!(check(&g, "source-error").unwrap().message.contains("dimensions"));
    let g = draw(
        &json!({"type":"alluvial","title":"T","encoding":{"y":"v"}}),
        &[json!({"a":"x","v":1})],
    );
    assert!(g.failure.is_some());
    let spec = json!({"type":"alluvial","title":"T","encoding":{"dimensions":["a","b"],"y":"v"}});
    let g = draw(&spec, &[json!({"a":"x","b":"p","v":"many"})]);
    assert!(g.failure.is_some());
    assert!(check(&g, "non-numeric").is_some());
    let g = draw(
        &spec,
        &[json!({"a":"x","b":"p","v":1e308}), json!({"a":"y","b":"q","v":1e308})],
    );
    assert!(g.failure.is_some());
    assert!(check(&g, "value-overflow").is_some());
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
    assert_eq!(g.panels[1].observations.len(), 35);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let spec = plain();
    stays_visible(&spec, &[]);
    stays_visible(&spec, &[json!({"a": null, "b": "p"})]);
    contract(&spec, &[json!({"a":"x","b":"p"})]);
    let huge = json!({"type":"alluvial","title":"T","encoding":{"dimensions":["a","b"],"y":"v"}});
    let g = draw(
        &huge,
        &[json!({"a":"x","b":"p","v":1e300}), json!({"a":"x","b":"q","v":1e-300})],
    );
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let dense: Vec<Value> = (0..400)
        .map(|i| json!({"a": format!("A{}", i % 17), "b": format!("B{}", i % 13), "c": format!("C{}", i % 11)}))
        .collect();
    let three = json!({"type":"alluvial","title":"T","encoding":{"dimensions":["a","b","c"]}});
    let g = draw_at(&three, &dense, 390.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    assert!(check(&g, "clipping").is_none() && check(&g, "label-collision").is_none());
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("alluvial", &spec, &rows);
}

#[test]
fn the_runtime_keeps_the_rows_the_exact_rows_keep() {
    let spec = json!({"type":"alluvial","title":"T","encoding":{"dimensions":["a","b"],"y":"v"}});
    let rows: Vec<Value> = vec![
        json!({"a": "North", "b": "Yes", "v": 1000}),
        json!({"a": "North", "b": "No", "v": 0.05}),
        json!({"a": "South", "b": "Yes", "v": 100.04}),
        json!({"a": "South", "b": "No", "v": 100.01}),
        json!({"a": "East", "b": "No", "v": 0}),
    ];
    runtime_matches_native(&spec, &rows);
}

#[test]
fn an_alluvial_over_a_limit_is_not_drawn_and_names_the_limit_and_at_it_draws() {
    let spec = json!({"type":"alluvial","title":"T","encoding":{"dimensions":["a","b"]}});
    let rows: Vec<Value> = (0..151)
        .map(|i| json!({"a": format!("A{i}"), "b": format!("B{i}")}))
        .collect();
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "nodes");
    assert_eq!(c.details["count"], 302);
    assert_eq!(c.details["limit"], 300);
    assert!(observations(&g).is_empty());
    let rows: Vec<Value> = (0..46)
        .flat_map(|i| (0..46).map(move |j| json!({"a": format!("A{i}"), "b": format!("B{j}")})))
        .collect();
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some());
    let c = check(&g, "limit-exceeded").expect("limit-exceeded");
    assert_eq!(c.level, Level::Error);
    assert_eq!(c.details["what"], "links");
    assert_eq!(c.details["count"], 2116);
    assert_eq!(c.details["limit"], 2000);
    assert!(observations(&g).is_empty());
    let rows: Vec<Value> = (0..150)
        .map(|i| json!({"a": format!("A{i}"), "b": format!("B{i}")}))
        .collect();
    let g = draw(&spec, &rows);
    assert!(g.failure.is_none(), "{:?}", g.failure);
    assert!(check(&g, "limit-exceeded").is_none());
    assert!(nonfinite(&g).is_none());
}
