//! Ribbon charts: bars with their groups ranked by value, ribbons joining a group to itself in the
//! next category, and the failures that name their cause (FEA-CH-75).

use super::common;

use common::*;
use richview_chart::geometry::{Geometry, Item, Observation, Rect, Role};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/ribbon.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

fn plain() -> Value {
    json!({"type":"ribbon","title":"T","encoding":{"x":"x","y":"v","color":"g"}})
}

fn bars(g: &Geometry) -> Vec<Rect> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter_map(|i| match i {
            Item::Rect {
                rect, role: Role::Mark, ..
            } => Some(*rect),
            _ => None,
        })
        .collect()
}

fn ribbons(g: &Geometry) -> Vec<Rect> {
    g.panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| matches!(i, Item::Area { .. }) && i.role() == Role::Band)
        .filter_map(Item::bounds)
        .collect()
}

fn segment<'a>(g: &'a Geometry, category: &str, group: &str) -> &'a Observation {
    observations(g)
        .into_iter()
        .find(|o| o.readout.heading == category && o.readout.lines[0].name == group)
        .unwrap_or_else(|| panic!("no segment for {group} in {category}"))
}

#[test]
fn the_golden_chart_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn a_bar_for_each_category_and_group_and_a_ribbon_between_neighbours() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    assert_eq!(bars(&g).len(), 20);
    assert_eq!(ribbons(&g).len(), 16);
    assert_eq!(observations(&g).len(), 20);
    assert!(g.legend.is_some());
}

#[test]
fn groups_are_ranked_largest_on_top_and_the_ranks_are_derived() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let y = |c: &str, r: &str| segment(&g, c, r).at[1];
    // January: North 50, South 42, East 35, West 20, from the top.
    assert!(y("Jan", "North") < y("Jan", "South") && y("Jan", "South") < y("Jan", "East"));
    assert!(y("Jan", "East") < y("Jan", "West"));
    // March: South passes North.
    assert!(y("Mar", "South") < y("Mar", "North"));
    let north = segment(&g, "Mar", "North");
    assert_eq!(line(north, "Rank"), Some("2 of 4"));
    let rank = north.readout.lines.iter().find(|l| l.name == "Rank").unwrap();
    assert!(rank.derived);
    assert!(north.readout.lines.iter().find(|l| l.name == "Share").unwrap().derived);
    assert_eq!(north.readout.lines[0].value, "44");
    assert!(!north.readout.lines[0].derived);
}

#[test]
fn segments_stack_without_gaps_and_ribbons_start_at_the_bars() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let area = |c: &str, r: &str| segment(&g, c, r).area.unwrap();
    let (south, east) = (area("Jan", "South"), area("Jan", "East"));
    assert!((south.bottom() - east.y).abs() < 1e-6);
    assert!((south.x - east.x).abs() < 1e-9 && (south.w - east.w).abs() < 1e-9);
    let bar_rects = bars(&g);
    for r in ribbons(&g) {
        assert!(
            bar_rects.iter().any(|b| (b.right() - r.x).abs() < 1e-6),
            "a ribbon leaves the right edge of a bar"
        );
        assert!(
            bar_rects.iter().any(|b| (b.x - r.right()).abs() < 1e-6),
            "a ribbon reaches the left edge of a bar"
        );
    }
}

#[test]
fn a_group_missing_in_a_category_has_no_segment_and_no_ribbon_there() {
    let (spec, mut rows) = fixture();
    // The last row is West in May.
    rows.pop();
    let g = draw(&spec, &rows);
    assert_eq!(bars(&g).len(), 19);
    assert_eq!(ribbons(&g).len(), 15);
}

#[test]
fn a_zero_is_a_hairline_with_a_reading_and_no_ribbon() {
    let (spec, mut rows) = fixture();
    // West in February.
    let at = rows
        .iter()
        .position(|r| r["region"] == "West" && r["month"] == "Feb")
        .unwrap();
    rows[at]["sales"] = json!(0);
    let g = draw(&spec, &rows);
    assert_eq!(bars(&g).len(), 20);
    assert_eq!(ribbons(&g).len(), 14);
    let zero = segment(&g, "Feb", "West");
    assert_eq!(zero.readout.lines[0].value, "0");
    assert!(zero.area.unwrap().h <= 1.0 + 1e-9);
    contract(&spec, &rows);
}

#[test]
fn values_are_printed_in_the_bars_that_have_room() {
    let (spec, rows) = fixture();
    let g = draw_at(&spec, &rows, 1440.0);
    let labels = g
        .panels
        .iter()
        .flat_map(|p| &p.items)
        .filter(|i| matches!(i, Item::Text(t) if t.role == Role::Label))
        .count();
    assert_eq!(labels, 20);
}

#[test]
fn negative_values_repeats_and_text_fail_the_panel() {
    let (spec, mut rows) = fixture();
    rows[1]["sales"] = json!(-2);
    let g = draw(&spec, &rows);
    assert!(g.failure.as_deref().unwrap().contains("negative"));
    assert!(check(&g, "source-error").unwrap().message.contains("Row 2"));
    let (spec, mut rows) = fixture();
    rows.push(rows[0].clone());
    let g = draw(&spec, &rows);
    assert!(g.failure.as_deref().unwrap().contains("Repeated"));
    let (spec, mut rows) = fixture();
    rows[3]["sales"] = json!("lots");
    let g = draw(&spec, &rows);
    assert!(g.failure.is_some() && check(&g, "non-numeric").is_some());
}

#[test]
fn rows_without_a_value_are_disclosed_not_zeros() {
    let (spec, mut rows) = fixture();
    rows[0]["sales"] = Value::Null;
    let g = draw(&spec, &rows);
    assert_eq!(check(&g, "excluded-observations").unwrap().details["count"], 1);
    assert_eq!(observations(&g).len(), 19);
}

#[test]
fn a_horizontal_request_is_ignored_with_a_note() {
    let (mut spec, rows) = fixture();
    spec["style"] = json!({"orientation": "horizontal"});
    let g = draw(&spec, &rows);
    assert!(check(&g, "style-invalid").is_some());
    assert_eq!(bars(&g).len(), 20);
    contract(&spec, &rows);
}

#[test]
fn one_group_and_one_category_still_draw() {
    let spec = json!({"type":"ribbon","title":"T","encoding":{"x":"x","y":"v"}});
    let rows = [
        json!({"x": "A", "v": 3}),
        json!({"x": "B", "v": 5}),
        json!({"x": "C", "v": 4}),
    ];
    let g = draw(&spec, &rows);
    assert_eq!(bars(&g).len(), 3);
    assert_eq!(ribbons(&g).len(), 2);
    assert!(g.legend.is_none());
    contract(&spec, &rows);
    contract(
        &plain(),
        &[json!({"x": "A", "g": "P", "v": 3}), json!({"x": "A", "g": "Q", "v": 1})],
    );
}

#[test]
fn facets_each_get_their_own_bars() {
    let spec = json!({"type":"ribbon","title":"T","encoding":{"x":"x","y":"v","color":"g","facet":"f"}});
    let mut rows = Vec::new();
    for f in ["North", "South"] {
        for x in ["A", "B", "C"] {
            for (k, g) in ["P", "Q"].iter().enumerate() {
                rows.push(json!({"x": x, "g": g, "v": 1 + k + x.len(), "f": f}));
            }
        }
    }
    let g = draw(&spec, &rows);
    assert_eq!(g.panels.len(), 2);
    assert_eq!(bars(&g).len(), 12);
    contract(&spec, &rows);
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, rows) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &rows[..1]);
    let zeros = [json!({"x": "A", "g": "P", "v": 0}), json!({"x": "B", "g": "P", "v": 0})];
    let g = draw(&plain(), &zeros);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let huge = [
        json!({"x": "A", "g": "P", "v": 1e300}),
        json!({"x": "A", "g": "Q", "v": 1e300}),
        json!({"x": "B", "g": "P", "v": 1e300}),
        json!({"x": "B", "g": "Q", "v": 1.0}),
    ];
    let g = draw(&plain(), &huge);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
    let overflow = [
        json!({"x": "A", "g": "P", "v": 1.5e308}),
        json!({"x": "A", "g": "Q", "v": 1.5e308}),
    ];
    let g = draw(&plain(), &overflow);
    assert!(g.failure.is_some() && check(&g, "value-overflow").is_some());
    let dense: Vec<Value> = (0..40)
        .flat_map(|i| {
            (0..6).map(
                move |k| json!({"x": format!("Category {i}"), "g": format!("Group {k}"), "v": 1 + (i * (k + 2)) % 9}),
            )
        })
        .collect();
    let g = draw_at(&plain(), &dense, 320.0);
    assert!(g.failure.is_none() && nonfinite(&g).is_none());
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("ribbon", &spec, &rows);
}

#[test]
fn the_runtime_keeps_the_ranks_and_shares_of_values_a_grid_step_apart() {
    // Two groups tie within a grid step in the first category and swap in the second; one tall
    // stack sets the axis.
    let mut rows = Vec::new();
    for (x, a, b, c) in [
        ("Q1", 100.04, 100.01, 10.0),
        ("Q2", 100.01, 100.04, 12.0),
        ("Q3", 900.0, 100.02, 14.0),
    ] {
        for (g, v) in [("A", a), ("B", b), ("C", c)] {
            rows.push(json!({"x": x, "v": v, "g": g}));
        }
    }
    runtime_matches_native(&plain(), &rows);
}
